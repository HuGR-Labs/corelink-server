/**
 * Tenant fast-suspend gate for the customer CAS/AC hot path (go-live GAP G4).
 *
 * # Why
 *
 * `tenant_offboarding_state` (migration 0046) is the durable source-of-truth for
 * the tenant-level offboarding state machine. Its terminal enforcement arms are
 * `suspended` (T+45..T+90; admin-revoke only) and `erased` (T+90+; cryptographic
 * erasure committed). Before this gate, that table was read ONLY by the runner
 * mint path (`runner_mint.ts`) — the customer CAS/AC hot path (`extractAuth` in
 * `index.ts`, which resolves the tenant from the `pat` D1 row) NEVER consulted
 * it. A suspended/abusive tenant therefore kept full read/write cache access
 * until every one of its PATs was individually revoked.
 *
 * INVARIANT (G4): a tenant whose `tenant_offboarding_state.state ∈ {suspended,
 * erased}` is denied **403 fail-closed** on the customer CAS/AC path. The earlier
 * offboarding arms (`cancel_requested` / `grace_period` / `read_only`) are the
 * customer's own export/restoration windows and MUST retain access — this gate
 * is deliberately NARROWER than `runner_mint.ts`'s "any offboarding row ⇒ deny".
 *
 * # Perf — single-flight + short-TTL cache (mirrors #667's `CachedTierResolver`)
 *
 * `extractAuth` runs on every authenticated CAS/AC request, so an uncached
 * per-request D1 read here would add a synchronous D1 round-trip to the hot path
 * (the exact cold-hydrate herd #667 fixed for the tier read). We mirror that
 * pattern: an in-memory `tenant -> {suspended, fetchedAt}` cache with a
 * {@link SUSPEND_CACHE_TTL_MS} TTL and a per-tenant single-flight lock so a warm
 * tenant resolves in memory and a burst of concurrent misses collapses to ONE
 * D1 read. The `tenant_offboarding_state` table stays the authority, so a
 * bounded `≤ TTL` staleness on a fresh suspend is the accepted trade-off (a
 * newly-suspended tenant loses access within one TTL; individual PAT revoke
 * remains the immediate lever).
 *
 * # Failure posture — fail-OPEN for availability, but never for a KNOWN suspend
 *
 * A D1 read error is NOT cached (it self-heals on the next op), matching the
 * tier cache / `checkRequestQuota` fail-open-for-availability posture: a
 * transient D1 fault must not break ACTIVE tenants. BUT fail-open must never
 * grant access to a tenant we ALREADY KNOW is suspended — so on a read error we
 * fall back to the last cached decision (even if past its TTL): a known-suspended
 * cached value still returns `true` (⇒ 403). Only a tenant we have NO prior
 * knowledge of is allowed through on a read error.
 *
 * INV-NO-PII-IN-LOGS: tenant_id is not logged here.
 */

import type { D1Database } from "@cloudflare/workers-types";

/**
 * The `tenant_offboarding_state.state` values that DENY customer CAS/AC access.
 * Lowercase to match the migration 0046 CHECK constraint (canonical 6-arm
 * taxonomy). `suspended` = admin-revoke-only window; `erased` = post cryptographic
 * erasure. The earlier arms (cancel_requested / grace_period / read_only) are
 * NOT here — the tenant still needs read access in its export/restore windows.
 */
const DENY_STATES: ReadonlySet<string> = new Set(["suspended", "erased"]);

/**
 * TTL for a cached suspend decision, in ms. Mirrors #667's
 * `DEFAULT_TIER_CACHE_TTL_MS` (30 s): amortises a cold burst's per-op D1 read
 * while bounding how long a freshly-suspended tenant can keep access to ≤ TTL.
 */
export const SUSPEND_CACHE_TTL_MS = 30_000;

/**
 * Upper bound on distinct tenants held in the cache. Mirrors #667's
 * `TIER_CACHE_CAP` — a defensive ceiling so the module-level map cannot grow
 * without bound under a wide tenant fan-out. On overflow we evict expired
 * entries first, then the oldest (insertion-order) entry.
 */
const SUSPEND_CACHE_CAP = 4096;

/** One cached suspend decision: the resolved verdict plus when it was fetched. */
interface CachedSuspend {
  readonly suspended: boolean;
  readonly fetchedAtMs: number;
}

/**
 * Module-level (per-isolate) cache. Map insertion order gives us cheap
 * oldest-first eviction. Keyed by tenant_id.
 */
const suspendCache = new Map<string, CachedSuspend>();

/**
 * Per-tenant single-flight map: a tenant whose decision is being fetched has an
 * in-flight promise here, so concurrent misses share ONE D1 read. Entries are
 * removed as soon as the read settles.
 */
const inflight = new Map<string, Promise<boolean>>();

/** Insert a decision, evicting to stay within {@link SUSPEND_CACHE_CAP}. */
function putCache(tenantId: string, suspended: boolean, nowMs: number): void {
  if (suspendCache.size >= SUSPEND_CACHE_CAP && !suspendCache.has(tenantId)) {
    // Drop expired entries first.
    for (const [k, e] of suspendCache) {
      if (nowMs - e.fetchedAtMs >= SUSPEND_CACHE_TTL_MS) {
        suspendCache.delete(k);
      }
    }
    // Still full → evict the oldest (insertion-order) entry.
    if (suspendCache.size >= SUSPEND_CACHE_CAP) {
      const oldest = suspendCache.keys().next().value;
      if (oldest !== undefined) {
        suspendCache.delete(oldest);
      }
    }
  }
  suspendCache.set(tenantId, { suspended, fetchedAtMs: nowMs });
}

/** D1 row shape for the offboarding-state lookup. */
interface OffboardingRow {
  readonly state: string | null;
}

/**
 * Is `tenantId` suspended/erased (⇒ deny the customer CAS/AC path with 403)?
 *
 * Cached with single-flight + {@link SUSPEND_CACHE_TTL_MS} TTL. On a D1 read
 * error: fall back to the last cached decision if one exists (a known suspend
 * still denies), else fail OPEN (return `false`) so a transient D1 fault does
 * not break active tenants. The error itself is never cached — it self-heals.
 *
 * @param db        The CONFIG_DB (D1) binding holding `tenant_offboarding_state`.
 * @param tenantId  The PAT-resolved tenant id (never a client-supplied value).
 * @param nowMs     Wall-clock ms (injectable for tests; defaults to Date.now()).
 */
export async function isTenantSuspended(
  db: D1Database,
  tenantId: string,
  nowMs: number = Date.now(),
): Promise<boolean> {
  // ── Fresh cache hit ────────────────────────────────────────────────────────
  const cached = suspendCache.get(tenantId);
  if (cached !== undefined && nowMs - cached.fetchedAtMs < SUSPEND_CACHE_TTL_MS) {
    return cached.suspended;
  }

  // ── Single-flight: collapse concurrent misses to one D1 read ───────────────
  const existing = inflight.get(tenantId);
  if (existing !== undefined) {
    return existing;
  }

  const flight = (async (): Promise<boolean> => {
    try {
      const row = await db
        .prepare(
          "SELECT state FROM tenant_offboarding_state WHERE tenant_id = ?1 LIMIT 1",
        )
        .bind(tenantId)
        .first<OffboardingRow>();
      // No row ⇒ active tenant (the common case). A row in a DENY_STATES arm
      // ⇒ suspended. Any other arm (cancel_requested/grace_period/read_only)
      // ⇒ still allowed (export/restore windows).
      const suspended = row !== null && row.state !== null && DENY_STATES.has(row.state);
      putCache(tenantId, suspended, nowMs);
      return suspended;
    } catch {
      // D1 fault — do NOT cache (self-heals). Fail OPEN for availability, EXCEPT
      // when we already hold a decision: a known-suspended cached value must
      // still deny (403). A tenant we have no prior knowledge of is allowed.
      const prev = suspendCache.get(tenantId);
      return prev !== undefined ? prev.suspended : false;
    } finally {
      inflight.delete(tenantId);
    }
  })();
  inflight.set(tenantId, flight);
  return flight;
}

/**
 * Test-only: clear the module-level cache and in-flight map so cases do not
 * leak cached decisions into one another.
 */
export function __resetTenantSuspendCacheForTest(): void {
  suspendCache.clear();
  inflight.clear();
}
