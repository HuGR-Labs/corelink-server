/**
 * Tenant data-residency (`primary_region`) three-tier cache for the customer
 * CAS/AC hot path (latency WP — collapse the warm `wdb` Server-Timing phase).
 *
 * # Why
 *
 * Every authenticated CAS/AC request resolves the tenant's `primary_region` from
 * D1 (`SELECT primary_region FROM tenant`) to decide region fan-out (backlog #29 —
 * Schrems II residency). That is a synchronous D1-PRIMARY round-trip on the hot
 * path (Cloudflare D1's primary is ENAM; there is NO South-America region), so a
 * SAM/GRU caller pays ~120 ms+ per request for a value that is, in practice,
 * IMMUTABLE per tenant (region pinning is a rare, admin-gated migration). This is
 * one of the 4-6 serial uncached D1 reads that make up the `wdb` phase.
 *
 * This mirrors the `pat` L2 and the {@link ./tenant_suspend_gate} exactly
 * (ADR-0070):
 *
 *   L1  in-memory per-isolate  ({@link RESIDENCY_CACHE_TTL_MS}, micro-burst dedup)
 *   L2  Workers KV  (`tres:<tenant>`, {@link KV_RESIDENCY_TTL_S}s — globally
 *       replicated with a PER-COLO edge cache, so it survives isolate fan-out and
 *       serves edge-local ~3 ms reads everywhere, including SAM)
 *   L3  D1  (the source-of-truth on every L1+L2 miss)
 *
 * # Failure posture — FAIL-CLOSED (unlike the suspend gate)
 *
 * The suspend gate fails OPEN (a transient D1 fault must not break active
 * tenants). Residency is the OPPOSITE: the caller (`index.ts`) returns a hard
 * `503 RESIDENCY_UNAVAILABLE` when the region cannot be resolved, because falling
 * through to the IAD/local path on an unknown region is precisely the cross-border
 * leak (an EU `weur` tenant served from US storage). So on a D1 error this cache
 * returns {@link RESIDENCY_UNRESOLVED} — signalling the caller to 503 — UNLESS we
 * already hold a cached region for the tenant, in which case we serve that (a
 * known region on a transient fault is both safe and correct, since residency is
 * immutable). A resolved value distinguishes "region X" from "no tenant row"
 * (`region: null` ⇒ the caller's existing `primaryRegion === undefined` IAD-local
 * fall-through).
 *
 * # Why bounded staleness is SAFE here
 *
 * A stale cached region can only ROUTE a request to the wrong region's Worker; it
 * can never SILENTLY leak, because the container's residency backstop
 * (`residency.rs`) re-checks the server-trusted `x-corelink-primary-region` macro
 * against the colo it actually landed on and returns `409 residency_violation` on
 * a mismatch. So the worst case of a stale entry (right after a rare admin re-pin)
 * is a self-correcting 409, not a cross-border write. That is what makes an
 * edge-cache window acceptable on a compliance-sensitive value.
 *
 * INV-NO-PII-IN-LOGS: tenant_id is not logged here.
 */

import type { D1Reader, KvReader } from "./pat_verify_cache.js";

/**
 * TTL for a cached residency decision in the L1 (per-isolate) tier, in ms.
 * Matches the `pat`/`tsusp` L1 (5 s) — with the L2 KV tier carrying the far-D1
 * latency load, L1 reverts to pure micro-burst dedup within one isolate, keeping
 * the chained (L1 + L2) staleness bound near the KV floor.
 */
export const RESIDENCY_CACHE_TTL_MS = 5_000;

/** KV key prefix for a cached residency verdict (distinct from `patrow:`/`tsusp:`). */
const KV_RESIDENCY_PREFIX = "tres:";

/**
 * L2 KV entry TTL for a cached residency verdict, in SECONDS. 60 s is KV's floor
 * and deliberately CONSERVATIVE for a compliance-sensitive value: a region re-pin
 * (rare, admin-gated) propagates within ≤ this + the L1 slack, and the container
 * residency backstop (409) prevents any stale route from becoming a silent leak in
 * the meantime. Mirrors {@link ./tenant_suspend_gate.KV_SUSPEND_TTL_S}. A longer
 * TTL is a possible follow-up once a re-pin KV-flush lever exists.
 */
export const KV_RESIDENCY_TTL_S = 60;

/** The KV key for a tenant's cached residency verdict. */
export function residencyKvKey(tenantId: string): string {
  return KV_RESIDENCY_PREFIX + tenantId;
}

/**
 * Sentinel for "could not resolve the region AND we hold no prior value" — the
 * caller MUST fail closed (503) rather than fall through to the IAD/local path.
 * Distinct from a resolved `{ region: null }` (a real tenant row with no region ⇒
 * IAD-local).
 */
export const RESIDENCY_UNRESOLVED = Symbol("residency_unresolved");

/**
 * A resolved residency decision: `region` is the tenant's `primary_region` macro
 * (e.g. `weur`), or `null` when the tenant has no row / no region (⇒ the caller's
 * existing IAD-local fall-through).
 */
export interface ResolvedResidency {
  readonly region: string | null;
}

/** The result of {@link resolveTenantResidency}: resolved, or unresolved-fail-closed. */
export type ResidencyResult = ResolvedResidency | typeof RESIDENCY_UNRESOLVED;

/**
 * Options for {@link resolveTenantResidency}. All optional so tests can pass none;
 * `index.ts` passes `kv` + `waitUntil`.
 */
export interface ResidencyOpts {
  /** L2 Workers KV binding (METADATA_KV). Absent ⇒ L2 skipped (L1 + D1 only). */
  readonly kv?: KvReader;
  /**
   * `ctx.waitUntil` — extends the request lifetime so the KV write-behind
   * completes after the response returns (else a `void kv.put(...)` is CANCELLED
   * on response return — the #859 bug). Tests without it fall back to `await`.
   */
  readonly waitUntil?: (p: Promise<unknown>) => void;
  /** Wall-clock ms (injectable for tests; defaults to `Date.now()`). */
  readonly nowMs?: number;
}

/**
 * Upper bound on distinct tenants held in the L1 cache — a defensive ceiling so
 * the module-level map cannot grow without bound under a wide tenant fan-out.
 * Mirrors the suspend gate's `SUSPEND_CACHE_CAP`.
 */
const RESIDENCY_CACHE_CAP = 4096;

/** One cached residency decision: the resolved region plus when it was fetched. */
interface CachedResidency {
  readonly region: string | null;
  readonly fetchedAtMs: number;
}

/**
 * Module-level (per-isolate) L1 cache. Map insertion order gives cheap
 * oldest-first eviction. Keyed by tenant_id.
 */
const residencyCache = new Map<string, CachedResidency>();

/**
 * Per-tenant single-flight map: concurrent misses for one tenant share ONE read
 * chain. Entries are removed as soon as the read settles.
 */
const inflight = new Map<string, Promise<ResidencyResult>>();

/** Insert a decision, evicting to stay within {@link RESIDENCY_CACHE_CAP}. */
function putCache(tenantId: string, region: string | null, nowMs: number): void {
  if (residencyCache.size >= RESIDENCY_CACHE_CAP && !residencyCache.has(tenantId)) {
    // Drop expired entries first.
    for (const [k, e] of residencyCache) {
      if (nowMs - e.fetchedAtMs >= RESIDENCY_CACHE_TTL_MS) {
        residencyCache.delete(k);
      }
    }
    // Still full → evict the oldest (insertion-order) entry.
    if (residencyCache.size >= RESIDENCY_CACHE_CAP) {
      const oldest = residencyCache.keys().next().value;
      if (oldest !== undefined) {
        residencyCache.delete(oldest);
      }
    }
  }
  residencyCache.set(tenantId, { region, fetchedAtMs: nowMs });
}

/**
 * Read + validate a cached residency verdict from KV. Returns `{ region }` on a
 * clean hit, or `null` on miss / malformed value / KV fault (all of which fall
 * through to D1). Never throws — a KV fault must not break the hot path.
 */
async function kvGetResidency(kv: KvReader, tenantId: string): Promise<ResolvedResidency | null> {
  let raw: string | null;
  try {
    raw = await kv.get(residencyKvKey(tenantId));
  } catch {
    return null; // KV fault → treat as miss, fall through to D1.
  }
  if (raw === null) {
    return null;
  }
  try {
    const o = JSON.parse(raw) as Record<string, unknown>;
    const region = o["region"];
    // Validate the shape before trusting it: a stored verdict is either a
    // non-empty string region or an explicit null (no-row). Anything else ⇒ miss.
    if (region === null) {
      return { region: null };
    }
    if (typeof region === "string" && region.length > 0) {
      return { region };
    }
    return null;
  } catch {
    return null; // malformed JSON → miss.
  }
}

/**
 * Write a residency verdict to KV with the bounded TTL. Best-effort — a KV write
 * failure must never break the hot path. Both a resolved region and the null
 * (no-row) verdict are cached: caching the negative is what turns the far-D1 read
 * into an edge-local one for tenants with no region row (the IAD-local common
 * case).
 */
async function kvPutResidency(kv: KvReader, tenantId: string, region: string | null): Promise<void> {
  try {
    await kv.put(residencyKvKey(tenantId), JSON.stringify({ region }), {
      expirationTtl: KV_RESIDENCY_TTL_S,
    });
  } catch {
    // Swallow — a KV write failure must never break the hot path (the D1 read
    // succeeded and the caller already has the resolved region).
  }
}

/** D1 row shape for the residency lookup. */
interface TenantRegionRow {
  readonly primary_region: string | null;
}

/**
 * Resolve `tenantId`'s `primary_region` (⇒ region fan-out routing) through the
 * three-tier cache.
 *
 * Returns {@link ResolvedResidency} (`region` = the macro, or `null` for
 * no-row/IAD-local) on success, or {@link RESIDENCY_UNRESOLVED} when the region
 * cannot be established AND no prior value is cached — the caller MUST then fail
 * closed (503), never fall through to IAD. On a D1 error WITH a cached value, the
 * cached region is served (safe: residency is immutable; a transient fault must
 * not 503 an established tenant). The error itself is never cached.
 *
 * @param db        The CONFIG_DB (D1) binding / replica session holding `tenant`.
 * @param tenantId  The PAT-resolved tenant id (never a client-supplied value).
 * @param opts      L2 KV binding, `waitUntil`, and an injectable clock.
 */
export async function resolveTenantResidency(
  db: D1Reader,
  tenantId: string,
  opts: ResidencyOpts = {},
): Promise<ResidencyResult> {
  const nowMs = opts.nowMs ?? Date.now();

  // ── L1: fresh per-isolate cache hit ─────────────────────────────────────────
  const cached = residencyCache.get(tenantId);
  if (cached !== undefined && nowMs - cached.fetchedAtMs < RESIDENCY_CACHE_TTL_MS) {
    return { region: cached.region };
  }

  // ── Single-flight: collapse concurrent misses to one read chain ────────────
  const existing = inflight.get(tenantId);
  if (existing !== undefined) {
    return existing;
  }

  const flight = (async (): Promise<ResidencyResult> => {
    try {
      // ── L2: KV (edge-local, per-colo — survives fan-out; the SAM fix) ──────
      if (opts.kv) {
        const kvVerdict = await kvGetResidency(opts.kv, tenantId);
        if (kvVerdict !== null) {
          putCache(tenantId, kvVerdict.region, nowMs);
          return kvVerdict;
        }
      }

      // ── L3: D1 source-of-truth (via the replica session in `db`) ───────────
      const row = await db
        .prepare("SELECT primary_region FROM tenant WHERE tenant_id = ?1 LIMIT 1")
        .bind(tenantId)
        .first<TenantRegionRow>();
      // No row OR a null region ⇒ `null` (the caller's IAD-local fall-through).
      const region =
        row !== null && typeof row.primary_region === "string" && row.primary_region.length > 0
          ? row.primary_region
          : null;
      putCache(tenantId, region, nowMs);

      // Write-behind to L2 KV (best-effort), handed to waitUntil so it survives
      // the response (else a Worker CANCELS the un-awaited put — the #859 bug).
      if (opts.kv) {
        const putPromise = kvPutResidency(opts.kv, tenantId, region);
        if (opts.waitUntil) {
          opts.waitUntil(putPromise);
        } else {
          await putPromise;
        }
      }
      return { region };
    } catch {
      // D1 fault — do NOT cache (self-heals). FAIL-CLOSED, EXCEPT when we already
      // hold a region: a known region on a transient fault is safe + correct, so
      // serve it rather than 503 an established tenant. A tenant we have no prior
      // knowledge of resolves to UNRESOLVED ⇒ the caller 503s (never IAD-leaks).
      // (A KV fault never lands here — kvGetResidency swallows it as a miss.)
      const prev = residencyCache.get(tenantId);
      return prev !== undefined ? { region: prev.region } : RESIDENCY_UNRESOLVED;
    } finally {
      inflight.delete(tenantId);
    }
  })();
  inflight.set(tenantId, flight);
  return flight;
}

/**
 * Test-only: clear the module-level cache and in-flight map so cases do not leak
 * cached decisions into one another.
 */
export function __resetTenantResidencyCacheForTest(): void {
  residencyCache.clear();
  inflight.clear();
}
