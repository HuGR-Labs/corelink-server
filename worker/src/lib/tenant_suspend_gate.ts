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
 * # Perf — three-tier read (L1 isolate → L2 KV → L3 D1), the SAM-latency layer
 *
 * `extractAuth` runs on every authenticated CAS/AC request, so an uncached
 * per-request D1 read here adds a synchronous D1 round-trip to the hot path. For
 * a client far from the single-region D1 primary (`running_in_region: ENAM`;
 * Cloudflare D1 has NO South-America region) that round-trip is ~120 ms even via
 * a read replica — and Cloudflare fans a single client's requests across many
 * isolates, so a per-isolate cache alone is defeated (it rarely hits for one
 * caller). This gate therefore mirrors the `pat` read's L2 exactly (ADR-0070):
 *
 *   L1  in-memory per-isolate  ({@link SUSPEND_CACHE_TTL_MS}, micro-burst dedup)
 *   L2  Workers KV  (`tsusp:<tenant>`, {@link KV_SUSPEND_TTL_S}s — globally
 *       replicated with a PER-COLO edge cache, so it survives isolate fan-out
 *       and serves edge-local ~3 ms reads everywhere, including SAM)
 *   L3  D1  (the `db` handle, a `first-unconstrained` replica session — the
 *       source-of-truth on every L1+L2 miss)
 *
 * Unlike the `pat` L2 (which caches POSITIVE rows only, so a fresh mint works via
 * the D1-miss path), THIS gate caches the **NEGATIVE** verdict too — "not
 * suspended" is the hot-path common case, and caching it is precisely what turns
 * the far-D1 read into an edge-local one. The bounded staleness that creates (a
 * tenant suspended in D1 keeps hot-path access until the L2 entry expires) is the
 * deliberate, ratified trade-off recorded in **ADR-0070** — the enforcement
 * window is ≤ {@link KV_SUSPEND_TTL_S}s (KV TTL) + ≤ {@link SUSPEND_CACHE_TTL_MS}
 * of isolate slack. The immediate hard-stop levers are unchanged: individual PAT
 * revoke (worker revoke also KV-deletes the `patrow:` entry) and the container
 * `NativePatGate`. This is inside the offboarding state machine's own day-scale
 * `suspended` arm; for abuse response the ADR keeps PAT-revoke as the ≤0-window
 * lever.
 *
 * # Failure posture — fail-OPEN for availability, but never for a KNOWN suspend
 *
 * A KV fault is swallowed as a miss (falls through to D1). A D1 read error is NOT
 * cached (it self-heals on the next op), matching the tier cache /
 * `checkRequestQuota` fail-open-for-availability posture: a transient D1 fault
 * must not break ACTIVE tenants. BUT fail-open must never grant access to a
 * tenant we ALREADY KNOW is suspended — so on a read error we fall back to the
 * last cached decision (even if past its TTL): a known-suspended cached value
 * still returns `true` (⇒ 403). Only a tenant we have NO prior knowledge of is
 * allowed through on a read error.
 *
 * INV-NO-PII-IN-LOGS: tenant_id is not logged here.
 */

import type { D1Reader, KvReader } from "./pat_verify_cache.js";

/**
 * The `tenant_offboarding_state.state` values that DENY customer CAS/AC access.
 * Lowercase to match the migration 0046 CHECK constraint (canonical 6-arm
 * taxonomy). `suspended` = admin-revoke-only window; `erased` = post cryptographic
 * erasure. The earlier arms (cancel_requested / grace_period / read_only) are
 * NOT here — the tenant still needs read access in its export/restore windows.
 */
const DENY_STATES: ReadonlySet<string> = new Set(["suspended", "erased"]);

/**
 * TTL for a cached suspend decision in the L1 (per-isolate) tier, in ms. With
 * the L2 KV tier (below) now carrying the herd + far-D1 latency load, L1 reverts
 * to pure micro-burst dedup within one isolate — so it is tightened to 5 s to
 * match the `pat` L1 ({@link ../lib/pat_verify_cache}) and the container
 * `NativePatGate` verify cache, keeping the chained staleness bound (L1 + L2)
 * near the KV floor rather than compounding a wider window. See ADR-0070.
 */
export const SUSPEND_CACHE_TTL_MS = 5_000;

/** KV key prefix for a cached suspend verdict (distinct from `patrow:`). */
const KV_SUSPEND_PREFIX = "tsusp:";

/**
 * L2 KV entry TTL for a cached suspend verdict, in SECONDS. 60 s is KV's floor
 * and bounds the suspend-enforcement window on the edge: a tenant suspended in
 * D1 keeps hot-path access until this entry expires (then the KV miss forces a
 * fresh D1 read). This is the ratified ADR-0070 trade-off — the coarse
 * `suspended` offboarding arm is day-scale, and PAT revoke remains the immediate
 * lever for abuse. It REPLACES (does not add to) the D1-read axis on the hot
 * path for far-from-D1 callers.
 */
export const KV_SUSPEND_TTL_S = 60;

/** The KV key for a tenant's cached suspend verdict. */
export function suspendKvKey(tenantId: string): string {
  return KV_SUSPEND_PREFIX + tenantId;
}

/**
 * Options for {@link isTenantSuspended}. All optional so existing callers and
 * tests can pass none; `extractAuth` passes `kv` + `waitUntil` (perf #99 / #859).
 */
export interface SuspendGateOpts {
  /** L2 Workers KV binding (METADATA_KV). Absent ⇒ L2 skipped (L1 + D1 only). */
  readonly kv?: KvReader;
  /**
   * `ctx.waitUntil` — extends the request lifetime so the KV write-behind
   * completes after the response returns. Without it a `void kv.put(...)` is
   * CANCELLED on response return (the #859 bug); a no-`waitUntil` caller (tests)
   * falls back to `await`.
   */
  readonly waitUntil?: (p: Promise<unknown>) => void;
  /** Wall-clock ms (injectable for tests; defaults to `Date.now()`). */
  readonly nowMs?: number;
}

/**
 * Upper bound on distinct tenants held in the L1 cache. Mirrors #667's
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
 * Module-level (per-isolate) L1 cache. Map insertion order gives us cheap
 * oldest-first eviction. Keyed by tenant_id.
 */
const suspendCache = new Map<string, CachedSuspend>();

/**
 * Per-tenant single-flight map: a tenant whose decision is being fetched has an
 * in-flight promise here, so concurrent misses share ONE read chain. Entries are
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

/**
 * Read + validate a cached suspend verdict from KV. Returns the boolean verdict
 * on a clean hit, or `null` on miss / malformed value / KV fault (all of which
 * fall through to D1). Never throws — a KV fault must not break auth.
 */
async function kvGetSuspend(kv: KvReader, tenantId: string): Promise<boolean | null> {
  let raw: string | null;
  try {
    raw = await kv.get(suspendKvKey(tenantId));
  } catch {
    return null; // KV fault → treat as miss, fall through to D1.
  }
  if (raw === null) {
    return null;
  }
  try {
    const o = JSON.parse(raw) as Record<string, unknown>;
    // Validate the shape before trusting it (a malformed/foreign value ⇒ miss).
    if (typeof o["suspended"] !== "boolean") {
      return null;
    }
    return o["suspended"] as boolean;
  } catch {
    return null; // malformed JSON → miss.
  }
}

/**
 * Write a suspend verdict to KV with the bounded enforcement-window TTL.
 * Best-effort — a KV write failure must never break auth. BOTH verdicts are
 * written (unlike the `pat` L2's positive-only rule): the negative verdict is
 * the hot-path common case and caching it is the latency win (ADR-0070).
 */
async function kvPutSuspend(kv: KvReader, tenantId: string, suspended: boolean): Promise<void> {
  try {
    await kv.put(suspendKvKey(tenantId), JSON.stringify({ suspended }), {
      expirationTtl: KV_SUSPEND_TTL_S,
    });
  } catch {
    // Swallow — a KV write failure must never break auth (the D1 read succeeded).
  }
}

/** D1 row shape for the offboarding-state lookup. */
interface OffboardingRow {
  readonly state: string | null;
}

/**
 * Is `tenantId` suspended/erased (⇒ deny the customer CAS/AC path with 403)?
 *
 * Three-tier read: L1 in-memory ({@link SUSPEND_CACHE_TTL_MS}) → L2 KV
 * ({@link KV_SUSPEND_TTL_S}s) → L3 D1 (source-of-truth), single-flighted so a
 * burst of concurrent misses collapses to one read chain. On a D1 read error:
 * fall back to the last cached decision if one exists (a known suspend still
 * denies), else fail OPEN (return `false`) so a transient D1 fault does not
 * break active tenants. The error itself is never cached — it self-heals.
 *
 * @param db        The CONFIG_DB (D1) binding / replica session holding
 *                  `tenant_offboarding_state`.
 * @param tenantId  The PAT-resolved tenant id (never a client-supplied value).
 * @param opts      L2 KV binding, `waitUntil`, and an injectable clock.
 */
export async function isTenantSuspended(
  db: D1Reader,
  tenantId: string,
  opts: SuspendGateOpts = {},
): Promise<boolean> {
  const nowMs = opts.nowMs ?? Date.now();

  // ── L1: fresh per-isolate cache hit ─────────────────────────────────────────
  const cached = suspendCache.get(tenantId);
  if (cached !== undefined && nowMs - cached.fetchedAtMs < SUSPEND_CACHE_TTL_MS) {
    return cached.suspended;
  }

  // ── Single-flight: collapse concurrent misses to one read chain ────────────
  const existing = inflight.get(tenantId);
  if (existing !== undefined) {
    return existing;
  }

  const flight = (async (): Promise<boolean> => {
    try {
      // ── L2: KV (edge-local, per-colo — survives fan-out; the SAM fix) ──────
      if (opts.kv) {
        const kvVerdict = await kvGetSuspend(opts.kv, tenantId);
        if (kvVerdict !== null) {
          putCache(tenantId, kvVerdict, nowMs);
          return kvVerdict;
        }
      }

      // ── L3: D1 source-of-truth (via the replica session in `db`) ───────────
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

      // Write-behind to L2 KV (best-effort). BOTH verdicts are cached — the
      // negative (not-suspended) is the hot-path common case and caching it is
      // the latency win (ADR-0070). Handed to waitUntil so it survives the
      // response (else a Worker CANCELS the un-awaited put — the #859 bug).
      if (opts.kv) {
        const putPromise = kvPutSuspend(opts.kv, tenantId, suspended);
        if (opts.waitUntil) {
          opts.waitUntil(putPromise);
        } else {
          await putPromise;
        }
      }
      return suspended;
    } catch {
      // D1 fault — do NOT cache (self-heals). Fail OPEN for availability, EXCEPT
      // when we already hold a decision: a known-suspended cached value must
      // still deny (403). A tenant we have no prior knowledge of is allowed.
      // (A KV fault never lands here — kvGetSuspend swallows it as a miss.)
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
