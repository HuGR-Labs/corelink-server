/**
 * Tenant TIER three-tier cache for the customer CAS/AC hot path — latency WP
 * SLICE 2 (collapse the warm `wdb` Server-Timing phase further).
 *
 * # Why
 *
 * Every authenticated CAS/AC request resolves the tenant's effective tier via
 * {@link getTierForTenant} — TWO synchronous D1-PRIMARY reads (`tier_selections`
 * WHERE active + `tenant.tier`) to build the server-trusted storage-quota header
 * and evaluate the request-count cap. Cloudflare D1's primary is ENAM (no
 * South-America region), so a SAM/GRU caller pays ~120 ms+ per request for it.
 * After slice 1 moved the residency read to KV, this tier pair is the next of
 * the "quota trio" (request-count / tier / storage-SUM) making up the residual
 * ~500 ms `wdb` phase (clw GRU probe, 2026-07-21).
 *
 * This mirrors the `pat`, `tsusp` and residency L2 exactly (ADR-0070):
 *
 *   L1  in-memory per-isolate  ({@link TIER_CACHE_TTL_MS}, micro-burst dedup)
 *   L2  Workers KV  (`ttier:<tenant>`, {@link KV_TIER_TTL_S}s — globally
 *       replicated with a PER-COLO edge cache, so it survives isolate fan-out and
 *       serves edge-local ~3 ms reads everywhere including SAM)
 *   L3  D1  ({@link getTierForTenant}, the source-of-truth on every L1+L2 miss)
 *
 * # Failure posture — FAIL-OPEN (like the suspend gate, unlike residency)
 *
 * A D1 fault on the tier read is NOT fatal: {@link getTierForTenant} already
 * returns `{ tier: 'free', d1Error: true }`, and the caller (`index.ts`) treats
 * `d1Error` as "unconfirmed" and SKIPS the storage-quota check (F21 fail-open
 * symmetry — never a false 429 for a paid tenant during a partial D1 outage).
 * This cache preserves that exactly: an `d1Error=true` result is returned to the
 * caller UNCHANGED and **never cached** (so a transient error can never pin a
 * tenant to 'free'); only a CONFIRMED tier (`d1Error=false`) is written to L1+L2.
 * The DO's deeper quota FSM is the allow-path safety net either way.
 *
 * # Why bounded staleness is SAFE here
 *
 * Unlike residency (immutable), a tier CHANGES on billing events. A stale cached
 * tier can only be wrong for `≤ KV_TIER_TTL_S` + the L1 slack:
 *   - a just-UPGRADED tenant keeps the lower cap for ≤ this window (briefly
 *     under-provisioned — self-corrects; a fast-follow KV-flush on the checkout
 *     write closes it, tracked below); and
 *   - a just-DOWNGRADED tenant keeps the higher cap for ≤ this window (briefly
 *     over-provisioned — bounded, and the authoritative downgrade also revokes
 *     the runners/paid entitlement via the signup-worker webhook path).
 * This is the SAME bounded-staleness trade-off ADR-0070 already ratifies for the
 * suspend gate (which caches the negative verdict), pinned to the same uniform
 * 60 s auth-freshness window. The worker tier only builds the storage-quota
 * HEADER + the request-count comparison; the container's quota FSM re-derives the
 * hard caps, so a stale worker tier is bounded, never authoritative.
 *
 * INV-NO-PII-IN-LOGS: nothing is logged here (no tenant id, no tier).
 */

import type { KvReader } from "./pat_verify_cache.js";
import { getTierForTenant, isValidTier, type Tier, type TierResult } from "./quota.js";

/**
 * Minimal D1 surface this module needs — just enough to hand to
 * {@link getTierForTenant}. `index.ts` passes `env.CONFIG_DB`.
 */
export type TierD1 = Parameters<typeof getTierForTenant>[0];

/**
 * L1 (per-isolate) TTL, ms. Matches the `pat`/`tsusp`/residency L1 (5 s) — with
 * the L2 KV tier carrying the far-D1 latency load, L1 reverts to pure
 * micro-burst dedup within one isolate, keeping the chained (L1 + L2) staleness
 * bound near the KV floor.
 */
export const TIER_CACHE_TTL_MS = 5_000;

/** KV key prefix for a cached tier (distinct from `patrow:`/`tsusp:`/`tres:`). */
const KV_TIER_PREFIX = "ttier:";

/**
 * L2 KV entry TTL, in SECONDS. 60 s is KV's floor and matches the residency /
 * suspend L2 — the same uniform ADR-0070 auth-freshness window. A billing tier
 * change propagates within ≤ this + the L1 slack; a fast-follow KV-flush on the
 * checkout/tier write is the tracked follow-up to make an upgrade instant.
 */
export const KV_TIER_TTL_S = 60;

/** The KV key for a tenant's cached tier. */
export function tierKvKey(tenantId: string): string {
  return KV_TIER_PREFIX + tenantId;
}

/** Options for {@link resolveTenantTierCached}. All optional (tests may pass none). */
export interface TierOpts {
  /** L2 Workers KV binding (METADATA_KV). Absent ⇒ L2 skipped (L1 + D1 only). */
  readonly kv?: KvReader;
  /**
   * `ctx.waitUntil` — extends the request lifetime so the KV write-behind
   * completes after the response returns (else a bare `void kv.put(...)` is
   * CANCELLED on response return — the #859 bug). Tests without it fall back to
   * `await`.
   */
  readonly waitUntil?: (p: Promise<unknown>) => void;
  /** Wall-clock ms (injectable for tests; defaults to `Date.now()`). */
  readonly nowMs?: number;
  /**
   * L3 fetch (injectable for tests). Defaults to {@link getTierForTenant}. Tests
   * inject a spy to assert single-flight / cache-hit-skips-D1 without mocking
   * D1's prepare/bind/first.
   */
  readonly fetch?: (db: TierD1, tenantId: string) => Promise<TierResult>;
}

/** Defensive ceiling on distinct tenants held in L1 (mirrors residency/suspend). */
const TIER_CACHE_CAP = 4096;

interface CachedTier {
  readonly tier: Tier;
  readonly fetchedAtMs: number;
}

/** Module-level (per-isolate) L1 cache, keyed by tenant_id. Insertion order = eviction order. */
const tierCache = new Map<string, CachedTier>();

/** Per-tenant single-flight: concurrent misses for one tenant share ONE read chain. */
const inflight = new Map<string, Promise<TierResult>>();

/** TEST-ONLY: reset the per-isolate singletons between cases. */
export function __resetTierCacheForTests(): void {
  tierCache.clear();
  inflight.clear();
}

/** Insert a confirmed tier, evicting to stay within {@link TIER_CACHE_CAP}. */
function putL1(tenantId: string, tier: Tier, nowMs: number): void {
  if (tierCache.size >= TIER_CACHE_CAP && !tierCache.has(tenantId)) {
    for (const [k, e] of tierCache) {
      if (nowMs - e.fetchedAtMs >= TIER_CACHE_TTL_MS) {
        tierCache.delete(k);
      }
    }
    if (tierCache.size >= TIER_CACHE_CAP) {
      const oldest = tierCache.keys().next().value;
      if (oldest !== undefined) {
        tierCache.delete(oldest);
      }
    }
  }
  tierCache.set(tenantId, { tier, fetchedAtMs: nowMs });
}

/**
 * Read + validate a cached tier from KV. Returns the {@link Tier} on a clean
 * hit, or `null` on miss / malformed value / KV fault (all ⇒ fall through to
 * D1). Never throws — a KV fault must not break the hot path.
 */
async function kvGetTier(kv: KvReader, tenantId: string): Promise<Tier | null> {
  let raw: string | null;
  try {
    raw = await kv.get(tierKvKey(tenantId));
  } catch {
    return null; // KV fault → miss → fall through to D1.
  }
  if (raw === null) {
    return null;
  }
  try {
    const o = JSON.parse(raw) as Record<string, unknown>;
    const tier = o["tier"];
    // Only trust a stored, still-valid tier name; anything else ⇒ miss.
    if (typeof tier === "string" && isValidTier(tier)) {
      return tier;
    }
  } catch {
    // malformed JSON → miss
  }
  return null;
}

/**
 * Resolve a tenant's effective tier through the L1 → L2 KV → L3 D1 cache.
 *
 * A CONFIRMED tier (`d1Error=false`) is served from L1/L2 when fresh and written
 * back on a miss; an ERROR result (`d1Error=true`) is returned to the caller
 * unchanged and never cached (a transient D1 fault must not pin a tenant to the
 * 'free' fallback). Drop-in for {@link getTierForTenant} on the hot path.
 */
export async function resolveTenantTierCached(
  db: TierD1,
  tenantId: string,
  opts: TierOpts = {},
): Promise<TierResult> {
  const nowMs = opts.nowMs ?? Date.now();
  const fetchTier = opts.fetch ?? getTierForTenant;

  // ── L1: per-isolate, fresh within TTL ──────────────────────────────────────
  const l1 = tierCache.get(tenantId);
  if (l1 !== undefined && nowMs - l1.fetchedAtMs < TIER_CACHE_TTL_MS) {
    return { tier: l1.tier, d1Error: false };
  }

  // ── Single-flight: collapse concurrent misses for the same tenant ──────────
  const existing = inflight.get(tenantId);
  if (existing !== undefined) {
    return existing;
  }

  const chain = (async (): Promise<TierResult> => {
    // ── L2: Workers KV (edge-local, survives isolate fan-out) ────────────────
    if (opts.kv !== undefined) {
      const kvTier = await kvGetTier(opts.kv, tenantId);
      if (kvTier !== null) {
        putL1(tenantId, kvTier, nowMs);
        return { tier: kvTier, d1Error: false };
      }
    }

    // ── L3: D1 source-of-truth ───────────────────────────────────────────────
    const result = await fetchTier(db, tenantId);

    // Only a CONFIRMED tier is cacheable — an error-derived 'free' must never be
    // pinned (it would fail a paid tenant open-but-wrong past the outage).
    if (!result.d1Error) {
      putL1(tenantId, result.tier, nowMs);
      if (opts.kv !== undefined) {
        const kv = opts.kv;
        const write = kv
          .put(tierKvKey(tenantId), JSON.stringify({ tier: result.tier }), {
            expirationTtl: KV_TIER_TTL_S,
          })
          .catch(() => {
            /* best-effort: a KV write fault must not fail the request */
          });
        if (opts.waitUntil !== undefined) {
          opts.waitUntil(write);
        } else {
          await write;
        }
      }
    }
    return result;
  })();

  inflight.set(tenantId, chain);
  try {
    return await chain;
  } finally {
    inflight.delete(tenantId);
  }
}
