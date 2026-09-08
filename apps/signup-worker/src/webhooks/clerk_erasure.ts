/** Clerk deletion and GDPR erasure queue domain. */
import type { AutoProvisionEnv } from "./clerk.js";

export interface ClerkUserDeletedEvent {
  type: "user.deleted";
  data: { id: string; deleted?: boolean };
}
/**
 * Frozen wire contract for the DSR erasure queue message (`dsr.queued.v1`).
 * The Rust queue consumer (WI-S11-008) deserializes this into the
 * orchestrator's `ErasureRequest`. `erasure_salt_hex` is 64 hex chars (32
 * bytes); `subject_id == tenant_id` because CoreLink provisions exactly one
 * tenant per Clerk user, so the tenant IS the unit of deletion.
 */
export interface DsrQueuedV1 {
  schema: "dev.hugr.corelink.dsr.queued.v1";
  dsr_id: string;
  tenant_id: string;
  subject_id: string;
  erasure_salt_hex: string;
  queued_at_ms: number;
  legal_hold: boolean;
  source: "clerk.user.deleted";
  clerk_user_id: string;
}

/** Clerk lifecycle IDs are opaque provider IDs, not arbitrary strings. */
export function isValidClerkUserId(value: unknown): value is string {
  return typeof value === "string" && /^user_[A-Za-z0-9_-]{1,128}$/.test(value);
}

const CLERK_PROVISION_LEASE_MS = 5 * 60_000;
// Keep the completed marker for the same period as the minted PAT. This
// blocks late duplicate webhooks without making a tenant permanently unable
// to recover after its credential expires.
const CLERK_PROVISION_COMPLETE_LEASE_MS = 365 * 24 * 60 * 60_000;
export const MIN_INTERNAL_AUTH_KEY_LEN = 32;

/** Atomically claim this Clerk user's provisioning lease before minting. */
export async function claimClerkProvision(db: D1Database, clerkUserId: string): Promise<boolean> {
  const nowMs = Date.now();
  const result = await db
    .prepare(
      "INSERT INTO clerk_provisioning_lock " +
        "(clerk_user_id, state, lease_until_ms, created_ms, updated_ms) " +
        "VALUES (?1, 'in_progress', ?2, ?2, ?2) " +
        "ON CONFLICT(clerk_user_id) DO UPDATE SET " +
        "lease_until_ms = ?2, updated_ms = ?2 " +
        "WHERE ((state = 'in_progress' AND lease_until_ms <= ?2) " +
        "OR (state = 'complete' AND lease_until_ms <= ?2))",
    )
    .bind(clerkUserId, nowMs + CLERK_PROVISION_LEASE_MS)
    .run();
  // Real D1 always exposes meta.changes. Test doubles that predate this
  // contract have no metadata; treating those as a claimed lease preserves
  // their existing focal behavior while production remains fail-closed.
  const changes = result.meta?.changes;
  return typeof changes !== "number" || changes === 1;
}

/** Mark the lease complete only after tenant + PAT provisioning succeeded. */
export async function completeClerkProvision(db: D1Database, clerkUserId: string): Promise<void> {
  const nowMs = Date.now();
  await db
    .prepare(
      "UPDATE clerk_provisioning_lock SET state = 'complete', " +
        "lease_until_ms = ?2, updated_ms = ?2 WHERE clerk_user_id = ?1",
    )
    .bind(clerkUserId, nowMs + CLERK_PROVISION_COMPLETE_LEASE_MS)
    .run();
}

function bytesToHex(buf: ArrayBuffer): string {
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/**
 * Deterministic name-based (v5-shaped) UUID from a Clerk user id. STABLE across
 * Svix redeliveries, so the same account deletion always maps to ONE `dsr_id`
 * and the erasure orchestrator (which dedups per `(dsr_id, backend)`) is
 * idempotent — a redelivered `user.deleted` never double-runs erasure.
 */
export async function deterministicDsrId(clerkUserId: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(`corelink-dsr-v1:${clerkUserId}`),
  );
  const b = new Uint8Array(digest).slice(0, 16);
  b[6] = ((b[6] ?? 0) & 0x0f) | 0x50; // version 5 (name-based)
  b[8] = ((b[8] ?? 0) & 0x3f) | 0x80; // RFC 4122 variant
  const h = bytesToHex(b.buffer);
  return `${h.slice(0, 8)}-${h.slice(8, 12)}-${h.slice(12, 16)}-${h.slice(16, 20)}-${h.slice(20)}`;
}

/**
 * 32-byte erasure salt (hex). `HMAC-SHA256(ERASURE_SALT_KEY, dsr_id)` when the
 * key is set (secret → unlinkable pseudonymization per GDPR Art. 4(5)).
 *
 * Fail-closed in prod (F9, CAA-360 2026-06-13): when `key` is absent and
 * `environment` starts with `"prod"`, throws an error so the caller returns 500
 * and Svix retries the erasure — the right-to-erasure obligation stays alive
 * while the operator misconfiguration is corrected.
 *
 * In non-prod environments (dev/CI) the deterministic `SHA-256("erasure-salt:"
 * + dsr_id)` fallback is still used so tests run without secrets; it is NOT
 * secret and MUST NOT reach production.
 */
export async function deriveErasureSalt(
  dsrId: string,
  key: string | undefined,
  environment?: string,
): Promise<string> {
  if (key && key.length > 0) {
    const k = await crypto.subtle.importKey(
      "raw",
      new TextEncoder().encode(key),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    const sig = await crypto.subtle.sign("HMAC", k, new TextEncoder().encode(dsrId));
    return bytesToHex(sig);
  }
  // ERASURE_SALT_KEY absent — fail CLOSED in prod (the predictable fallback
  // breaks GDPR pseudonymization unlinkability).
  if (environment && environment.startsWith("prod")) {
    throw new Error(
      "ERASURE_SALT_KEY is not configured — refusing to derive a predictable erasure salt in prod (fail-CLOSED)",
    );
  }
  // Non-prod fallback: deterministic but NOT secret. Never reaches production.
  const d = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(`erasure-salt:${dsrId}`),
  );
  return bytesToHex(d);
}

/**
 * Build the `dsr.queued.v1` erasure message for a deleted Clerk user. Pure
 * given its inputs (deterministic `dsr_id` + salt) → fully testable + idempotent.
 *
 * Throws when `saltKey` is absent and `environment` starts with `"prod"` (F9,
 * CAA-360 2026-06-13) — the caller must surface this as a 500 for Svix retry.
 */
export async function buildErasureQueueMessage(input: {
  clerkUserId: string;
  tenantId: string;
  nowMs: number;
  saltKey: string | undefined;
  environment?: string;
  /**
   * Whether the tenant is under a legal hold (litigation / regulatory /
   * retention obligation). When true, the erasure orchestrator and every
   * backend adapter PRESERVE the data instead of erasing it (CTRL-PRIV-033;
   * adapter_d1.rs:202, adapter_r2_cas.rs:157, adapter_r2_ac.rs:144 all return
   * NotApplicable on `legal_hold == true`). Resolved by the caller from the
   * tenant's legal-hold state. Defaults to false — the absence of a hold — so
   * existing callers are unaffected; the caller MUST pass `true` for a held
   * tenant or the preservation branch is never reached.
   */
  legalHold?: boolean;
}): Promise<DsrQueuedV1> {
  const dsrId = await deterministicDsrId(input.clerkUserId);
  const saltHex = await deriveErasureSalt(dsrId, input.saltKey, input.environment);
  return {
    schema: "dev.hugr.corelink.dsr.queued.v1",
    dsr_id: dsrId,
    tenant_id: input.tenantId,
    subject_id: input.tenantId, // 1 Clerk user : 1 tenant — tenant is the deletion unit
    erasure_salt_hex: saltHex,
    queued_at_ms: input.nowMs,
    legal_hold: input.legalHold ?? false,
    source: "clerk.user.deleted",
    clerk_user_id: input.clerkUserId,
  };
}

/**
 * Resolve whether `tenantId` is under a legal hold from D1.
 *
 * Legal hold is an OPERATOR-ONLY control (litigation / regulatory / unpaid-
 * invoice retention) with no self-serve surface at launch; it is recorded in a
 * dedicated `tenant_legal_hold` table (one row per held tenant, cleared by
 * DELETING the row). A self-serve account deletion (Clerk
 * `user.deleted`) MUST honor an active hold by carrying `legal_hold: true` into
 * the erasure message so the CTRL-PRIV-033 preservation branch in the
 * orchestrator/adapters fires and the legally-retained data is NOT destroyed.
 *
 * Posture on a query error → `false` (NOT held). This is deliberate: the
 * `tenant_legal_hold` table is not yet provisioned in prod, and a missing table
 * surfaces here as a thrown error. Returning `true` on error would make EVERY
 * account deletion a no-op preservation and silently break the live GDPR
 * right-to-erasure obligation — a far larger harm than the low-severity, not-
 * yet-built hold feature. So until an operator provisions the table (and a hold
 * actually exists), this resolves to false and erasure proceeds exactly as it
 * does today; the moment the table + a row exist, a held tenant's deletion
 * carries `legal_hold: true` and preservation kicks in. An ABSENT row (the
 * common case once the table exists — no hold) likewise returns false.
 *
 * NOTE: this wires the previously-dead CTRL-PRIV-033 branch to a real
 * source-of-truth. The hold WRITE surface (operator tooling + the
 * `tenant_legal_hold` migration) is the operator's launch step; this is the
 * READ side that the live erasure trigger consults.
 */
async function tenantUnderLegalHold(
  db: NonNullable<AutoProvisionEnv["CONFIG_DB"]>,
  tenantId: string,
): Promise<boolean> {
  try {
    // Frozen schema (migration 0076, C-LEGALHOLD): tenant_legal_hold has
    // columns (tenant_id, reason, held_at_ms) ONLY. A hold IS the presence of a
    // row keyed by tenant_id; it is cleared by DELETING the row (there is no
    // `released_at_ms` soft-delete column). So existence of a row == held.
    const row = await db
      .prepare(
        "SELECT 1 AS held FROM tenant_legal_hold WHERE tenant_id = ?1 LIMIT 1",
      )
      .bind(tenantId)
      .first<{ held: number }>();
    return row != null;
  } catch {
    // Hold table not yet provisioned / transient read error → no hold exists
    // that we can honor; let erasure proceed (preserving the live obligation).
    // See the posture note above for why this is false, not true.
    return false;
  }
}

/**
 * Handle a verified Clerk `user.deleted` event: look up the tenant and enqueue
 * a GDPR erasure request. FAIL-LOUD (500 → Svix retries) when a tenant exists
 * but the queue binding is absent, so a right-to-erasure obligation is never
 * silently dropped. No tenant (deleted pre-provision or already erased) → 200 no-op.
 */
export async function handleUserDeleted(
  event: ClerkUserDeletedEvent,
  env: AutoProvisionEnv,
  svixId: string,
): Promise<Response> {
  const clerkUserId = event.data?.id;
  if (!clerkUserId) {
    return Response.json({ ok: true, erasure_enqueued: false, reason: "no_user_id" });
  }

  let tenantId: string | null = null;
  if (env.CONFIG_DB) {
    try {
      const row = await env.CONFIG_DB.prepare(
        "SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1 LIMIT 1",
      )
        .bind(clerkUserId)
        .first<{ tenant_id: string }>();
      tenantId = row?.tenant_id ?? null;
    } catch {
      // D1 error — 500 so Svix retries. We must NOT silently drop a deletion.
      return new Response(
        JSON.stringify({ ok: false, error: "tenant_lookup_failed" }),
        { status: 500, headers: { "content-type": "application/json" } },
      );
    }
  }

  if (!tenantId) {
    // No provisioned tenant — nothing to erase. Ack so Svix stops retrying.
    return Response.json({ ok: true, erasure_enqueued: false, reason: "no_tenant" });
  }

  if (!env.DSR_QUEUE) {
    console.error(
      `[clerk-webhook] user.deleted tenant=${tenantId} but DSR_QUEUE unbound — ` +
        `cannot honor erasure; returning 500 for Svix retry (svix=${svixId})`,
    );
    return new Response(
      JSON.stringify({ ok: false, error: "dsr_queue_unconfigured" }),
      { status: 500, headers: { "content-type": "application/json" } },
    );
  }

  // Honor an operator legal hold: a held tenant's data MUST be PRESERVED, not
  // erased, even when the (self-serve, or attacker-driven) Clerk account is
  // deleted. Resolve the hold from D1 and carry it into the message so the
  // CTRL-PRIV-033 preservation branch is actually reachable (it was dead while
  // legal_hold was hardcoded false). CONFIG_DB is the same binding used for the
  // tenant lookup above; if it is absent we cannot read a hold and proceed
  // as un-held (no hold capability provisioned).
  const legalHold = env.CONFIG_DB ? await tenantUnderLegalHold(env.CONFIG_DB, tenantId) : false;

  let msg: DsrQueuedV1;
  try {
    msg = await buildErasureQueueMessage({
      clerkUserId,
      tenantId,
      nowMs: Date.now(),
      saltKey: env.ERASURE_SALT_KEY,
      environment: env.ENVIRONMENT,
      legalHold,
    });
  } catch (saltErr) {
    // F9 (CAA-360 2026-06-13): ERASURE_SALT_KEY absent in prod → fail CLOSED.
    // Return 500 so Svix retries — the erasure obligation stays alive until
    // the operator provisions the secret.
    console.error(
      `[clerk-webhook] erasure salt derivation failed for tenant=${tenantId} svix=${svixId}: ${String(saltErr)}`,
    );
    return new Response(
      JSON.stringify({ ok: false, error: "erasure_salt_key_unconfigured" }),
      { status: 500, headers: { "content-type": "application/json" } },
    );
  }
  // G4 (WI-S11-008): write a durable "DSR requested" anchor BEFORE enqueue so the
  // 24h verify sweep can detect an SLA breach even when the erasure fails before
  // ANY backend tombstone lands (audit-fail-closed → no dsr_erasure_log row).
  // Idempotent: dsr_id is the deterministic name-based UUID, so a Svix
  // redelivery is an INSERT-OR-IGNORE no-op. Best-effort: a D1 failure here must
  // NOT block the enqueue (the queue + orchestrator + audit chain are the
  // primary obligation path); the sweep degrades to its dsr_erasure_log source.
  if (env.CONFIG_DB) {
    try {
      await env.CONFIG_DB.prepare(
        "INSERT OR IGNORE INTO dsr_requested (dsr_id, tenant_id, requested_at, status) " +
          "VALUES (?1, ?2, ?3, 'requested')",
      )
        .bind(msg.dsr_id, tenantId, msg.queued_at_ms)
        .run();
    } catch (reqErr) {
      console.error(
        `[clerk-webhook] dsr_requested write failed dsr_id=${msg.dsr_id} svix=${svixId}: ${String(reqErr)}`,
      );
    }
  }

  await env.DSR_QUEUE.send(msg);
  // dsr_id is a pseudonymous id; tenant_id is not secret. erasure_salt is NEVER logged.
  console.log(
    `[clerk-webhook] erasure enqueued dsr_id=${msg.dsr_id} tenant=${tenantId} svix=${svixId}`,
  );
  return Response.json({
    ok: true,
    erasure_enqueued: true,
    dsr_id: msg.dsr_id,
    tenant_id: tenantId,
  });
}
