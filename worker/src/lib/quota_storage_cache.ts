/**
 * Tenant STORAGE-SUM three-tier cache for the customer CAS/AC hot path — latency
 * WP-B1 (the last of the "quota trio" — request-count / tier / storage-SUM — still
 * making a synchronous D1-PRIMARY round trip on a warm READ).
 *
 * # Why
 *
 * `checkStorageQuota` (`quota.ts`) runs `SELECT SUM(bytes_used) …` on EVERY
 * request of a finite-storage tier — i.e. every tier except `enterprise` —
 * INCLUDING GET/HEAD reads. Cloudflare D1's primary is ENAM (no South-America
 * region), so a far caller pays ~120 ms+ per request for a value that a READ
 * cannot even change. After WP-A/WP-C collapsed the `_public` blob + map reads to
 * the colo edge, and slice-2 cached the tier, this storage SUM is what still makes
 * a real (finite-storage) customer pay a synchronous cross-colo D1 read on a warm
 * cache HIT — roughly doubling the residual vs. an unlimited-storage tenant (which
 * skips the SUM).
 *
 * This mirrors the `pat` / `tsusp` / `ttier` / residency L2 EXACTLY (ADR-0070):
 *
 *   L1  in-memory per-isolate  ({@link STORAGE_CACHE_TTL_MS}, micro-burst dedup)
 *   L2  Workers KV  (`qstor:<tenant>`, {@link KV_STORAGE_TTL_S}s — globally
 *       replicated with a PER-COLO edge cache; survives isolate fan-out, ~3 ms
 *       edge-local everywhere including SAM)
 *   L3  D1  (`SUM(bytes_used)` — the source-of-truth on every L1+L2 miss)
 *
 * # What is cached — the BYTES, not the verdict
 *
 * The cache stores the `SUM(bytes_used)` number, NOT the ok/over-cap verdict. The
 * cheap cap COMPARISON (`storageResultForBytes(bytes, tier)`) is recomputed live
 * against the caller's freshly-resolved tier on every hit. So a tier change (an
 * upgrade/downgrade resolved by the tier cache) is reflected INSTANTLY — only the
 * bytes are ≤ (KV TTL + L1 slack) stale, and the expensive part (the SUM) is what
 * we avoid. One number, one source of staleness.
 *
 * # Why bounded staleness is SAFE — and READS ONLY
 *
 * `SUM(bytes_used)` moves ONLY on a WRITE (a PUT/POST fill adds bytes; a
 * DELETE/erase removes them). A READ cannot grow storage. So a ≤65 s-stale byte
 * count on a READ can only be wrong in the customer-favorable direction: a tenant
 * who JUST crossed their cap via a write keeps being allowed to READ for ≤65 s
 * (reads cannot make it worse). It can NEVER wrongly 402 an under-cap tenant,
 * because only a confirmed SUM is cached. This is the same uniform ADR-0070
 * freshness window already ratified for tier/suspend.
 *
 * **This cache is for NON-MUTATING requests only.** A PUT/POST that ADDS bytes
 * must see LIVE storage before it is allowed past the cap — that is the whole
 * point of storage enforcement. Callers keep resolving writes via the live
 * `checkStorageQuota` / `runQuotaBatch` path; only GET/HEAD consult this cache.
 * The existing "reads fail-open, writes fail-closed" posture is preserved: this
 * module is a read-only accelerant, never on the write path.
 *
 * # Failure posture — FAIL-OPEN, never cache an error
 *
 * A D1 fault on the SUM read yields `d1Error:true`; the caller treats that as the
 * read-side fail-open (`ok:true`) and it is **never cached** (a transient fault
 * can never pin a tenant to a wrong byte count). Only a CONFIRMED SUM is written
 * to L1+L2 — identical to the tier cache's `d1Error` rule.
 *
 * INV-NO-PII-IN-LOGS: nothing is logged here (no tenant id, no bytes).
 */

import type { KvReader } from "./pat_verify_cache.js";
import {
  storageResultForBytes,
  storageSumStatement,
  type QuotaCheckResult,
  type StorageSumRow,
  type Tier,
} from "./quota.js";

/** Minimal D1 surface this module needs (index.ts passes `env.CONFIG_DB`). */
export type StorageD1 = Parameters<typeof storageSumStatement>[0];

/**
 * L1 (per-isolate) TTL, ms. Matches the `pat`/`tsusp`/`ttier`/residency L1 (5 s):
 * with the L2 KV carrying the far-D1 load, L1 is pure micro-burst dedup within one
 * isolate, keeping the chained (L1 + L2) staleness bound near the KV floor.
 */
export const STORAGE_CACHE_TTL_MS = 5_000;

/** KV key prefix for a cached storage SUM (distinct from `patrow:`/`ttier:`/…). */
const KV_STORAGE_PREFIX = "qstor:";

/**
 * L2 KV entry TTL, in SECONDS. 60 s is KV's floor and matches the tier / suspend
 * L2 — the same uniform ADR-0070 auth-freshness window. A byte-count change from a
 * write propagates within ≤ this + the L1 slack.
 */
export const KV_STORAGE_TTL_S = 60;

/** The KV key for a tenant's cached storage SUM. */
export function storageKvKey(tenantId: string): string {
  return KV_STORAGE_PREFIX + tenantId;
}

/** Result of the cached byte-count resolution. */
export interface StorageBytesResult {
  /** The SUM(bytes_used) for the tenant (0 when the row is absent). */
  readonly totalBytes: number;
  /** True iff the SUM could not be confirmed (D1 fault) — caller fails open, never cached. */
  readonly d1Error: boolean;
}

/** Options for {@link resolveStorageBytesCached}. All optional (tests may pass none). */
export interface StorageOpts {
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
   * L3 fetch (injectable for tests). Defaults to the live `SUM(bytes_used)` read.
   * Tests inject a spy to assert single-flight / cache-hit-skips-D1.
   */
  readonly fetch?: (db: StorageD1, tenantId: string) => Promise<StorageBytesResult>;
}

/** Defensive ceiling on distinct tenants held in L1 (mirrors tier/residency/suspend). */
const STORAGE_CACHE_CAP = 4096;

interface CachedBytes {
  readonly totalBytes: number;
  readonly fetchedAtMs: number;
}

/** Module-level (per-isolate) L1 cache, keyed by tenant_id. Insertion order = eviction order. */
const bytesCache = new Map<string, CachedBytes>();

/** Per-tenant single-flight: concurrent misses for one tenant share ONE read chain. */
const inflight = new Map<string, Promise<StorageBytesResult>>();

/** TEST-ONLY: reset the per-isolate singletons between cases. */
export function __resetStorageCacheForTests(): void {
  bytesCache.clear();
  inflight.clear();
}

/** Insert a confirmed byte count, evicting to stay within {@link STORAGE_CACHE_CAP}. */
function putL1(tenantId: string, totalBytes: number, nowMs: number): void {
  if (bytesCache.size >= STORAGE_CACHE_CAP && !bytesCache.has(tenantId)) {
    for (const [k, e] of bytesCache) {
      if (nowMs - e.fetchedAtMs >= STORAGE_CACHE_TTL_MS) {
        bytesCache.delete(k);
      }
    }
    if (bytesCache.size >= STORAGE_CACHE_CAP) {
      const oldest = bytesCache.keys().next().value;
      if (oldest !== undefined) {
        bytesCache.delete(oldest);
      }
    }
  }
  bytesCache.set(tenantId, { totalBytes, fetchedAtMs: nowMs });
}

/**
 * Read + validate a cached byte count from KV. Returns the number on a clean hit,
 * or `null` on miss / malformed value / KV fault (all ⇒ fall through to D1). Never
 * throws — a KV fault must not break the hot path.
 */
async function kvGetBytes(kv: KvReader, tenantId: string): Promise<number | null> {
  let raw: string | null;
  try {
    raw = await kv.get(storageKvKey(tenantId));
  } catch {
    return null; // KV fault → miss → fall through to D1.
  }
  if (raw === null) {
    return null;
  }
  try {
    const o = JSON.parse(raw) as Record<string, unknown>;
    const bytes = o["bytes"];
    // Only trust a finite non-negative number; anything else ⇒ miss.
    if (typeof bytes === "number" && Number.isFinite(bytes) && bytes >= 0) {
      return bytes;
    }
  } catch {
    // malformed JSON → miss
  }
  return null;
}

/** Default L3: the live `SUM(bytes_used)` read, translated to {@link StorageBytesResult}. */
async function fetchStorageBytes(db: StorageD1, tenantId: string): Promise<StorageBytesResult> {
  try {
    const row = await storageSumStatement(db, tenantId).first<StorageSumRow>();
    return { totalBytes: row?.total_bytes ?? 0, d1Error: false };
  } catch {
    return { totalBytes: 0, d1Error: true };
  }
}

/**
 * Resolve a tenant's `SUM(bytes_used)` through the L1 → L2 KV → L3 D1 cache.
 *
 * A CONFIRMED count (`d1Error=false`) is served from L1/L2 when fresh and written
 * back on a miss; an ERROR result (`d1Error=true`) is returned to the caller
 * unchanged and never cached. The caller turns the bytes into a verdict live via
 * `storageResultForBytes(bytes, tier)`.
 */
export async function resolveStorageBytesCached(
  db: StorageD1,
  tenantId: string,
  opts: StorageOpts = {},
): Promise<StorageBytesResult> {
  const nowMs = opts.nowMs ?? Date.now();
  const doFetch = opts.fetch ?? fetchStorageBytes;

  // ── L1: per-isolate, fresh within TTL ──────────────────────────────────────
  const l1 = bytesCache.get(tenantId);
  if (l1 !== undefined && nowMs - l1.fetchedAtMs < STORAGE_CACHE_TTL_MS) {
    return { totalBytes: l1.totalBytes, d1Error: false };
  }

  // ── Single-flight: collapse concurrent misses for the same tenant ──────────
  const existing = inflight.get(tenantId);
  if (existing !== undefined) {
    return existing;
  }

  const chain = (async (): Promise<StorageBytesResult> => {
    // ── L2: Workers KV (edge-local, survives isolate fan-out) ────────────────
    if (opts.kv !== undefined) {
      const kvBytes = await kvGetBytes(opts.kv, tenantId);
      if (kvBytes !== null) {
        putL1(tenantId, kvBytes, nowMs);
        return { totalBytes: kvBytes, d1Error: false };
      }
    }

    // ── L3: D1 source-of-truth ───────────────────────────────────────────────
    const result = await doFetch(db, tenantId);

    // Only a CONFIRMED count is cacheable — an error must never be pinned.
    if (!result.d1Error) {
      putL1(tenantId, result.totalBytes, nowMs);
      if (opts.kv !== undefined) {
        const kv = opts.kv;
        const write = kv
          .put(storageKvKey(tenantId), JSON.stringify({ bytes: result.totalBytes }), {
            expirationTtl: KV_STORAGE_TTL_S,
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

/**
 * Cached storage-quota check for a NON-MUTATING (GET/HEAD) request — a drop-in for
 * `checkStorageQuota(db, tenant, tier, isMutating=false)` that avoids the
 * synchronous SUM on a warm hit. Semantics are byte-identical to the live check:
 *   - unconfirmed tier (`d1Error`) → fail OPEN (`ok:true`), nothing cached;
 *   - unlimited-storage tier → `ok:true`, no SUM at all;
 *   - otherwise → `storageResultForBytes(cachedBytes, tier)`, the SAME verdict
 *     (and the same `reason` string) the live path returns from the same bytes.
 *
 * WRITES MUST NOT use this — they need the live SUM before adding bytes.
 */
export async function checkStorageQuotaCachedRead(
  db: StorageD1,
  tenantId: string,
  tier: Tier,
  tierD1Error: boolean,
  storageCapFinite: boolean,
  opts: StorageOpts = {},
): Promise<QuotaCheckResult> {
  // Unconfirmed tier → read-side fail-open (identical to checkStorageQuota's first
  // branch for isMutating=false). Never cache.
  if (tierD1Error) {
    return { ok: true };
  }
  // Unlimited-storage tier (enterprise) → nothing to check, no SUM.
  if (!storageCapFinite) {
    return { ok: true };
  }
  const { totalBytes, d1Error } = await resolveStorageBytesCached(db, tenantId, opts);
  if (d1Error) {
    // Read-side fail-open on a SUM fault (availability), consistent with the live path.
    return { ok: true };
  }
  return storageResultForBytes(totalBytes, tier);
}
