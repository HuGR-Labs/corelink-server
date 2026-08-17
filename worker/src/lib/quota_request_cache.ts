/**
 * Tenant REQUEST-COUNT fast path — latency WP-B2. Takes the monthly metering
 * UPSERT off the WARM read path for tenants that are provably far from their cap,
 * while keeping the cap boundary enforced EXACTLY.
 *
 * # Why
 *
 * The monthly request-count UPSERT (`incrementMonthlyRequestCount` /
 * `runQuotaBatch`) is a synchronous D1-PRIMARY write on every metered request.
 * After WP-A/WP-C/slice-2 collapsed blob+map+tier to the colo edge, that one write
 * is the ~44 ms residual on a warm same-region `_public` cache HIT (measured from a
 * US runner box, 2026-08-16). A cache HIT that pays a cross-colo D1 write is the
 * last thing between the campaign target (20–50 ms warm) and reality.
 *
 * # The idea — enforce exactly near the cap, meter asynchronously far from it
 *
 * The counter must both ENFORCE (429 at the cap) and ADVANCE. Split by headroom:
 *
 *   - **Far under cap** (`cap - cachedCount > {@link burstMargin}(cap)`): serve
 *     immediately; the increment runs via `ctx.waitUntil` (off the hot path) and
 *     refreshes the KV count from the authoritative `RETURNING` value. NO
 *     synchronous D1 on the request path.
 *   - **Near cap, KV miss, unconfirmed tier, or a WRITE verb**: fall back to the
 *     EXACT synchronous path (`runQuotaBatch`) unchanged — byte-identical to
 *     today. The caller owns that fallback; this module returns `null` to signal
 *     it.
 *
 * Because the fast path is entered ONLY with proven headroom, a tenant within
 * `burstMargin` of its cap is ALWAYS counted exactly and 429'd exactly at the
 * boundary. The only softness is a transient over-serve bounded by `burstMargin`
 * for a tenant far under cap — and requests are a CAP (a 429), never a $-metered
 * charge, so that softness is customer-favorable and never over-bills.
 *
 * # Correctness invariants (see the ADR 2026-08-17-adr-edge-async-metering)
 *
 *   - **Never OVER-count.** KV is refreshed from the D1 `RETURNING` value, never a
 *     blind local `+1` that could double-count across isolates. A dropped
 *     `waitUntil` write UNDER-counts — the direction this fail-open counter already
 *     tolerates — never over-counts (which would wrongly 429 a paying tenant).
 *   - **The boundary is exact.** `burstMargin(cap)` is chosen to exceed the max
 *     requests a tenant could issue within the KV staleness window (60 s), so a
 *     full-window lag can never carry a tenant past `cap`; anyone that close takes
 *     the synchronous path.
 *   - **`d1Error` never uses the fast path** — an unconfirmed tier falls back to
 *     the exact path (preserves the F21 fail-open posture verbatim).
 *   - **Fan-out / metering-disabled requests are not this module's concern** — the
 *     caller passes `meter=false` and never calls in.
 *
 * INV-NO-PII-IN-LOGS: nothing is logged here.
 */

import type { KvReader } from "./pat_verify_cache.js";
import {
  currentYearMonthUtc,
  incrementMonthlyRequestCount,
  QUOTAS,
  type Tier,
} from "./quota.js";

/** L1 (per-isolate) TTL, ms — pure micro-burst dedup within one isolate (5 s). */
export const REQUEST_CACHE_TTL_MS = 5_000;

/** KV key prefix for a cached monthly count (distinct from `ttier:`/`qstor:`/…). */
const KV_REQUEST_PREFIX = "qreq:";

/**
 * L2 KV entry TTL, in SECONDS. 60 s — the same uniform ADR-0070 window as the
 * tier / storage caches. Also the staleness bound that {@link burstMargin} must
 * dominate. A fresh month is a new key (miss → sync repopulate) so a stale count
 * can never leak across a month boundary.
 */
export const KV_REQUEST_TTL_S = 60;

/**
 * Burst-margin sizing. A tenant is only served on the async fast path when it is
 * MORE than this many requests below its cap. The margin must exceed the largest
 * number of requests a single tenant could plausibly issue within the KV
 * staleness window (≤ {@link KV_REQUEST_TTL_S}s + the L1 slack), across all colos,
 * so that even a fully-stale cached count cannot carry the tenant past the real
 * cap on the fast path.
 *
 * `max(FLOOR, cap * FRACTION)`:
 *   - FRACTION 5 % scales the margin with the cap (bigger tiers, bigger bursts).
 *   - FLOOR 25 000 dominates the smallest cap (free = 500 000/mo). 25 000 requests
 *     in 60 s is 416 req/s sustained — orders of magnitude beyond any real free
 *     tenant (whose monthly cap averages 0.19 req/s), so the free boundary is
 *     enforced exactly with an enormous safety factor.
 */
export const BURST_MARGIN_FRACTION = 0.05;
export const BURST_MARGIN_FLOOR = 25_000;

/** The burst margin for a finite cap. */
export function burstMargin(cap: number): number {
  return Math.max(BURST_MARGIN_FLOOR, Math.floor(cap * BURST_MARGIN_FRACTION));
}

/** The KV key for a tenant's cached monthly count (month-scoped). */
export function requestKvKey(tenantId: string, yearMonth: string): string {
  return `${KV_REQUEST_PREFIX}${tenantId}:${yearMonth}`;
}

/** Options for {@link tryFastRequestCount} / {@link populateRequestCountKv}. */
export interface RequestCacheOpts {
  /** L2 Workers KV binding (METADATA_KV). Absent ⇒ the fast path cannot arm (returns null). */
  readonly kv?: KvReader;
  /** `ctx.waitUntil` — carries the async increment past response return. */
  readonly waitUntil?: (p: Promise<unknown>) => void;
  /** Wall-clock ms (injectable for tests; defaults to `Date.now()`). */
  readonly nowMs?: number;
  /** Year-month bucket (injectable for tests; defaults to {@link currentYearMonthUtc}). */
  readonly yearMonth?: string;
  /** L3 increment (injectable for tests). Defaults to {@link incrementMonthlyRequestCount}. */
  readonly increment?: (db: RequestD1, tenantId: string) => Promise<{ counted: boolean; count: number }>;
}

/** Minimal D1 surface (index.ts passes `env.CONFIG_DB`). */
export type RequestD1 = Parameters<typeof incrementMonthlyRequestCount>[0];

/** A taken fast-path decision — the tenant is served, the increment is async. */
export interface FastRequestResult {
  /** Best-known count at decision time (for telemetry only; NOT an enforcement value). */
  readonly count: number;
}

/**
 * The pure arming decision (NO side effect — reads the cache only). Shadow mode
 * uses this to log whether the fast path WOULD arm on real traffic without
 * double-counting; the `on` path uses it before scheduling the async increment.
 */
export interface FastPathDecision {
  /** True iff the tenant is provably far enough under its cap to serve async. */
  readonly arm: boolean;
  /** The cached count the decision was based on (0 for an uncapped tier). */
  readonly cachedCount: number;
  /** Why it did not arm (for shadow telemetry); `null` when it armed. */
  readonly reason: "uncapped-armed" | "no-kv" | "kv-miss" | "near-cap" | null;
}

const REQUEST_CACHE_CAP = 4096;

interface CachedCount {
  count: number;
  readonly fetchedAtMs: number;
}

/** Per-isolate L1, keyed by `<tenant>:<ym>`. Insertion order = eviction order. */
const countCache = new Map<string, CachedCount>();

/** TEST-ONLY: reset the per-isolate singleton between cases. */
export function __resetRequestCacheForTests(): void {
  countCache.clear();
}

function l1Key(tenantId: string, ym: string): string {
  return `${tenantId}:${ym}`;
}

function putL1(key: string, count: number, nowMs: number): void {
  if (countCache.size >= REQUEST_CACHE_CAP && !countCache.has(key)) {
    for (const [k, e] of countCache) {
      if (nowMs - e.fetchedAtMs >= REQUEST_CACHE_TTL_MS) {
        countCache.delete(k);
      }
    }
    if (countCache.size >= REQUEST_CACHE_CAP) {
      const oldest = countCache.keys().next().value;
      if (oldest !== undefined) {
        countCache.delete(oldest);
      }
    }
  }
  countCache.set(key, { count, fetchedAtMs: nowMs });
}

/** Read a cached count: L1 (fresh) → KV. Returns null on any miss / fault. */
async function readCachedCount(
  kv: KvReader | undefined,
  tenantId: string,
  ym: string,
  nowMs: number,
): Promise<number | null> {
  const key = l1Key(tenantId, ym);
  const l1 = countCache.get(key);
  if (l1 !== undefined && nowMs - l1.fetchedAtMs < REQUEST_CACHE_TTL_MS) {
    return l1.count;
  }
  if (kv === undefined) {
    return null;
  }
  let raw: string | null;
  try {
    raw = await kv.get(requestKvKey(tenantId, ym));
  } catch {
    return null;
  }
  if (raw === null) {
    return null;
  }
  try {
    const o = JSON.parse(raw) as Record<string, unknown>;
    const count = o["count"];
    if (typeof count === "number" && Number.isFinite(count) && count >= 0) {
      putL1(key, count, nowMs);
      return count;
    }
  } catch {
    /* malformed → miss */
  }
  return null;
}

/** Write the authoritative count to KV (best-effort; a fault must not fail the request). */
async function putKvCount(kv: KvReader, tenantId: string, ym: string, count: number): Promise<void> {
  await kv
    .put(requestKvKey(tenantId, ym), JSON.stringify({ count }), { expirationTtl: KV_REQUEST_TTL_S })
    .catch(() => {
      /* best-effort */
    });
}

/**
 * Populate the request-count KV from an authoritative D1 count observed on the
 * SYNCHRONOUS fallback path (`runQuotaBatch`). Called by the caller after the slow
 * path so subsequent requests for this tenant can arm the fast path. Also seeds
 * L1. Best-effort; never throws.
 */
export function populateRequestCountKv(
  tenantId: string,
  count: number,
  opts: RequestCacheOpts = {},
): void {
  const nowMs = opts.nowMs ?? Date.now();
  const ym = opts.yearMonth ?? currentYearMonthUtc();
  putL1(l1Key(tenantId, ym), count, nowMs);
  if (opts.kv !== undefined) {
    const kv = opts.kv;
    const write = putKvCount(kv, tenantId, ym, count);
    if (opts.waitUntil !== undefined) {
      opts.waitUntil(write);
    } else {
      void write;
    }
  }
}

/**
 * Attempt the async fast path for a metered, tier-CONFIRMED, NON-mutating request.
 *
 * Returns a {@link FastRequestResult} (the tenant is served; the increment has been
 * scheduled off the hot path) ONLY when the tenant is provably far under its cap.
 * Returns `null` — meaning "fall back to the exact synchronous path" — when:
 *   - there is no KV binding (cannot maintain the cached count safely), OR
 *   - the cached count is a miss (must sync-read to establish the count), OR
 *   - the tenant is within {@link burstMargin} of its cap (enforce exactly).
 *
 * A tier whose request axis is uncapped (`team`/`enterprise`, MAX_SAFE_INTEGER) is
 * always fast (nothing to enforce) — it still meters asynchronously for telemetry.
 *
 * The caller must have already established: `meter === true` (not a fan-out /
 * kill-switch) and `tierResult.d1Error === false`.
 */
export async function decideFastPath(
  tenantId: string,
  tier: Tier,
  opts: RequestCacheOpts = {},
): Promise<FastPathDecision> {
  const nowMs = opts.nowMs ?? Date.now();
  const ym = opts.yearMonth ?? currentYearMonthUtc();
  const cap = QUOTAS[tier].requestsPerMonthMax;
  const uncapped = cap === Number.MAX_SAFE_INTEGER;

  // Uncapped request axis (team/enterprise) → no boundary; always armed.
  if (uncapped) {
    return { arm: true, cachedCount: 0, reason: "uncapped-armed" };
  }
  // A capped tier needs a KV-backed cached count to prove headroom safely.
  if (opts.kv === undefined) {
    return { arm: false, cachedCount: 0, reason: "no-kv" };
  }
  const c = await readCachedCount(opts.kv, tenantId, ym, nowMs);
  if (c === null) {
    return { arm: false, cachedCount: 0, reason: "kv-miss" };
  }
  if (cap - c <= burstMargin(cap)) {
    return { arm: false, cachedCount: c, reason: "near-cap" };
  }
  return { arm: true, cachedCount: c, reason: null };
}

/**
 * Attempt the async fast path for a metered, tier-CONFIRMED, NON-mutating request.
 * Composes {@link decideFastPath} with the async increment. Returns a
 * {@link FastRequestResult} when armed (served; increment scheduled off-path), or
 * `null` to signal the caller to fall back to the exact synchronous path.
 */
export async function tryFastRequestCount(
  db: RequestD1,
  tenantId: string,
  tier: Tier,
  opts: RequestCacheOpts = {},
): Promise<FastRequestResult | null> {
  const nowMs = opts.nowMs ?? Date.now();
  const ym = opts.yearMonth ?? currentYearMonthUtc();
  const decision = await decideFastPath(tenantId, tier, opts);
  if (!decision.arm) {
    return null;
  }
  const cap = QUOTAS[tier].requestsPerMonthMax;
  const uncapped = cap === Number.MAX_SAFE_INTEGER;
  const cachedCount = decision.cachedCount;

  // ── Armed: serve now, meter asynchronously, refresh KV from the RETURNING count.
  // The caller only enters here with metering ENABLED, so the increment always runs
  // (the `true` flag). A dropped async write under-counts (tolerated), never over.
  const doIncrement =
    opts.increment ?? ((d: RequestD1, t: string) => incrementMonthlyRequestCount(d, t, true));
  const work = (async (): Promise<void> => {
    const inc = await doIncrement(db, tenantId);
    if (inc.counted && opts.kv !== undefined) {
      await putKvCount(opts.kv, tenantId, ym, inc.count);
    }
  })().catch(() => {
    /* a dropped async increment under-counts (tolerated fail-open); never fatal */
  });
  if (opts.waitUntil !== undefined) {
    opts.waitUntil(work);
  } else {
    await work;
  }

  // Optimistic L1 bump (+1) so subsequent same-isolate reads see the count grow.
  // This only TIGHTENS the headroom gate (never loosens it) — a conservative,
  // enforcement-safe local estimate; it is never used as an authoritative count.
  if (!uncapped) {
    putL1(l1Key(tenantId, ym), cachedCount + 1, nowMs);
  }

  return { count: cachedCount };
}
