/**
 * CAS analytics emitter — Phase-0 lightweight `first_cache_hit` detector.
 *
 * Spec source: `specs/_audits/2026-05-27-phase-0-execution-plan.md` §2.F
 *              "Activation event detection" + PLG framework §3.2.
 *
 * **Coordination note.** This file lives under `apps/cas-worker/` — a brand
 * new namespace that does not collide with Wave-33 Stream B's `crates/`
 * reorg (Wave-33 splits the Rust workspace; this is a TypeScript edge
 * middleware that wraps the Worker request lifecycle). The full CAS data
 * plane will eventually move under this directory; until then this file
 * stands alone as the Phase-0 emitter contract.
 *
 * Algorithm (per spec):
 *   `first_cache_hit` fires on the **2nd** `read_hit` for the same
 *   `(tenant_id, content_hash)` within a 24h window. State is kept in a
 *   KV-style index keyed by `tenant_id|content_hash` with a 24h TTL.
 *
 * Output:
 *   Emits to the `analytics_events` D1 table via the same shape used by
 *   the signup-worker (so agent G's saved views work uniformly).
 */

export interface CasEventContext {
  tenantId: string;
  patId: string;
  contentHash: string;
  /** "read_hit" | "read_miss" | "write" — only "read_hit" triggers the check. */
  kind: "read_hit" | "read_miss" | "write";
  occurredAt: Date;
}

export interface KvLike {
  get(key: string): Promise<string | null>;
  put(key: string, value: string, opts?: { expirationTtl?: number }): Promise<void>;
}

export interface AnalyticsSink {
  emit(
    eventName: string,
    tenantId: string,
    properties: Record<string, unknown>,
  ): Promise<void>;
}

const HIT_TTL_SECONDS = 24 * 60 * 60;
const FIRST_HIT_MARKER = "v1:first_cache_hit_emitted";

/**
 * Process one CAS event. Returns `true` iff this call fired the
 * `first_cache_hit` activation event (used by tests).
 */
export async function recordCasEvent(input: {
  ctx: CasEventContext;
  kv: KvLike;
  analytics: AnalyticsSink;
}): Promise<boolean> {
  const { ctx, kv, analytics } = input;
  if (ctx.kind !== "read_hit") return false;

  // Cheap idempotency: once we've fired `first_cache_hit` for this tenant
  // we never fire it again. Stored as a per-tenant marker with no TTL.
  const tenantMarkerKey = `tenant:${ctx.tenantId}:first_cache_hit_emitted`;
  const already = await kv.get(tenantMarkerKey);
  if (already === FIRST_HIT_MARKER) return false;

  // Per-(tenant, content_hash) 24h window. We mark "seen-as-hit-once" on the
  // first read_hit; the second matching read_hit within the window fires.
  const dedupeKey = `cas:${ctx.tenantId}:${ctx.contentHash}:hit_count`;
  const prior = await kv.get(dedupeKey);
  const count = prior ? Number.parseInt(prior, 10) + 1 : 1;
  await kv.put(dedupeKey, String(count), { expirationTtl: HIT_TTL_SECONDS });

  if (count < 2) return false;

  await analytics.emit("first_cache_hit", ctx.tenantId, {
    pat_id: ctx.patId,
    content_hash: ctx.contentHash,
    occurred_at: ctx.occurredAt.toISOString(),
  });
  await kv.put(tenantMarkerKey, FIRST_HIT_MARKER);
  return true;
}

/**
 * Emit the `first_cli_authed` event on the **first** authenticated `/v1/ping`
 * call from a tenant. Same per-tenant once-only semantics as above.
 */
export async function recordFirstCliAuthed(input: {
  tenantId: string;
  patId: string;
  cliVersion?: string;
  userAgent?: string;
  kv: KvLike;
  analytics: AnalyticsSink;
}): Promise<boolean> {
  const markerKey = `tenant:${input.tenantId}:first_cli_authed_emitted`;
  const already = await input.kv.get(markerKey);
  if (already) return false;
  await input.analytics.emit("first_cli_authed", input.tenantId, {
    pat_id: input.patId,
    cli_version: input.cliVersion ?? "unknown",
    user_agent: input.userAgent ?? "unknown",
  });
  await input.kv.put(markerKey, "1");
  return true;
}
