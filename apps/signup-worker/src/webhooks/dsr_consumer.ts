/**
 * DSR erasure queue consumer (WI-S11-008 WP-G).
 *
 * Cloudflare Queue consumer for `dsr.queued.v1` messages produced by the Clerk
 * `user.deleted` webhook (see `clerk.ts::handleUserDeleted`). Each message is
 * forwarded to the container's internal erasure endpoint
 * (`POST /_internal/dsr/erase`, gated by `CORELINK_INTERNAL_AUTH_KEY`), which
 * runs the 12-backend erasure orchestrator. This mirrors the existing
 * container-internal call pattern used for `/_internal/pat/mint`.
 *
 * Reliability (M2 — poison-message classification): the container response is
 * CLASSIFIED, not blanket-retried. A 2xx → `ack()`. A *permanent* 4xx
 * (malformed body 400/409, or erasure REJECTED 422) can NEVER succeed on
 * redelivery → `ack()` it as a poison message (with a loud structured
 * `console.error`) rather than burning the full 10-retry budget then silently
 * dead-lettering. Only genuinely transient faults — 5xx, transport/network
 * errors, and self-healing misconfig (401/403/404 auth-key/route, 408/429
 * backpressure) — `retry()`, because dropping those would abandon a GDPR
 * Art.17 erasure obligation. The orchestrator is idempotent (dedup per
 * `(dsr_id, backend)`), so a redelivered message never double-erases.
 */

import type { DsrQueuedV1 } from "./clerk.js";
import { resolveEraseAuthKey } from "../lib/erase-auth-key.js";
import { deriveErasureSalt } from "./clerk_erasure.js";
import { constantTimeEqual } from "./github_provision.js";

/** Minimal env surface the consumer needs (kept independent of the full Worker Env). */
export interface DsrConsumerEnv {
  CORELINK_API_BASE: string;
  /**
   * Dedicated erase/DSR consumer secret (red-team #3 per-consumer split).
   * Preferred for the `x-corelink-internal-auth` header on the erase call so
   * it matches the main-Worker / container erase gate in the full-split config
   * (rt-nuclear #23); falls back to the shared key below.
   */
  CORELINK_ERASE_AUTH_KEY?: string;
  /** Shared secret for the `x-corelink-internal-auth` header on the container call. */
  CORELINK_INTERNAL_AUTH_KEY?: string;
  /** Service binding to the main CoreLink Worker (bypasses CF edge error 1014). */
  CORELINK_API_SVC?: { fetch: typeof fetch };
}

/** Minimal Cloudflare Queue message surface (test-independent of workers-types). */
export interface QueueMessage<T> {
  readonly body: T;
  /** Cloudflare delivery attempt number; absent only in narrow unit-test fakes. */
  readonly attempts?: number;
  ack(): void;
  retry(): void;
}
export interface QueueMessageBatch<T> {
  readonly messages: ReadonlyArray<QueueMessage<T>>;
}

/**
 * Forward ONE erasure request to the container's internal erasure endpoint.
 * Returns `{ ok, status }` — the batch handler classifies `status` via
 * `classifyErasureStatus` into ack-success / ack-poison / retry. NEVER throws
 * on a transport error (caught and reported as `{ ok:false, status:0 }`, which
 * classifies as `retry`).
 */
export async function processErasureMessage(
  msg: DsrQueuedV1,
  env: DsrConsumerEnv,
): Promise<{ ok: boolean; status: number }> {
  // rt-nuclear #23: send the ERASE consumer key (shared-key fallback) so the
  // header matches the main-Worker / container erase gate in the full-split
  // config. Resolution mirrors `erase_auth_key_from_env()` + the main Worker's
  // `resolveConsumerKey(env, "erase")`.
  const eraseAuthKey = resolveEraseAuthKey(env);
  if (!eraseAuthKey) {
    // Cannot authenticate to the container — retry rather than DROP a GDPR
    // erasure obligation (self-heals once the secret is bound).
    console.error(
      `[dsr-consumer] no erase/internal auth key bound — retrying erasure dsr_id=${msg.dsr_id}`,
    );
    return { ok: false, status: 0 };
  }
  const req = new Request(`${env.CORELINK_API_BASE}/_internal/dsr/erase`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-corelink-internal-auth": eraseAuthKey,
    },
    body: JSON.stringify(msg),
  });
  try {
    const resp = env.CORELINK_API_SVC
      ? await env.CORELINK_API_SVC.fetch(req)
      : await fetch(req);
    return { ok: resp.ok, status: resp.status };
  } catch (err) {
    console.error(
      `[dsr-consumer] erasure call threw dsr_id=${msg.dsr_id}: ${(err as Error).message.slice(0, 120)}`,
    );
    return { ok: false, status: 0 };
  }
}

/** Queue disposition for one erasure attempt (M2). */
export type ErasureDisposition = "ack-success" | "ack-poison" | "retry";

/**
 * Classify a container erase HTTP status into a queue disposition (M2).
 *
 * - `2xx` → `ack-success`.
 * - PERMANENT 4xx — malformed/duplicate body (`400`/`409`) or erasure REJECTED
 *   by the legitimacy gate (`422`) — → `ack-poison`: redelivery can never
 *   change the outcome, so we consume it (with a loud log) instead of burning
 *   the 10-retry budget then silently dead-lettering.
 * - Self-healing misconfig (`401`/`403` auth-key, `404` route-not-mounted) and
 *   backpressure (`408`/`429`) → `retry`: these clear once the secret binds /
 *   route mounts / limit resets; dropping them would abandon a GDPR Art.17
 *   obligation.
 * - `5xx`, transport/network faults (`status === 0`), and any unknown status →
 *   `retry` (genuinely transient; fail toward NOT dropping the obligation).
 */
export function classifyErasureStatus(status: number): ErasureDisposition {
  if (status >= 200 && status < 300) return "ack-success";
  if (status === 0) return "retry"; // transport / network fault
  if (status >= 500) return "retry"; // transient server fault
  if (status === 401 || status === 403 || status === 404 || status === 408 || status === 429) {
    return "retry"; // auth/route misconfig or backpressure — self-heals
  }
  if (status >= 400 && status < 500) return "ack-poison"; // permanent reject/malformed
  return "retry"; // unknown — fail toward not dropping the erasure obligation
}

/**
 * Process a batch of `dsr.queued.v1` messages. Each is classified
 * (`classifyErasureStatus`): acked on success, acked-as-poison (loud log) on a
 * permanent 4xx, retried on a transient fault. Per-message isolation: one
 * failure never blocks the rest of the batch.
 */
export async function handleErasureQueueBatch(
  batch: QueueMessageBatch<DsrQueuedV1>,
  env: DsrConsumerEnv,
): Promise<void> {
  for (const m of batch.messages) {
    const r = await processErasureMessage(m.body, env);
    const disposition = classifyErasureStatus(r.status);
    if (disposition === "retry") {
      m.retry();
      continue;
    }
    if (disposition === "ack-poison") {
      // PERMANENT failure — consume (don't retry) but make it loudly visible.
      console.error(
        JSON.stringify({
          level: "error",
          severity: "high",
          event: "dsr.erasure.poison",
          component: "dsr-consumer",
          status: r.status,
          dsr_id: m.body.dsr_id,
          tenant_id: m.body.tenant_id,
          action: "ack_poison",
          note: "permanent 4xx erasure failure — acked WITHOUT retry; manual review of this GDPR Art.17 request required",
        }),
      );
    }
    m.ack();
  }
}

// ── M3: dead-letter-queue handler (corelink-dsr-erasure-dlq) ──────────────────

/**
 * Body of a re-enqueued erasure: the original `dsr.queued.v1` plus a bounded
 * re-enqueue counter. The container's `DsrQueuedV1` deserializer accepts and
 * ignores unknown fields, so this extra marker is safe on the erase call.
 */
export type DsrDlqBody = DsrQueuedV1 & { _dlq_requeue?: number };

const MAX_DLQ_REQUEUES = 1;
const MAX_DLQ_PAGING_ATTEMPTS = 3;
const PAGERDUTY_EVENTS_V2_URL = "https://events.pagerduty.com/v2/enqueue";
const DLQ_EVENT_NAME = "dsr.erasure.dead_letter";

type DlqReceiptStatus =
  | "paging_pending"
  | "paging_claimed"
  | "paging_retry"
  | "paging_ambiguous"
  | "paging_recovered"
  | "paged"
  | "requeue_claimed"
  | "requeued"
  | "terminal"
  | "delivery_exhausted"
  | "requeue_ambiguous";

const DLQ_RECEIPT_STATUSES = new Set<DlqReceiptStatus>([
  "paging_pending", "paging_claimed", "paging_retry", "paging_ambiguous",
  "paging_recovered", "paged", "requeue_claimed", "requeued", "terminal",
  "delivery_exhausted", "requeue_ambiguous",
]);

export interface DsrDlqReceipt {
  status: DlqReceiptStatus;
  paging_claimed: number;
  requeue_claimed: number;
}

function isValidDlqReceipt(value: DsrDlqReceipt): boolean {
  return DLQ_RECEIPT_STATUSES.has(value.status)
    && (value.paging_claimed === 0 || value.paging_claimed === 1)
    && (value.requeue_claimed === 0 || value.requeue_claimed === 1);
}

export interface DsrDlqReceiptStore {
  find(eventId: string): Promise<DsrDlqReceipt | null>;
  record(eventId: string, status: DlqReceiptStatus, nowMs: number): Promise<void>;
  claimPaging(eventId: string, nowMs: number): Promise<boolean>;
  claimRequeue(eventId: string, nowMs: number): Promise<boolean>;
}

interface D1Statement {
  bind(...values: unknown[]): D1Statement;
  first<T = unknown>(): Promise<T | null>;
  run(): Promise<D1RunResult>;
}

interface D1RunResult {
  success: boolean;
  meta?: { changes?: number };
}

interface D1ReceiptDatabase {
  prepare(sql: string): D1Statement;
  batch(statements: D1Statement[]): Promise<D1RunResult[]>;
}

const REDRIVE_TTL_MS = 7 * 24 * 60 * 60_000;
const REDRIVE_LEASE_MS = 5 * 60_000;
const EVENT_ID_RE = /^dsr-erasure-dlq:[0-9a-f]{64}$/;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;
const CAPTURE_ACTOR_REF = "system:dlq-capture";
const CAPTURE_APPROVAL_REF = "not-applicable";

interface RedriveEnvelope {
  dsr_id: string;
  tenant_id: string;
  queued_at_ms: number;
  legal_hold: number;
  requeue_count: number;
}

export interface RedriveStore {
  capture(eventId: string, body: DsrDlqBody, nowMs: number): Promise<boolean>;
  claim(eventId: string, actorRef: string, approvalRef: string, nowMs: number): Promise<"claimed" | "expired" | "denied">;
  envelope(eventId: string): Promise<RedriveEnvelope | null>;
  fence(eventId: string, nowMs: number): Promise<boolean>;
  submitted(eventId: string, nowMs: number): Promise<void>;
  ambiguous(eventId: string, nowMs: number): Promise<void>;
  cleanup(nowMs: number): Promise<void>;
}

/** Stores only the migration 0145 recovery allowlist, never the queue body. */
export class D1RedriveStore implements RedriveStore {
  constructor(private readonly db: D1ReceiptDatabase) {}

  /**
   * D1 batch is transactional. The audit insert is conditional on the new
   * state, so a denied claim/fence cannot manufacture a transition record.
   */
  private async transition(
    eventId: string,
    transition: "claimed" | "submitted" | "ambiguous",
    update: D1Statement,
  ): Promise<boolean> {
    const audit = this.db.prepare(
      "INSERT OR IGNORE INTO dsr_dlq_redrive_audit (event_id, transition) SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM dsr_dlq_redrive_envelopes WHERE event_id=?1 AND state=?2)",
    ).bind(eventId, transition);
    const [result] = await this.db.batch([update, audit]);
    if (!result?.success) throw new Error(`dsr_dlq_redrive_${transition}_failed`);
    return result.meta?.changes === 1;
  }

  async capture(eventId: string, body: DsrDlqBody, nowMs: number): Promise<boolean> {
    const requeueCount = normalizedDlqRequeueCount(body._dlq_requeue);
    if (!EVENT_ID_RE.test(eventId) || !UUID_RE.test(body.dsr_id) || !UUID_RE.test(body.tenant_id)
      || !Number.isSafeInteger(body.queued_at_ms) || body.queued_at_ms < 0
      || typeof body.legal_hold !== "boolean" || body.subject_id !== body.tenant_id) return false;
    const result = await this.db.prepare(
      "INSERT OR IGNORE INTO dsr_dlq_redrive_envelopes (event_id, dsr_id, tenant_id, queued_at_ms, legal_hold, requeue_count, state, actor_ref, approval_ref, expires_at_ms, claim_expires_at_ms, updated_at_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'ready', ?7, ?8, ?9, 0, ?10)",
    ).bind(eventId, body.dsr_id, body.tenant_id, body.queued_at_ms, body.legal_hold ? 1 : 0,
      requeueCount, CAPTURE_ACTOR_REF, CAPTURE_APPROVAL_REF, nowMs + REDRIVE_TTL_MS, nowMs).run();
    if (!result.success) throw new Error("dsr_dlq_redrive_capture_failed");
    return true;
  }

  async claim(eventId: string, actorRef: string, approvalRef: string, nowMs: number): Promise<"claimed" | "expired" | "denied"> {
    const claimed = await this.transition(eventId, "claimed", this.db.prepare(
      "UPDATE dsr_dlq_redrive_envelopes SET state='claimed', actor_ref=?2, approval_ref=?3, claim_expires_at_ms=?4, updated_at_ms=?5 WHERE event_id=?1 AND state='ready' AND requeue_count=0 AND expires_at_ms>?5",
    ).bind(eventId, actorRef, approvalRef, nowMs + REDRIVE_LEASE_MS, nowMs));
    if (claimed) return "claimed";
    const row = await this.db.prepare("SELECT expires_at_ms FROM dsr_dlq_redrive_envelopes WHERE event_id=?1")
      .bind(eventId).first<{ expires_at_ms: number }>();
    return row && row.expires_at_ms <= nowMs ? "expired" : "denied";
  }

  async envelope(eventId: string): Promise<RedriveEnvelope | null> {
    return this.db.prepare("SELECT dsr_id, tenant_id, queued_at_ms, legal_hold, requeue_count FROM dsr_dlq_redrive_envelopes WHERE event_id=?1 AND state='claimed'")
      .bind(eventId).first<RedriveEnvelope>();
  }

  async fence(eventId: string, nowMs: number): Promise<boolean> {
    return this.transition(eventId, "ambiguous", this.db.prepare(
      "UPDATE dsr_dlq_redrive_envelopes SET state='ambiguous', updated_at_ms=?2 WHERE event_id=?1 AND state='claimed' AND claim_expires_at_ms>?2 AND expires_at_ms>?2",
    ).bind(eventId, nowMs));
  }

  async submitted(eventId: string, nowMs: number): Promise<void> {
    const submitted = await this.transition(eventId, "submitted", this.db.prepare(
      "UPDATE dsr_dlq_redrive_envelopes SET state='submitted', updated_at_ms=?2 WHERE event_id=?1 AND state='ambiguous'",
    ).bind(eventId, nowMs));
    if (!submitted) throw new Error("dsr_dlq_redrive_submit_failed");
  }

  async ambiguous(eventId: string, nowMs: number): Promise<void> {
    const ambiguous = await this.transition(eventId, "ambiguous", this.db.prepare(
      "UPDATE dsr_dlq_redrive_envelopes SET state='ambiguous', updated_at_ms=?2 WHERE event_id=?1 AND state='claimed'",
    ).bind(eventId, nowMs));
    if (!ambiguous) throw new Error("dsr_dlq_redrive_ambiguous_failed");
  }

  async cleanup(nowMs: number): Promise<void> {
    const leaseExpiry = this.db.prepare(
      "UPDATE dsr_dlq_redrive_envelopes SET state='ambiguous', updated_at_ms=?1 WHERE state='claimed' AND claim_expires_at_ms<=?1",
    ).bind(nowMs);
    const leaseAudit = this.db.prepare(
      "INSERT OR IGNORE INTO dsr_dlq_redrive_audit (event_id, transition) SELECT event_id, 'ambiguous' FROM dsr_dlq_redrive_envelopes WHERE state='ambiguous' AND claim_expires_at_ms<=?1",
    ).bind(nowMs);
    const leaseResults = await this.db.batch([leaseExpiry, leaseAudit]);
    if (!leaseResults.every((result) => result.success)) throw new Error("dsr_dlq_redrive_cleanup_failed");
    // Frozen 0145 does not timestamp audit rows. Deleting them with their
    // seven-day envelope is stricter than the required 30-day maximum.
    await this.db.prepare("DELETE FROM dsr_dlq_redrive_audit WHERE event_id IN (SELECT event_id FROM dsr_dlq_redrive_envelopes WHERE expires_at_ms<=?1)")
      .bind(nowMs).run();
    await this.db.prepare("DELETE FROM dsr_dlq_redrive_envelopes WHERE expires_at_ms<=?1 AND state<>'claimed'")
      .bind(nowMs).run();
  }
}

class D1DsrDlqReceiptStore implements DsrDlqReceiptStore {
  constructor(private readonly db: D1ReceiptDatabase) {}

  async find(eventId: string): Promise<DsrDlqReceipt | null> {
    return this.db
      .prepare("SELECT status, paging_claimed, requeue_claimed FROM dsr_dlq_delivery_receipts WHERE event_id = ?1")
      .bind(eventId)
      .first<DsrDlqReceipt>();
  }

  async record(eventId: string, status: DlqReceiptStatus, nowMs: number): Promise<void> {
    const result = await this.db
      .prepare(
        "INSERT INTO dsr_dlq_delivery_receipts (event_id, status, updated_at_ms) VALUES (?1, ?2, ?3) " +
          "ON CONFLICT(event_id) DO UPDATE SET status = excluded.status, " +
          "paging_claimed = CASE WHEN excluded.status = 'paging_retry' THEN 0 ELSE paging_claimed END, " +
          "updated_at_ms = excluded.updated_at_ms",
      )
      .bind(eventId, status, nowMs)
      .run();
    if (!result.success) throw new Error("dsr_dlq_receipt_write_failed");
  }

  async claimPaging(eventId: string, nowMs: number): Promise<boolean> {
    const result = await this.db
      .prepare(
        "UPDATE dsr_dlq_delivery_receipts SET paging_claimed = 1, status = 'paging_claimed', updated_at_ms = ?2 " +
          "WHERE event_id = ?1 AND paging_claimed = 0",
      )
      .bind(eventId, nowMs)
      .run();
    if (!result.success) throw new Error("dsr_dlq_receipt_paging_claim_failed");
    return result.meta?.changes === 1;
  }

  async claimRequeue(eventId: string, nowMs: number): Promise<boolean> {
    const result = await this.db
      .prepare(
        "UPDATE dsr_dlq_delivery_receipts SET requeue_claimed = 1, status = 'requeue_claimed', updated_at_ms = ?2 " +
          "WHERE event_id = ?1 AND requeue_claimed = 0",
      )
      .bind(eventId, nowMs)
      .run();
    if (!result.success) throw new Error("dsr_dlq_receipt_claim_failed");
    return result.meta?.changes === 1;
  }
}

/**
 * Queue payloads are external input. Normalize the marker before making a
 * retry decision: NaN, fractions, negatives, and strings must never bypass
 * the one-requeue cap or produce misleading operator counts.
 */
function normalizedDlqRequeueCount(value: unknown): number {
  if (value === undefined) return 0;
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    return MAX_DLQ_REQUEUES;
  }
  return Math.min(value, MAX_DLQ_REQUEUES);
}

/**
 * Stable correlation for one DLQ generation. The deterministic value is
 * intentionally derived from the immutable DSR identity and the bounded
 * requeue generation, so redelivery of the same DLQ message keeps one event
 * id while the one allowed requeue gets a distinct id on a later DLQ arrival.
 */
async function dlqEventId(body: DsrDlqBody, requeueCount: number): Promise<string> {
  const dsrId = typeof body.dsr_id === "string" && body.dsr_id.length > 0 ? body.dsr_id : "unknown";
  const input = new TextEncoder().encode(`corelink/dsr-dlq/v1\u0000${dsrId}\u0000${requeueCount}`);
  const digest = await crypto.subtle.digest("SHA-256", input);
  return `dsr-erasure-dlq:${Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

export interface DsrDlqEnv {
  DSR_QUEUE?: { send(message: unknown): Promise<void> };
  PAGERDUTY_ROUTING_KEY?: string;
  PAGERDUTY_FETCH?: typeof fetch;
  CONFIG_DB?: D1ReceiptDatabase;
  DSR_DLQ_RECEIPTS?: DsrDlqReceiptStore;
  DSR_DLQ_REDRIVE_AUTH_KEY?: string;
  ERASURE_SALT_KEY?: string;
  ENVIRONMENT?: string;
  DSR_DLQ_REDRIVE?: RedriveStore;
}

function isRedriveStore(value: unknown): value is RedriveStore {
  return !!value && typeof (value as RedriveStore).capture === "function"
    && typeof (value as RedriveStore).claim === "function";
}

type PagingResult =
  | { status: "delivered" }
  | { status: "not_configured" }
  | { status: "failed"; error: "http_rejected" | "transport_error" };

/** Deliver one critical DLQ page through the canonical PagerDuty procedure. */
async function pageDlqEvent(
  body: DsrDlqBody,
  eventId: string,
  requeueCount: number,
  env: DsrDlqEnv,
): Promise<PagingResult> {
  const routingKey = env.PAGERDUTY_ROUTING_KEY?.trim();
  if (!routingKey) return { status: "not_configured" };

  const fetcher = env.PAGERDUTY_FETCH ?? fetch;
  try {
    const response = await fetcher(PAGERDUTY_EVENTS_V2_URL, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        routing_key: routingKey,
        event_action: "trigger",
        dedup_key: eventId,
        payload: {
          summary: "DSR erasure dead-letter exhausted",
          source: "corelink-signup-worker",
          severity: "critical",
          custom_details: {
            event_id: eventId,
            exhausted: true,
            requeue_count: requeueCount,
          },
        },
      }),
    });
    // PagerDuty Events API v2 acknowledges an accepted trigger with 202.  A
    // generic 2xx (for example a proxy-generated 200 page) is not delivery
    // evidence and must not let us consume the DLQ copy.
    if (response.status !== 202) {
      // Do not copy provider response text/status into logs: the response can
      // contain arbitrary data and must never become an operator-log sink.
      return { status: "failed", error: "http_rejected" };
    }
    return { status: "delivered" };
  } catch {
    // The provider/transport exception is intentionally not logged. It may
    // include URLs, credentials, or arbitrary upstream response text.
    return { status: "failed", error: "transport_error" };
  }
}

function logDlqEvent(fields: Record<string, unknown>): void {
  const canonicalEvent = { event: "dsr.erasure.dead_letter" };
  if (fields.event === undefined) fields.event = canonicalEvent.event;
  console.error(JSON.stringify(fields));
}

type ReceiptBoundaryFailure = "not_configured" | "malformed" | "storage_error";

/**
 * A receipt boundary failure must not become a silent platform discard when
 * the DLQ's bounded delivery budget is exhausted. No PagerDuty or requeue side
 * effect is safe without a receipt claim, so emit a redacted terminal alert.
 */
function retryReceiptBoundaryFailure(
  m: QueueMessage<DsrDlqBody>,
  eventId: string,
  requeueCount: number,
  reason: ReceiptBoundaryFailure,
): void {
  if ((m.attempts ?? 0) < MAX_DLQ_PAGING_ATTEMPTS) {
    m.retry();
    return;
  }
  logDlqEvent({
    level: "alert", severity: "critical", event: DLQ_EVENT_NAME,
    component: "dsr-erasure-dlq", event_id: eventId, exhausted: true,
    requeue_count: requeueCount, action: "receipt_boundary_exhausted",
    receipt_boundary: reason,
    note: "DLQ receipt boundary is unavailable at the retry limit; manual operator disposition required",
  });
  // A receipt-bound recovery envelope is the precondition for every terminal
  // ACK. Retain the DLQ copy even after the alert retry budget is exhausted.
  m.retry();
}

/**
 * Dead-letter handler for `corelink-dsr-erasure-dlq` (M3, GDPR Art.17).
 *
 * A message reaches here only after exhausting the main queue's 10-retry budget
 * on a *sustained transient* fault (container down, erase-auth key never bound)
 * — permanent 4xx failures are now poison-acked upstream and never dead-letter.
 * For EACH dead-lettered erasure we:
 *   1. emit a STRUCTURED, loud ALERT log keyed by an opaque receipt digest, so
 *      the obligation is visible beyond the hourly verify cron without exposing
 *      data-subject identifiers, and
 *   2. give it ONE bounded extra attempt by re-enqueuing onto the main erasure
 *      queue — IFF it has not already been re-enqueued AND the producer binding
 *      is present. The `_dlq_requeue` marker caps this at a single re-enqueue so
 *      a hard-down backend can never create a DLQ⇄main loop. Otherwise the
 *      message is left DEAD (acked) with the loud alert for operator action.
 */
export async function handleErasureDlqBatch(
  batch: QueueMessageBatch<DsrDlqBody>,
  env: DsrDlqEnv,
): Promise<void> {
  for (const m of batch.messages) {
    const body = m.body;
    const priorRequeues = normalizedDlqRequeueCount(body._dlq_requeue);
    let eventId: string;
    try {
      eventId = await dlqEventId(body, priorRequeues);
    } catch {
      m.retry();
      continue;
    }
    const redrive = env.DSR_DLQ_REDRIVE
      ?? (env.CONFIG_DB ? new D1RedriveStore(env.CONFIG_DB) : isRedriveStore(env.DSR_DLQ_RECEIPTS) ? env.DSR_DLQ_RECEIPTS : undefined);
    // A terminal DLQ acknowledgment is permitted only after a strict recovery
    // envelope is durable. Malformed or unavailable recovery storage retries.
    try {
      if (!redrive || !await redrive.capture(eventId, body, Date.now())) {
        m.retry();
        continue;
      }
    } catch {
      m.retry();
      continue;
    }
    const store = env.DSR_DLQ_RECEIPTS ?? (env.CONFIG_DB ? new D1DsrDlqReceiptStore(env.CONFIG_DB) : undefined);
    if (!store) {
      retryReceiptBoundaryFailure(m, eventId, priorRequeues, "not_configured");
      continue;
    }
    let existing: DsrDlqReceipt | null;
    try {
      existing = await store.find(eventId);
      if (existing && !isValidDlqReceipt(existing)) {
        retryReceiptBoundaryFailure(m, eventId, priorRequeues, "malformed");
        continue;
      }
      if (existing?.status === "terminal" || existing?.status === "requeued" || existing?.status === "delivery_exhausted" || existing?.status === "requeue_ambiguous" || existing?.status === "paging_ambiguous") {
        m.ack();
        continue;
      }
      await store.record(eventId, "paging_pending", Date.now());
    } catch {
      retryReceiptBoundaryFailure(m, eventId, priorRequeues, "storage_error");
      continue;
    }
    try {
      const claimed = await store.claimPaging(eventId, Date.now());
      if (!claimed) {
        await store.record(eventId, "paging_ambiguous", Date.now());
        logDlqEvent({
          level: "alert", severity: "critical", event: DLQ_EVENT_NAME,
          component: "dsr-erasure-dlq", event_id: eventId, exhausted: true,
          requeue_count: priorRequeues, action: "paging_ambiguous",
          note: "PagerDuty delivery has an ambiguous durable claim; manual operator disposition required",
        });
        m.ack();
        continue;
      }
    } catch {
      retryReceiptBoundaryFailure(m, eventId, priorRequeues, "storage_error");
      continue;
    }
    const canRequeue = priorRequeues < MAX_DLQ_REQUEUES && !!env.DSR_QUEUE;

    const paging = await pageDlqEvent(body, eventId, priorRequeues, env);
    if (paging.status !== "delivered") {
      if (paging.status === "failed" && paging.error === "transport_error") {
        try {
          await store.record(eventId, "paging_ambiguous", Date.now());
        } catch {
          m.retry();
          continue;
        }
        logDlqEvent({
          level: "alert", severity: "critical", event: DLQ_EVENT_NAME,
          component: "dsr-erasure-dlq", event_id: eventId, exhausted: true,
          requeue_count: priorRequeues, action: "paging_ambiguous",
          paging_status: paging.status, paging_error: paging.error,
          note: "PagerDuty transport outcome is ambiguous; durable operator disposition required",
        });
        m.ack();
        continue;
      }
      const exhausted = (m.attempts ?? 0) >= MAX_DLQ_PAGING_ATTEMPTS;
      try {
        await store.record(eventId, exhausted ? "delivery_exhausted" : "paging_retry", Date.now());
      } catch {
        m.retry();
        continue;
      }
      logDlqEvent({
        level: "alert",
        severity: "critical",
        event: DLQ_EVENT_NAME,
        component: "dsr-erasure-dlq",
        event_id: eventId,
        exhausted: true,
        requeue_count: priorRequeues,
        action: exhausted ? "delivery_exhausted" : "retry_paging",
        paging_status: paging.status,
        paging_error: paging.status === "failed" ? paging.error : "route_not_configured",
        note: exhausted
          ? "PagerDuty delivery retry budget exhausted; durable operator disposition required"
          : "PagerDuty did not accept the exhausted DSR alert; retaining the DLQ copy for delivery retry",
      });
      if (exhausted) m.ack(); else m.retry();
      continue;
    }

    const recovered = existing?.status === "paging_retry";
    try {
      await store.record(eventId, recovered ? "paging_recovered" : "paged", Date.now());
    } catch {
      m.retry();
      continue;
    }

    if (!canRequeue) {
      logDlqEvent({
        level: "alert",
        severity: "critical",
        event: DLQ_EVENT_NAME,
        component: "dsr-erasure-dlq",
        event_id: eventId,
        exhausted: true,
        requeue_count: priorRequeues,
        action: "left_dead",
        recovered,
        paging_status: paging.status,
        paging_configured: true,
        note: "GDPR Art.17 erasure dead after bounded re-enqueue — MANUAL operator action required",
      });
      try {
        await store.record(eventId, "terminal", Date.now());
        m.ack();
      } catch {
        m.retry();
      }
      continue;
    }
    try {
      const claimed = await store.claimRequeue(eventId, Date.now());
      if (!claimed) {
        await store.record(eventId, "requeue_ambiguous", Date.now());
        logDlqEvent({
          level: "alert", severity: "critical", event: DLQ_EVENT_NAME,
          component: "dsr-erasure-dlq", event_id: eventId, exhausted: true,
          requeue_count: priorRequeues, action: "requeue_ambiguous",
          recovered,
          paging_status: paging.status, paging_configured: true,
          note: "A bounded DSR re-enqueue has an ambiguous durable claim; manual operator disposition required",
        });
        m.ack();
        continue;
      }
      await env.DSR_QUEUE!.send({ ...body, _dlq_requeue: priorRequeues + 1 });
      await store.record(eventId, "requeued", Date.now());
      logDlqEvent({
        level: "alert",
        severity: "critical",
        event: DLQ_EVENT_NAME,
        component: "dsr-erasure-dlq",
        event_id: eventId,
        exhausted: true,
        requeue_count: priorRequeues + 1,
        action: "requeue_once",
        recovered,
        paging_status: paging.status,
        paging_configured: true,
        note: "GDPR Art.17 erasure exhausted main-queue retries — re-enqueued for ONE bounded final attempt",
      });
      m.ack(); // handed back to the main queue; consume the DLQ copy
    } catch {
      try {
        await store.record(eventId, "requeue_ambiguous", Date.now());
      } catch {
        m.retry();
        continue;
      }
      logDlqEvent({
        level: "alert",
        severity: "critical",
        event: DLQ_EVENT_NAME,
        component: "dsr-erasure-dlq",
        event_id: eventId,
        exhausted: true,
        requeue_count: priorRequeues,
        action: "requeue_ambiguous",
        recovered,
        paging_status: paging.status,
        paging_configured: true,
        requeue_error: "transport_error",
        note: "Bounded DSR re-enqueue is ambiguous; durable operator disposition required",
      });
      m.ack();
    }
  }
}

function redriveResponse(status: number, error: string): Response {
  return Response.json({ error }, { status });
}

function validRedriveRef(value: string, prefix: string): boolean {
  return value.length <= 96 && new RegExp(`^${prefix}[A-Za-z0-9._:-]+$`).test(value);
}

/** Operator-only: accepts an opaque receipt id, never caller tenant or payload. */
export async function handleDsrDlqRedrive(request: Request, env: DsrDlqEnv): Promise<Response> {
  if (request.method !== "POST") return redriveResponse(405, "method_not_allowed");
  const expected = env.DSR_DLQ_REDRIVE_AUTH_KEY?.trim();
  if (!expected || expected.length < 32) return redriveResponse(503, "unavailable");
  const authorization = request.headers.get("authorization") ?? "";
  const token = authorization.startsWith("Bearer ") ? authorization.slice("Bearer ".length) : "";
  if (!constantTimeEqual(token, expected)) return redriveResponse(401, "unauthorized");
  if ((!env.CONFIG_DB && !env.DSR_DLQ_REDRIVE) || !env.DSR_QUEUE) return redriveResponse(503, "unavailable");
  const actorRef = request.headers.get("x-corelink-operator-ref") ?? "";
  const approvalRef = request.headers.get("x-corelink-approval-ref") ?? "";
  if (!validRedriveRef(actorRef, "op_") || !validRedriveRef(approvalRef, "apr_")) return redriveResponse(400, "invalid_operator_reference");
  let parsed: unknown;
  try { parsed = await request.json(); } catch { return redriveResponse(400, "invalid_receipt"); }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed) || Object.keys(parsed).length !== 1) return redriveResponse(400, "invalid_receipt");
  const eventId = (parsed as { event_id?: unknown }).event_id;
  if (typeof eventId !== "string" || !EVENT_ID_RE.test(eventId)) return redriveResponse(400, "invalid_receipt");
  const store = env.DSR_DLQ_REDRIVE ?? new D1RedriveStore(env.CONFIG_DB!);
  const nowMs = Date.now();
  try { await store.cleanup(nowMs); } catch { return redriveResponse(503, "unavailable"); }
  let claimed: "claimed" | "expired" | "denied";
  try { claimed = await store.claim(eventId, actorRef, approvalRef, nowMs); } catch { return redriveResponse(503, "unavailable"); }
  if (claimed === "expired") return redriveResponse(410, "receipt_expired");
  if (claimed !== "claimed") return redriveResponse(409, "receipt_not_redriveable");
  let envelope: RedriveEnvelope | null;
  try { envelope = await store.envelope(eventId); } catch {
    await store.ambiguous(eventId, Date.now()).catch(() => undefined);
    return redriveResponse(503, "unavailable");
  }
  if (!envelope || !UUID_RE.test(envelope.dsr_id) || !UUID_RE.test(envelope.tenant_id)
    || envelope.requeue_count !== 0 || !Number.isSafeInteger(envelope.queued_at_ms) || envelope.queued_at_ms < 0
    || (envelope.legal_hold !== 0 && envelope.legal_hold !== 1)) {
    await store.ambiguous(eventId, Date.now()).catch(() => undefined);
    return redriveResponse(503, "unavailable");
  }
  let salt: string;
  try { salt = await deriveErasureSalt(envelope.dsr_id, env.ERASURE_SALT_KEY, env.ENVIRONMENT); } catch {
    await store.ambiguous(eventId, Date.now()).catch(() => undefined);
    return redriveResponse(503, "unavailable");
  }
  try {
    // Durable ambiguous state precedes send. Unknown outcomes cannot resend.
    if (!await store.fence(eventId, Date.now())) return redriveResponse(409, "receipt_not_redriveable");
    await env.DSR_QUEUE.send({ schema: "dev.hugr.corelink.dsr.queued.v1", dsr_id: envelope.dsr_id,
      tenant_id: envelope.tenant_id, subject_id: envelope.tenant_id, erasure_salt_hex: salt,
      queued_at_ms: envelope.queued_at_ms, legal_hold: envelope.legal_hold === 1,
      source: "clerk.user.deleted", _dlq_requeue: 1 });
    await store.submitted(eventId, Date.now());
    return Response.json({ status: "submitted" }, { status: 202 });
  } catch {
    return redriveResponse(502, "redrive_ambiguous");
  }
}
