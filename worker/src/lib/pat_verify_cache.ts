/**
 * Per-token PAT-verify cache for the Worker auth hot path (perf #99).
 *
 * # Why
 *
 * `extractAuth` runs on EVERY authenticated request. After the (cheap, in-memory)
 * HMAC possession proof it does a synchronous D1 read of the `pat` row
 * (`SELECT … FROM pat WHERE token_id = ?1 AND revoked_at_ms IS NULL`). Profiling
 * against prod (2026-07-19, a SAM/São-Paulo client) showed that D1-over-HTTP read
 * costs ~0.7 s per request — trans-continental to the D1 primary — and it is the
 * SOLE uncached read left on the warm hot path (the tenant suspend gate already
 * caches; see {@link ./tenant_suspend_gate.ts}). Isolation:
 *   - `/health` (no auth) .......................... ~40 ms
 *   - bad token (fails HMAC, PRE-D1) ............... ~45 ms
 *   - valid token, COLD isolate (pat + suspend read) ~1.4 s
 *   - valid token, WARM isolate (suspend cached) ... ~0.7 s  ← this uncached pat read
 * So caching the pat-row lookup collapses the warm auth floor from ~0.7 s toward
 * the ~45 ms HMAC-only cost. This is the exact cold-hydrate-herd pattern #667
 * fixed for the container tier read and the suspend gate mirrors on the edge.
 *
 * # Revocation-correctness (this is a credential cache — the load-bearing part)
 *
 * A cache hit skips the D1 `revoked_at_ms IS NULL` re-check, so a PAT revoked
 * mid-entry keeps access until the entry expires. We bound that window with a
 * short {@link PAT_VERIFY_CACHE_TTL_MS} TTL, sized against the governing SLO:
 *   - ADR-0030 `SLO-FRESH-PAT-REVOKE ≤ 60 s p99` — the customer revocation SLA
 *     is the MAX of its stale-window axes, so any in-region verify cache must
 *     stay WELL within 60 s.
 *   - The container's `NativePatGate` verify cache (the closest analog — also a
 *     PAT-verify cache) uses **5 s** (red-team C3 deliberately dropped it 60 s→5 s
 *     to bound native-plane revocation latency; ADR-0030 WP-D M26 addendum records
 *     it as an in-region consumer bounded within the 60 s SLA). We match it: 5 s.
 * Correctness rules that keep D1 the source-of-truth (INV-AUTH-NEON-IS-SOT):
 *   1. **Positive results ONLY are cached.** A `not_found` (unknown/revoked at
 *      read time) is NEVER cached, so a freshly-MINTED token works immediately
 *      (no negative-cache delay) and a revoked token is bounded solely by an
 *      existing positive entry's ≤5 s TTL.
 *   2. **Expiry is re-evaluated by the caller on every hit** from the cached
 *      `expires_ms` — expiry has ZERO added staleness; only revoke has the ≤5 s
 *      window.
 *   3. **A D1 fault is never cached and never serves a stale/expired entry.** On
 *      a miss-path D1 error we surface `error` so `extractAuth` returns its
 *      existing `d1_lookup_error → 503` (fail-closed, retryable) — identical
 *      posture to today. A still-FRESH hit (<TTL) is served through a transient
 *      D1 blip (that entry was DB-confirmed <5 s ago), which never extends the
 *      revocation window past the TTL.
 *
 * # Safety: why keying by `token_id` post-HMAC is sound
 *
 * The cache is consulted ONLY after `verifyPatHmacMulti` proves the presented
 * `<token_id>.<secret>` was signed by `PAT_SIGNING_KEY`. A caller with a wrong
 * secret fails HMAC and never reaches the cache; the cache stores only the D1
 * row (tenant / scope / expiry / runner marker), never the secret — so a hit
 * leaks nothing and grants nothing without an independent possession proof.
 *
 * INV-NO-PII-IN-LOGS: nothing is logged here; token_id is a non-secret handle.
 */

import type { D1Database } from "@cloudflare/workers-types";

/**
 * A read-capable D1 handle — either the `D1Database` primary or a
 * `D1Database.withSession(...)` replica session (D1 read replication). Both
 * expose `prepare`, which is all this cache needs. Typed structurally so a
 * session (whose full type varies by @cloudflare/workers-types version) is
 * accepted without a hard dependency on `D1DatabaseSession`.
 */
export type D1Reader = Pick<D1Database, "prepare">;

/**
 * The verified `pat` row cached on the hot path — byte-for-byte the columns
 * `extractAuth` selects and returns. Positive (found, non-revoked) rows only.
 */
export interface CachedPatRow {
  readonly tenant_id: string;
  readonly expires_ms: number;
  /** D1 `pat.scope` (may be NULL on older rows; caller normalises to ""). */
  readonly scope: string | null;
  /** D1 `pat.runner_job_ac_key` (NULL on normal PATs; set on runner-minted). */
  readonly runner_job_ac_key: string | null;
}

/**
 * Outcome of a cached verify. `found`/`not_found` are transparent to the caller
 * (same decisions as the raw D1 read); `error` is a D1 fault the caller maps to
 * 503 — preserving the pre-cache fail-closed posture exactly.
 */
export type PatVerifyResult =
  | { readonly kind: "found"; readonly row: CachedPatRow }
  | { readonly kind: "not_found" }
  | { readonly kind: "error" };

/**
 * TTL for a cached POSITIVE verify, in ms. 5 s — matches the container
 * `NativePatGate` verify cache and stays well within ADR-0030's 60 s p99
 * revocation SLA. This is the maximum time a mid-entry-revoked PAT can keep
 * edge access.
 */
export const PAT_VERIFY_CACHE_TTL_MS = 5_000;

/**
 * Upper bound on distinct cached tokens (per-isolate). Mirrors the suspend
 * gate's defensive ceiling so the module-level map cannot grow without bound
 * under a wide token fan-out. On overflow: evict expired entries first, then
 * the oldest (insertion-order) entry.
 */
const PAT_VERIFY_CACHE_CAP = 8192;

/** One cached positive verify: the resolved row plus when it was fetched. */
interface CachedEntry {
  readonly row: CachedPatRow;
  readonly fetchedAtMs: number;
}

/**
 * Module-level (per-isolate) cache. Map insertion order gives cheap
 * oldest-first eviction. Keyed by the non-secret `token_id`.
 */
const patCache = new Map<string, CachedEntry>();

/**
 * Per-token single-flight map: concurrent misses for the SAME token_id share
 * ONE D1 read (collapses a same-token burst instead of stampeding D1). Entries
 * are removed as soon as the read settles.
 */
const inflight = new Map<string, Promise<PatVerifyResult>>();

/** Insert a positive entry, evicting to stay within {@link PAT_VERIFY_CACHE_CAP}. */
function putCache(tokenId: string, row: CachedPatRow, nowMs: number): void {
  if (patCache.size >= PAT_VERIFY_CACHE_CAP && !patCache.has(tokenId)) {
    // Drop expired entries first.
    for (const [k, e] of patCache) {
      if (nowMs - e.fetchedAtMs >= PAT_VERIFY_CACHE_TTL_MS) {
        patCache.delete(k);
      }
    }
    // Still full → evict the oldest (insertion-order) entry.
    if (patCache.size >= PAT_VERIFY_CACHE_CAP) {
      const oldest = patCache.keys().next().value;
      if (oldest !== undefined) {
        patCache.delete(oldest);
      }
    }
  }
  patCache.set(tokenId, { row, fetchedAtMs: nowMs });
}

const PAT_ROW_SQL =
  "SELECT tenant_id, expires_ms, scope, runner_job_ac_key FROM pat WHERE token_id = ?1 AND revoked_at_ms IS NULL LIMIT 1";

/**
 * Read the `pat` row, preferring the replica `readDb` and falling back to the
 * `primaryDb` when the replica is stale or faulted. Returns the row, or `null`
 * only when it is genuinely absent (confirmed against the primary). Throws only
 * if BOTH handles fault.
 *
 * The fallback is what makes D1 read replication SAFE for auth:
 * - **Freshness (read-after-write):** a freshly-MINTED PAT is written to the
 *   primary; the client may present it before it has replicated. A replica read
 *   would return `null` → a spurious 401. So a `null` from the replica is
 *   re-checked against the PRIMARY before we reject — a new token authenticates
 *   immediately. A genuinely unknown/revoked token is `null` on both.
 * - **Availability:** a replica fault falls back to the primary rather than 503.
 * - **Revocation staleness is intentionally NOT re-checked here:** a *non-null*
 *   stale replica read (a token revoked on the primary but not yet replicated)
 *   is honored, bounding the revocation window to the replication lag — the same
 *   ≤lag window the whole read-replica auth path accepts, well within ADR-0030's
 *   `SLO-FRESH-PAT-REVOKE ≤ 60 s p99` (D1 replica lag is sub-second in practice).
 */
async function readPatRow(
  readDb: D1Reader,
  primaryDb: D1Reader | undefined,
  tokenId: string,
): Promise<CachedPatRow | null> {
  const canFallBack = primaryDb !== undefined && primaryDb !== readDb;
  try {
    const row = await readDb.prepare(PAT_ROW_SQL).bind(tokenId).first<CachedPatRow>();
    if (row !== null) {
      return row;
    }
    // Replica miss — could be a not-yet-replicated fresh mint. Confirm on primary.
    return canFallBack
      ? await primaryDb!.prepare(PAT_ROW_SQL).bind(tokenId).first<CachedPatRow>()
      : null;
  } catch (replicaErr) {
    // Replica fault — fall back to the primary for availability.
    if (canFallBack) {
      return primaryDb!.prepare(PAT_ROW_SQL).bind(tokenId).first<CachedPatRow>();
    }
    throw replicaErr;
  }
}

/**
 * Resolve the `pat` row for `tokenId`, served from a per-isolate 5 s cache with
 * single-flight. MUST be called only AFTER the HMAC possession proof (see the
 * safety note in this file's header). The SQL and its columns are identical to
 * the pre-cache `extractAuth` read, so decisions are unchanged except for the
 * ≤{@link PAT_VERIFY_CACHE_TTL_MS} revocation window on a cache hit.
 *
 * @param readDb   The read handle for the lookup — a `withSession(...)` replica
 *                 session (fast, near the caller) or the primary. Reads route
 *                 here first.
 * @param tokenId  The parsed, non-secret token_id (D1 lookup key).
 * @param opts.primaryDb  The primary `CONFIG_DB`, used as the read-after-write /
 *                 availability fallback (see {@link readPatRow}). Omit (or pass
 *                 the same handle as `readDb`) to read a single handle only.
 * @param opts.nowMs      Wall-clock ms (injectable for tests; defaults to Date.now()).
 */
export async function verifyPatRowCached(
  readDb: D1Reader,
  tokenId: string,
  opts: { primaryDb?: D1Reader; nowMs?: number } = {},
): Promise<PatVerifyResult> {
  const nowMs = opts.nowMs ?? Date.now();
  // ── Fresh positive hit ──────────────────────────────────────────────────────
  // Served even through a transient D1 blip: the entry was DB-confirmed < TTL
  // ago, so this never extends the revocation window past the TTL.
  const cached = patCache.get(tokenId);
  if (cached !== undefined && nowMs - cached.fetchedAtMs < PAT_VERIFY_CACHE_TTL_MS) {
    return { kind: "found", row: cached.row };
  }

  // ── Single-flight: collapse concurrent misses to one D1 read ────────────────
  const existing = inflight.get(tokenId);
  if (existing !== undefined) {
    return existing;
  }

  const flight = (async (): Promise<PatVerifyResult> => {
    try {
      const row = await readPatRow(readDb, opts.primaryDb, tokenId);
      if (row === null) {
        // Unknown OR revoked-at-read-time (confirmed against the primary). Do NOT
        // cache negatives — a token minted a moment ago must authenticate
        // immediately, and a revoked token stays bounded solely by any existing
        // positive entry's TTL. (Any stale positive entry is left to expire; it
        // is never refreshed from a null read.) D1 stays the source-of-truth.
        return { kind: "not_found" };
      }
      putCache(tokenId, row, nowMs);
      return { kind: "found", row };
    } catch {
      // Both handles faulted — do NOT cache, and do NOT fall back to a
      // stale/expired entry (that would push the revocation window past the
      // TTL). Surface the fault so extractAuth returns d1_lookup_error → 503
      // (unchanged fail-closed, retryable posture).
      return { kind: "error" };
    } finally {
      inflight.delete(tokenId);
    }
  })();
  inflight.set(tokenId, flight);
  return flight;
}

/**
 * Test-only: clear the module-level cache and in-flight map so cases do not
 * leak cached rows into one another.
 */
export function __resetPatVerifyCacheForTest(): void {
  patCache.clear();
  inflight.clear();
}
