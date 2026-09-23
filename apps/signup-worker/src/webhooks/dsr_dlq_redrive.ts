/**
 * Operator-only recovery for a terminal DSR DLQ receipt.
 *
 * The recovery envelope deliberately has a smaller data surface than the
 * queue message.  In particular it never stores the Clerk principal, a salt,
 * raw JSON, provider responses, or a caller-supplied tenant.  This keeps a
 * later operator redrive tenant-bound without turning the receipt into a
 * second DSR payload store.
 */
import { deriveErasureSalt } from "./clerk_erasure.js";
import { constantTimeEqual } from "./github_provision.js";

const MAX_REQUEUE_COUNT = 1;
const REDRIVE_ENVELOPE_RETENTION_MS = 7 * 24 * 60 * 60_000;
const REDRIVE_AUDIT_RETENTION_MS = 30 * 24 * 60 * 60_000;
const REDRIVE_CLAIM_LEASE_MS = 5 * 60_000;
const MAX_REDRIVE_BODY_BYTES = 160;

export interface D1RedriveStatement {
  bind(...values: unknown[]): D1RedriveStatement;
  first<T = unknown>(): Promise<T | null>;
  run(): Promise<{ success: boolean; meta?: { changes?: number } }>;
}

export interface D1RedriveDatabase {
  prepare(sql: string): D1RedriveStatement;
}

export interface DsrDlqRecoveryEnvelopeInput {
  eventId: string;
  dsrId: string;
  tenantId: string;
  queuedAtMs: number;
  legalHold: boolean;
  requeueCount: number;
}

export type DsrDlqRedriveClaim = "claimed" | "expired" | "not_ready";

/** The only persistence authority used by the DLQ consumer and redrive route. */
export interface DsrDlqRecoveryStore {
  capture(input: DsrDlqRecoveryEnvelopeInput, nowMs: number): Promise<void>;
  makeReady(eventId: string, nowMs: number): Promise<void>;
  close(eventId: string, nowMs: number): Promise<void>;
  claim(
    eventId: string,
    actorRef: string,
    approvalRef: string,
    nowMs: number,
  ): Promise<DsrDlqRedriveClaim>;
  claimedEnvelope(eventId: string): Promise<ClaimedEnvelope | null>;
  /** Durable ambiguous intent is the fence immediately before Queue.send. */
  prepareDispatch(eventId: string, nowMs: number): Promise<boolean>;
  submitted(eventId: string, nowMs: number): Promise<void>;
  ambiguous(eventId: string, nowMs: number): Promise<void>;
  cleanup(nowMs: number): Promise<void>;
}

export interface ClaimedEnvelope {
  dsr_id: string;
  tenant_id: string;
  queued_at_ms: number;
  legal_hold: number;
  requeue_count: number;
}

function isCanonicalUuid(value: string): boolean {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(value);
}

export function isOpaqueDlqEventId(value: unknown): value is string {
  return typeof value === "string" && /^dsr-erasure-dlq:[0-9a-f]{64}$/.test(value);
}

function isOpaqueReference(value: string, prefix: "op_" | "apr_"): boolean {
  return value.length >= prefix.length + 1
    && value.length <= 96
    && value.startsWith(prefix)
    && /^[A-Za-z0-9._:-]+$/.test(value);
}

function isCaptureInput(input: DsrDlqRecoveryEnvelopeInput): boolean {
  return isOpaqueDlqEventId(input.eventId)
    && isCanonicalUuid(input.dsrId)
    && isCanonicalUuid(input.tenantId)
    && Number.isSafeInteger(input.queuedAtMs)
    && input.queuedAtMs >= 0
    && typeof input.legalHold === "boolean"
    && (input.requeueCount === 0 || input.requeueCount === MAX_REQUEUE_COUNT);
}

/** D1 implementation. Every stored column is part of the migration allowlist. */
export class D1DsrDlqRecoveryStore implements DsrDlqRecoveryStore {
  constructor(private readonly db: D1RedriveDatabase) {}

  async capture(input: DsrDlqRecoveryEnvelopeInput, nowMs: number): Promise<void> {
    // A malformed queue payload must never proceed to a terminal ACK without
    // an envelope. Throwing makes the consumer retain it for repair.
    if (!isCaptureInput(input)) throw new Error("dsr_dlq_redrive_capture_invalid");
    const result = await this.db.prepare(
      "INSERT OR IGNORE INTO dsr_dlq_redrive_envelopes " +
        "(event_id, dsr_id, tenant_id, queued_at_ms, legal_hold, requeue_count, redrive_state, created_at_ms, expires_at_ms, claim_expires_at_ms, updated_at_ms) " +
        "VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'captured', ?7, ?8, 0, ?7)",
    ).bind(
      input.eventId,
      input.dsrId,
      input.tenantId,
      input.queuedAtMs,
      input.legalHold ? 1 : 0,
      input.requeueCount,
      nowMs,
      nowMs + REDRIVE_ENVELOPE_RETENTION_MS,
    ).run();
    if (!result.success) throw new Error("dsr_dlq_redrive_capture_failed");
  }

  async makeReady(eventId: string, nowMs: number): Promise<void> {
    const result = await this.db.prepare(
      "UPDATE dsr_dlq_redrive_envelopes SET redrive_state = 'ready', updated_at_ms = ?2 " +
        "WHERE event_id = ?1 AND redrive_state = 'captured' AND requeue_count = 0",
    ).bind(eventId, nowMs).run();
    if (!result.success) throw new Error("dsr_dlq_redrive_ready_failed");
  }

  async close(eventId: string, nowMs: number): Promise<void> {
    const result = await this.db.prepare(
      "UPDATE dsr_dlq_redrive_envelopes SET redrive_state = 'closed', updated_at_ms = ?2 " +
        "WHERE event_id = ?1 AND redrive_state IN ('captured', 'ready')",
    ).bind(eventId, nowMs).run();
    if (!result.success) throw new Error("dsr_dlq_redrive_close_failed");
  }

  async claim(eventId: string, actorRef: string, approvalRef: string, nowMs: number): Promise<DsrDlqRedriveClaim> {
    const result = await this.db.prepare(
      "UPDATE dsr_dlq_redrive_envelopes SET redrive_state = 'claimed', actor_ref = ?2, approval_ref = ?3, " +
        "claim_expires_at_ms = ?4, updated_at_ms = ?5 " +
        "WHERE event_id = ?1 AND redrive_state = 'ready' AND requeue_count = 0 AND expires_at_ms > ?5",
    ).bind(eventId, actorRef, approvalRef, nowMs + REDRIVE_CLAIM_LEASE_MS, nowMs).run();
    if (!result.success) throw new Error("dsr_dlq_redrive_claim_failed");
    if (result.meta?.changes === 1) return "claimed";
    const row = await this.db.prepare(
      "SELECT expires_at_ms FROM dsr_dlq_redrive_envelopes WHERE event_id = ?1",
    ).bind(eventId).first<{ expires_at_ms: number }>();
    return row && row.expires_at_ms <= nowMs ? "expired" : "not_ready";
  }

  async claimedEnvelope(eventId: string): Promise<ClaimedEnvelope | null> {
    return this.db.prepare(
      "SELECT dsr_id, tenant_id, queued_at_ms, legal_hold, requeue_count FROM dsr_dlq_redrive_envelopes " +
        "WHERE event_id = ?1 AND redrive_state = 'claimed'",
    ).bind(eventId).first<ClaimedEnvelope>();
  }

  async prepareDispatch(eventId: string, nowMs: number): Promise<boolean> {
    const result = await this.db.prepare(
      "UPDATE dsr_dlq_redrive_envelopes SET redrive_state = 'ambiguous', updated_at_ms = ?2 " +
        "WHERE event_id = ?1 AND redrive_state = 'claimed' AND claim_expires_at_ms > ?2",
    ).bind(eventId, nowMs).run();
    if (!result.success) throw new Error("dsr_dlq_redrive_dispatch_fence_failed");
    return result.meta?.changes === 1;
  }

  private async transition(eventId: string, state: "submitted", nowMs: number): Promise<void> {
    const result = await this.db.prepare(
      "UPDATE dsr_dlq_redrive_envelopes SET redrive_state = ?2, claim_expires_at_ms = 0, updated_at_ms = ?3 " +
        "WHERE event_id = ?1 AND redrive_state = 'ambiguous'",
    ).bind(eventId, state, nowMs).run();
    if (!result.success || result.meta?.changes !== 1) throw new Error(`dsr_dlq_redrive_${state}_failed`);
  }

  async submitted(eventId: string, nowMs: number): Promise<void> {
    await this.transition(eventId, "submitted", nowMs);
  }

  async ambiguous(eventId: string, nowMs: number): Promise<void> {
    const result = await this.db.prepare(
      "UPDATE dsr_dlq_redrive_envelopes SET redrive_state = 'ambiguous', claim_expires_at_ms = 0, updated_at_ms = ?2 " +
        "WHERE event_id = ?1 AND redrive_state = 'claimed'",
    ).bind(eventId, nowMs).run();
    if (!result.success) throw new Error("dsr_dlq_redrive_ambiguous_failed");
  }

  async cleanup(nowMs: number): Promise<void> {
    const markStaleClaim = await this.db.prepare(
      "UPDATE dsr_dlq_redrive_envelopes SET redrive_state = 'ambiguous', claim_expires_at_ms = 0, updated_at_ms = ?1 " +
        "WHERE redrive_state = 'claimed' AND claim_expires_at_ms > 0 AND claim_expires_at_ms <= ?1",
    ).bind(nowMs).run();
    if (!markStaleClaim.success) throw new Error("dsr_dlq_redrive_claim_cleanup_failed");
    const purgeAudit = await this.db.prepare(
      "DELETE FROM dsr_dlq_redrive_audit WHERE occurred_at_ms <= ?1",
    ).bind(nowMs - REDRIVE_AUDIT_RETENTION_MS).run();
    if (!purgeAudit.success) throw new Error("dsr_dlq_redrive_audit_cleanup_failed");
    const purgeEnvelope = await this.db.prepare(
      "DELETE FROM dsr_dlq_redrive_envelopes WHERE expires_at_ms <= ?1 AND redrive_state <> 'claimed'",
    ).bind(nowMs).run();
    if (!purgeEnvelope.success) throw new Error("dsr_dlq_redrive_envelope_cleanup_failed");
  }
}

export interface DsrDlqRedriveEnv {
  /** Dedicated credential: no shared or queue credential is accepted here. */
  DSR_DLQ_REDRIVE_AUTH_KEY?: string;
  ERASURE_SALT_KEY?: string;
  ENVIRONMENT?: string;
  DSR_QUEUE?: { send(message: unknown): Promise<void> };
  CONFIG_DB?: D1RedriveDatabase;
  DSR_DLQ_RECOVERY?: DsrDlqRecoveryStore;
}

function json(status: number, body: Record<string, string>): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}

async function parseEventId(request: Request): Promise<string | null> {
  const raw = await request.text();
  if (new TextEncoder().encode(raw).byteLength > MAX_REDRIVE_BODY_BYTES) return null;
  let body: unknown;
  try {
    body = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!body || typeof body !== "object" || Array.isArray(body)) return null;
  const record = body as Record<string, unknown>;
  if (Object.keys(record).length !== 1 || !Object.prototype.hasOwnProperty.call(record, "event_id")) return null;
  return isOpaqueDlqEventId(record.event_id) ? record.event_id : null;
}

/**
 * POST /internal/dsr/dlq/redrive
 *
 * The caller may name only an opaque receipt.  Tenant, legal hold, timestamp,
 * requeue count, and salt always come from the bounded envelope or fixed wire
 * constants.  A caller therefore cannot substitute a tenant into a redrive.
 */
export async function handleDsrDlqRedrive(request: Request, env: DsrDlqRedriveEnv): Promise<Response> {
  if (request.method !== "POST") return json(405, { error: "method_not_allowed" });
  const expected = env.DSR_DLQ_REDRIVE_AUTH_KEY?.trim();
  if (!expected || expected.length < 32) return json(503, { error: "unavailable" });
  const authorization = request.headers.get("authorization") ?? "";
  const bearer = "Bearer ";
  const presented = authorization.startsWith(bearer) ? authorization.slice(bearer.length) : "";
  if (!constantTimeEqual(presented, expected)) return json(401, { error: "unauthorized" });
  const actorRef = request.headers.get("x-corelink-operator-ref") ?? "";
  const approvalRef = request.headers.get("x-corelink-approval-ref") ?? "";
  if (!isOpaqueReference(actorRef, "op_") || !isOpaqueReference(approvalRef, "apr_")) {
    return json(400, { error: "invalid_operator_reference" });
  }
  const eventId = await parseEventId(request);
  if (!eventId) return json(400, { error: "invalid_receipt" });
  const store = env.DSR_DLQ_RECOVERY ?? (env.CONFIG_DB ? new D1DsrDlqRecoveryStore(env.CONFIG_DB) : undefined);
  if (!store || !env.DSR_QUEUE) return json(503, { error: "unavailable" });

  const nowMs = Date.now();
  let claim: DsrDlqRedriveClaim;
  try {
    claim = await store.claim(eventId, actorRef, approvalRef, nowMs);
  } catch {
    return json(503, { error: "unavailable" });
  }
  if (claim === "expired") return json(410, { error: "receipt_expired" });
  if (claim !== "claimed") return json(409, { error: "receipt_not_redriveable" });

  // A claimed row remains the durable outcome if salt derivation itself fails.
  // It is never released for an automatic second attempt.
  let envelope: ClaimedEnvelope | null;
  try {
    envelope = await store.claimedEnvelope(eventId);
  } catch {
    return json(503, { error: "unavailable" });
  }
  if (!envelope || envelope.requeue_count !== 0 || !isCanonicalUuid(envelope.dsr_id) || !isCanonicalUuid(envelope.tenant_id)) {
    try { await store.ambiguous(eventId, Date.now()); } catch { return json(503, { error: "unavailable" }); }
    return json(409, { error: "receipt_not_redriveable" });
  }

  let salt: string;
  try {
    salt = await deriveErasureSalt(envelope.dsr_id, env.ERASURE_SALT_KEY, env.ENVIRONMENT);
  } catch {
    return json(503, { error: "unavailable" });
  }
  // This irreversible fence is durable BEFORE Queue.send. Once it succeeds,
  // cleanup leaves the record operator-visible as ambiguous until a successful
  // submission can advance it; no lease expiry can reopen or race a send.
  try {
    if (!await store.prepareDispatch(eventId, Date.now())) {
      return json(409, { error: "receipt_not_redriveable" });
    }
  } catch {
    return json(503, { error: "unavailable" });
  }
  const message = {
    schema: "dev.hugr.corelink.dsr.queued.v1" as const,
    dsr_id: envelope.dsr_id,
    tenant_id: envelope.tenant_id,
    subject_id: envelope.tenant_id,
    erasure_salt_hex: salt,
    queued_at_ms: envelope.queued_at_ms,
    legal_hold: envelope.legal_hold === 1,
    source: "clerk.user.deleted" as const,
    _dlq_requeue: MAX_REQUEUE_COUNT,
  };
  try {
    await env.DSR_QUEUE.send(message);
  } catch {
    try { await store.ambiguous(eventId, Date.now()); } catch { return json(503, { error: "unavailable" }); }
    return json(502, { error: "redrive_ambiguous" });
  }
  try {
    await store.submitted(eventId, Date.now());
  } catch {
    try { await store.ambiguous(eventId, Date.now()); } catch { return json(503, { error: "unavailable" }); }
    return json(502, { error: "redrive_ambiguous" });
  }
  return json(202, { status: "submitted" });
}

/** Bounded scheduled retention cleanup; failed cleanup is surfaced to Sentry. */
export async function runDsrDlqRedriveCleanup(env: DsrDlqRedriveEnv, nowMs: number): Promise<void> {
  const store = env.DSR_DLQ_RECOVERY ?? (env.CONFIG_DB ? new D1DsrDlqRecoveryStore(env.CONFIG_DB) : undefined);
  if (store) await store.cleanup(nowMs);
}
