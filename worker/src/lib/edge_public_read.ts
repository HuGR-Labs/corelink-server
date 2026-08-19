// Worker-native `_public` cache-HIT read path (F3.3).
//
// Serves a brew/pip `_public` cache HIT directly from the Worker edge —
// `env.CONFIG_DB` map+blocklist read → `env.CAS_BUCKET.get` (native R2) →
// blake3 re-verify — bypassing the Durable Object + Rust container round-trip
// that dominates the current warm-HIT latency (~585 ms `origin`). A MISS or any
// uncertainty returns `null`, and the caller falls through to the unchanged
// container path (which owns the upstream fetch + fill).
//
// Every transform in here is a faithful, parity-tested port of the container
// truth on origin/main:
//   - url_hash:  brew = lowercasehex(blake3(canonical_bottle_path))  (brew/bottle.rs:47-91)
//                pip  = lowercase(<sha256> path segment)             (pip/wheel.rs, pip.rs:117)
//   - map read:  SELECT content_hash ... WHERE namespace='_public' AND url_hash=?
//                AND NOT EXISTS (public_blocklist)                   (adapter_cache.rs:85-91)
//   - R2 key:    <R2_CAS_REGION>/<derive_prefix(tdk,PUBLIC_UUID)>/<content_hash>
//                                                                    (r2_s3.rs:472/869/877)
//   - derive_prefix: HMAC-SHA256(tdk, uuid16-BE) -> base64url-no-pad -> [..16]
//                                                     (crates/tenant-path/src/prefix.rs:148-166)
//   - re-hash-on-read: blake3(bytes) == content_hash else MISS      (adapter_cache.rs:318-329)
//
// SECURITY INVARIANTS (violating any = a cross-tenant regression):
//   - namespace is the literal constant "_public", NEVER a tenant header; the
//     caller has already run the full PAT verify + tenant-spoof guard.
//   - the revocation `NOT EXISTS public_blocklist` filter is mandatory and in the
//     SAME statement as the map read (no TOCTOU).
//   - re-hash-on-read is mandatory before returning bytes (poison self-heal = MISS).

import { blake3Hex, blake3HexBytes } from "./blake3.js";
import type { KvReader, D1Reader } from "./pat_verify_cache.js";

/** `_public` reserved namespace — matches `adapter_cache.rs:35`. */
export const PUBLIC_NAMESPACE = "_public";

/**
 * The 16 big-endian bytes of `PUBLIC_NAMESPACE_UUID`
 * (`Uuid::from_u128(0x5f5f_7075_626c_6963_0000_0000_0000_0001)`, `r2_s3.rs:869`).
 */
export const PUBLIC_NAMESPACE_UUID_BYTES = new Uint8Array([
  0x5f, 0x5f, 0x70, 0x75, 0x62, 0x6c, 0x69, 0x63,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
]);

const TENANT_PREFIX_LEN = 16; // `crates/tenant-path/src/prefix.rs:15`

/**
 * ASCII-only lowercase, faithful to Rust's `str::to_ascii_lowercase` (only maps
 * `A`-`Z`; leaves every other byte, incl. non-ASCII, untouched). `String.prototype
 * .toLowerCase()` would additionally fold Unicode, diverging from the container.
 */
function asciiLowercase(s: string): string {
  let out = "";
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    out += c >= 0x41 && c <= 0x5a ? String.fromCharCode(c + 0x20) : (s[i] as string);
  }
  return out;
}

/** Decode a hex string to bytes (even length, 0-9a-fA-F). */
function hexToBytes(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) throw new Error("hex length must be even");
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    const byte = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    if (Number.isNaN(byte)) throw new Error("invalid hex");
    out[i] = byte;
  }
  return out;
}

/** base64url encode with no padding — matches Rust `base64 URL_SAFE_NO_PAD`. */
function base64UrlNoPad(bytes: Uint8Array): string {
  let bin = "";
  for (let i = 0; i < bytes.length; i++) bin += String.fromCharCode(bytes[i] as number);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** Minimal shape of the bindings this module needs. */
export interface EdgePublicReadEnv {
  CONFIG_DB: D1Database;
  CAS_BUCKET: R2Bucket;
  R2_TDK_HEX?: string;
  R2_CAS_REGION?: string;
  /**
   * Workers KV (METADATA_KV). Absent ⇒ the L2 map cache is skipped (L1 + D1
   * only). Runtime always has it in prod; declared optional so tests/builds
   * without the binding still typecheck (mirrors how `index.ts` reads it).
   */
  METADATA_KV?: KvReader;
}

/**
 * Port of `canonical_bottle_path` (`brew/bottle.rs:47-91`). PURE over the path.
 * NOTE: the Rust doc-comment claims percent-decode/re-encode, but the CODE does
 * NOT — we match the code. Steps: drop `?query` → strip one leading `/` → strip
 * the `brew/<tenant>/` mount prefix → trim trailing `/` → ascii-lowercase.
 */
export function canonicalBottlePath(rawPath: string): string {
  let p = rawPath.split("?", 1)[0] ?? "";
  if (p.startsWith("/")) p = p.slice(1);
  if (p.startsWith("brew/")) {
    const rest = p.slice("brew/".length);
    const i = rest.indexOf("/");
    p = i >= 0 ? rest.slice(i + 1) : "";
  }
  p = p.replace(/\/+$/, "");
  return asciiLowercase(p);
}

/** brew `url_hash` = lowercasehex(blake3(canonical_bottle_path)). */
export async function brewUrlHash(rawPath: string): Promise<string> {
  return blake3Hex(canonicalBottlePath(rawPath));
}

/**
 * pip `url_hash` = the `<sha256>` path segment, validated 64-hex, lowercased
 * (`pip/wheel.rs:112`, `pip.rs:117`; `Digest::from_hex`/`to_hex`, `digest.rs:60-99`).
 * Path shape: `/pip/<tenant>/pkg/<sha256>/<filename>`. Returns null if no valid
 * 64-hex sha256 segment is present (→ caller falls through to the container).
 */
export function pipUrlHash(rawPath: string): string | null {
  const path = rawPath.split("?", 1)[0] ?? "";
  const segs = path.split("/");
  const i = segs.indexOf("pkg");
  const cand = i >= 0 && i + 1 < segs.length ? segs[i + 1] : undefined;
  if (!cand || !/^[0-9a-fA-F]{64}$/.test(cand)) return null;
  return cand.toLowerCase();
}

/**
 * Port of `derive_prefix(tdk, PUBLIC_NAMESPACE_UUID)` (`prefix.rs:148-166`):
 * HMAC-SHA256(key=tdk 32B, msg=uuid 16B big-endian) → base64url-no-pad → first 16 chars.
 */
export async function derivePublicPrefix(tdkHex: string): Promise<string> {
  const tdk = hexToBytes(tdkHex);
  if (tdk.length !== 32) throw new Error("R2_TDK_HEX must be 32 bytes (64 hex chars)");
  const key = await crypto.subtle.importKey(
    "raw",
    tdk,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = new Uint8Array(
    await crypto.subtle.sign("HMAC", key, PUBLIC_NAMESPACE_UUID_BYTES),
  );
  return base64UrlNoPad(mac).slice(0, TENANT_PREFIX_LEN);
}

/** R2 object key for a `_public` blob: `<region>/<prefix>/<content_hash>` (`r2_s3.rs:472`). */
export function publicR2Key(region: string, prefix: string, contentHash: string): string {
  return `${region}/${prefix}/${contentHash}`;
}

/**
 * Resolve `_public` url_hash → content_hash via `CONFIG_DB`, WITH the mandatory
 * revocation join, in one statement (`adapter_cache.rs:85-91`). Returns null on
 * a miss OR a blocklisted content_hash.
 */
export async function lookupPublicContentHash(
  db: D1Reader,
  urlHash: string,
): Promise<string | null> {
  const row = await db
    .prepare(
      "SELECT c.content_hash AS content_hash FROM adapter_cache_map c " +
        "WHERE c.namespace = ?1 AND c.url_hash = ?2 " +
        "AND NOT EXISTS (SELECT 1 FROM public_blocklist pb WHERE pb.content_hash = c.content_hash) " +
        "LIMIT 1",
    )
    .bind(PUBLIC_NAMESPACE, urlHash)
    .first<{ content_hash: string }>();
  return row?.content_hash ?? null;
}

// ── WP-C: cached `_public` map+blocklist lookup (the revocation gate) ─────────
//
// The map read is the ONLY mutable authorization in the HIT path, so it — not the
// (immutable, content-addressed) blob — carries the short TTL and IS the
// revocation window. A revoked/blocklisted content_hash stops being served within
// ≤ PUBMAP_KV_TTL_S + the L1 slack (a B1b revoke may additionally purge the
// `pubmap:` key to collapse this to the L1 slack — tracked follow-up). Mirrors the
// pat/tsusp/tier/residency three-tier cache (ADR-0070), same uniform 60 s window.
//
// Both a HIT (content_hash) and a NEGATIVE (miss/revoked ⇒ null) are cached; a D1
// FAULT (the read throws) is never cached — it rejects before any write, so a
// transient error can never pin a stale verdict.
const PUBMAP_L1_TTL_MS = 5_000;
const PUBMAP_KV_TTL_S = 60; // KV floor; = the bounded revocation window (ADR)
const PUBMAP_KV_PREFIX = "pubmap:";
const PUBMAP_L1_CAP = 4096;

// ── B1b: active revocation accelerator (collapse the ~60s window to ~seconds) ─
//
// The map cache above serves a POSITIVE verdict (a content_hash) for up to
// PUBMAP_KV_TTL_S without re-consulting D1's `public_blocklist`, so a revoke only
// takes effect at the edge once that verdict expires (~60s). To collapse that,
// a successful `_public` revoke ALSO writes a content_hash-keyed blocklist key
// here (`pubblock:<content_hash>`, written by the Worker revoke seam in
// index.ts). readPublicHit checks it on the resolved content_hash BEFORE serving
// — even on a cached-positive map verdict — so a revoked hash stops serving
// within KV propagation (~seconds). D1's blocklist join remains the permanent,
// authoritative backstop once the ~60s positive verdict expires; this KV entry
// only has to outlive that window, hence a generous but bounded TTL. A KV fault
// FAILS OPEN (serve continues, falling back to the ~60s D1-backed window) — it
// is an accelerator, never the sole gate.
const PUBBLOCK_KV_PREFIX = "pubblock:";
const PUBBLOCK_KV_TTL_S = 3_600; // ≫ the ~65s positive-verdict lifetime it must cover

interface CachedMap {
  readonly h: string | null;
  readonly at: number;
}
const mapCache = new Map<string, CachedMap>();
const mapInflight = new Map<string, Promise<string | null>>();

/** TEST-ONLY: reset the per-isolate map-cache singletons between cases. */
export function __resetPublicMapCacheForTests(): void {
  mapCache.clear();
  mapInflight.clear();
}

/** KV key for a cached `_public` map verdict (namespace is always `_public`). */
export function pubmapKvKey(urlHash: string): string {
  return PUBMAP_KV_PREFIX + urlHash;
}

/** KV key for the content_hash-keyed `_public` revocation marker (B1b). */
export function publicBlocklistKvKey(contentHash: string): string {
  return PUBBLOCK_KV_PREFIX + contentHash;
}

/**
 * Mark a `_public` content_hash as revoked at the edge (B1b). Called by the
 * Worker revoke seam AFTER the container's authoritative revoke succeeds. The
 * value is a non-empty sentinel; only presence matters. Bounded TTL — the D1
 * blocklist is the permanent backstop.
 */
export async function writePublicBlocklistKv(
  kv: KvReader,
  contentHash: string,
): Promise<void> {
  await kv.put(publicBlocklistKvKey(contentHash), "1", {
    expirationTtl: PUBBLOCK_KV_TTL_S,
  });
}

/**
 * Is this content_hash revoked at the edge? A present `pubblock:<hash>` key ⇒
 * revoked. FAILS OPEN (returns false) on any KV fault: the accelerator must
 * never fail a serveable request — the ~60s D1-backed map gate still applies.
 */
export async function isPublicRevokedAtEdge(
  kv: KvReader,
  contentHash: string,
): Promise<boolean> {
  try {
    return (await kv.get(publicBlocklistKvKey(contentHash))) !== null;
  } catch {
    return false;
  }
}

function putMapL1(urlHash: string, h: string | null, nowMs: number): void {
  if (mapCache.size >= PUBMAP_L1_CAP && !mapCache.has(urlHash)) {
    for (const [k, e] of mapCache) {
      if (nowMs - e.at >= PUBMAP_L1_TTL_MS) mapCache.delete(k);
    }
    if (mapCache.size >= PUBMAP_L1_CAP) {
      const oldest = mapCache.keys().next().value;
      if (oldest !== undefined) mapCache.delete(oldest);
    }
  }
  mapCache.set(urlHash, { h, at: nowMs });
}

/**
 * Read a cached map verdict from KV. Returns `{ h }` (h = content_hash or null) on
 * a clean hit, or `null` on miss / malformed / KV fault (⇒ fall through to D1).
 * Never throws.
 */
async function kvGetMap(
  kv: KvReader,
  urlHash: string,
): Promise<{ h: string | null } | null> {
  let raw: string | null;
  try {
    raw = await kv.get(pubmapKvKey(urlHash));
  } catch {
    return null;
  }
  if (raw === null) return null;
  try {
    const o = JSON.parse(raw) as Record<string, unknown>;
    const h = o["h"];
    if (h === null) return { h: null };
    if (typeof h === "string" && /^[0-9a-f]{64}$/.test(h)) return { h };
  } catch {
    /* malformed → miss */
  }
  return null;
}

/** Options for {@link lookupPublicContentHashCached}. All optional (tests pass none). */
export interface MapCacheOpts {
  /** L2 Workers KV (METADATA_KV). Absent ⇒ L2 skipped. */
  readonly kv?: KvReader;
  /** `ctx.waitUntil` so the KV write-behind survives response return. */
  readonly waitUntil?: (p: Promise<unknown>) => void;
  /** Wall-clock ms (injectable for tests). */
  readonly nowMs?: number;
  /** L3 fetch (injectable for tests); defaults to {@link lookupPublicContentHash}. */
  readonly fetch?: (db: D1Reader, urlHash: string) => Promise<string | null>;
}

/**
 * L1 → L2 KV → L3 D1 cached form of {@link lookupPublicContentHash}. Caches both
 * the positive (content_hash) and negative (null) verdict with a bounded TTL (the
 * revocation window); a D1 fault rejects and is never cached. Single-flighted per
 * url_hash.
 */
export async function lookupPublicContentHashCached(
  db: D1Reader,
  urlHash: string,
  opts: MapCacheOpts = {},
): Promise<string | null> {
  const nowMs = opts.nowMs ?? Date.now();
  const fetchMap = opts.fetch ?? lookupPublicContentHash;

  const l1 = mapCache.get(urlHash);
  if (l1 !== undefined && nowMs - l1.at < PUBMAP_L1_TTL_MS) return l1.h;

  const existing = mapInflight.get(urlHash);
  if (existing !== undefined) return existing;

  const chain = (async (): Promise<string | null> => {
    if (opts.kv !== undefined) {
      const kvv = await kvGetMap(opts.kv, urlHash);
      if (kvv !== null) {
        putMapL1(urlHash, kvv.h, nowMs);
        return kvv.h;
      }
    }
    // L3 D1 (throws on fault ⇒ chain rejects ⇒ nothing cached).
    const h = await fetchMap(db, urlHash);
    putMapL1(urlHash, h, nowMs);
    if (opts.kv !== undefined) {
      const kv = opts.kv;
      const write = kv
        .put(pubmapKvKey(urlHash), JSON.stringify({ h }), {
          expirationTtl: PUBMAP_KV_TTL_S,
        })
        .catch(() => {
          /* best-effort: a KV write fault must not fail the request */
        });
      if (opts.waitUntil !== undefined) opts.waitUntil(write);
      else await write;
    }
    return h;
  })();

  mapInflight.set(urlHash, chain);
  try {
    return await chain;
  } finally {
    mapInflight.delete(urlHash);
  }
}

// ── WP-A: colo Cache API L1 for the `_public` blob (R2 = origin-of-record) ────
//
// The blob is content-addressed and IMMUTABLE (the bytes for a given content_hash
// never change), so it is cached per-colo with a LONG TTL under a key that is the
// content identity itself — NEVER the PAT — so tenants transparently share one
// colo entry (the whole point of `_public`). Only our own fill (which re-hash
// validates) ever writes it, so a colo-hit serves fill-validated bytes WITHOUT
// re-hashing (owner-approved: the key IS the hash and the colo cache is as trusted
// as the runtime). Revocation is enforced UPSTREAM at the map gate (short TTL), so
// a long blob TTL is safe: a revoked hash simply stops being reachable (map miss)
// while its now-unreferenced bytes age out of the colo cache.
const PUBLIC_BLOB_CACHE_TTL_S = 604_800; // 7 d — immutable content-addressed blob

/**
 * Synthetic per-colo Cache API key for a `_public` blob. The host is never
 * resolved — the Cache API keys purely on the URL string — so this is a pure
 * content-addressed cache handle shared across every tenant in the colo.
 */
export function publicBlobCacheKey(contentHash: string): Request {
  return new Request(`https://public-cas.cache.local/${contentHash}`);
}

/**
 * The Cloudflare per-colo default cache, or `null` where the Cache API is absent
 * (node unit tests, or any runtime without `caches`). A `null` degrades the read
 * to the R2-direct path — the colo cache is a pure optimization, never load-bearing.
 */
function coloCache(): Cache | null {
  const c = (globalThis as unknown as { caches?: { default?: Cache } }).caches;
  return c?.default ?? null;
}

/**
 * Content-Type served for an edge `_public` blob. These are opaque
 * content-addressed CAS artifacts (Homebrew bottles, pip wheels) — clients fetch
 * by bytes and verify by hash, so `application/octet-stream` is the faithful,
 * safe type (also what the colo-cache fill stamps and what the container's binary
 * blob read returns). Kept as a single constant so the serve path and the fill
 * agree.
 */
export const PUBLIC_BLOB_CONTENT_TYPE = "application/octet-stream";

/**
 * Parse a single HTTP `Range: bytes=…` header against a known total length.
 * Returns the inclusive `{start,end}` for a satisfiable single range,
 * `"unsatisfiable"` for a syntactically-valid but out-of-bounds range (caller →
 * 416), or `null` when there is no Range header or it is a form we don't serve
 * from the edge (multi-range / non-`bytes` unit → caller falls back to a full 200).
 * Supports `bytes=a-b`, `bytes=a-` (open-ended), and `bytes=-N` (suffix).
 */
export function parseByteRange(
  rangeHeader: string | null,
  totalLen: number,
): { start: number; end: number } | "unsatisfiable" | null {
  if (!rangeHeader) return null;
  const m = /^bytes=(\d*)-(\d*)$/.exec(rangeHeader.trim());
  if (!m) return null; // multi-range / malformed / non-bytes unit → serve full 200
  const startRaw = m[1] ?? "";
  const endRaw = m[2] ?? "";
  if (startRaw === "" && endRaw === "") return null;
  let start: number;
  let end: number;
  if (startRaw === "") {
    // suffix form: the last `endRaw` bytes
    const suffix = Number(endRaw);
    if (suffix === 0) return "unsatisfiable";
    start = Math.max(0, totalLen - suffix);
    end = totalLen - 1;
  } else {
    start = Number(startRaw);
    end = endRaw === "" ? totalLen - 1 : Number(endRaw);
    if (end >= totalLen) end = totalLen - 1;
  }
  if (start > end || start >= totalLen) return "unsatisfiable";
  return { start, end };
}

export type EdgeRouteKind = "brew" | "pip";

/**
 * The full edge `_public` HIT read. Returns the bytes on a verified HIT, or null
 * for ANY miss/uncertainty (unknown route, no url_hash, map miss, revoked, R2
 * absent, or re-hash mismatch) — the caller MUST fall through to the container.
 *
 * MUST be called only on the tenant's home-region worker leg (after residency
 * fan-out), so `env.R2_CAS_REGION` / `env.CAS_BUCKET` are correct for this tenant.
 */
export async function readPublicHit(
  env: EdgePublicReadEnv,
  routeKind: EdgeRouteKind,
  rawPath: string,
  ctx?: { waitUntil(p: Promise<unknown>): void },
): Promise<{ bytes: ArrayBuffer; contentHash: string } | null> {
  if (!env.R2_TDK_HEX || !env.R2_CAS_REGION) return null;

  const urlHash =
    routeKind === "brew" ? await brewUrlHash(rawPath) : pipUrlHash(rawPath);
  if (!urlHash) return null;

  // WP-C: map+blocklist gate (cached, bounded TTL = the revocation window).
  // P1: read it through the nearest D1 read replica (`withSession`), mirroring the
  // PAT verify path (ADR-0070, index.ts) — the map read is the last synchronous
  // D1-PRIMARY round trip on the far-region (e.g. SAM) HIT hot path once L1+L2
  // miss. Feature-detected: a test double / a runtime without `withSession`
  // degrades to the primary. Correctness is unchanged — the `pubblock:` KV marker
  // (checked below) and the D1 `public_blocklist` join remain the authoritative
  // revocation backstops; the replica only widens the POSITIVE-map staleness by at
  // most replica lag, already inside the ~60s map-cache TTL window (ADR-0070).
  const mapReadDb: D1Reader =
    typeof env.CONFIG_DB.withSession === "function"
      ? env.CONFIG_DB.withSession("first-unconstrained")
      : env.CONFIG_DB;
  const contentHash = await lookupPublicContentHashCached(mapReadDb, urlHash, {
    ...(env.METADATA_KV ? { kv: env.METADATA_KV } : {}),
    ...(ctx ? { waitUntil: ctx.waitUntil.bind(ctx) } : {}),
  });
  if (!contentHash) return null; // map miss OR revoked (D1 blocklist join)

  // B1b: revocation accelerator. The map verdict above may be a cached POSITIVE
  // (skips the D1 blocklist join for up to ~60s); honor a fresh revoke NOW by
  // checking the content_hash-keyed KV marker before serving. Fails open on KV
  // fault (the ~60s D1-backed window remains the backstop). Also gates the colo
  // Cache API read below, so a revoked hash is never served from any tier.
  if (env.METADATA_KV && (await isPublicRevokedAtEdge(env.METADATA_KV, contentHash))) {
    return null;
  }

  // WP-A: colo Cache API L1. Serve fill-validated bytes on a colo-hit without
  // re-hashing (key IS the content_hash; only our validated fill writes it).
  const cache = coloCache();
  const cacheKey = publicBlobCacheKey(contentHash);
  if (cache) {
    const cached = await cache.match(cacheKey);
    if (cached) {
      return { bytes: await cached.arrayBuffer(), contentHash };
    }
  }

  // Colo MISS = FILL: R2 (origin-of-record) → re-hash validate → populate colo.
  const prefix = await derivePublicPrefix(env.R2_TDK_HEX);
  const key = publicR2Key(env.R2_CAS_REGION, prefix, contentHash);

  const obj = await env.CAS_BUCKET.get(key);
  if (!obj) return null; // bytes absent (e.g. revoked between map read and here)

  const bytes = await obj.arrayBuffer();

  // Re-hash-on-FILL: never cache/serve bytes that do not hash to the mapped
  // content_hash (poison self-heal = MISS).
  if ((await blake3HexBytes(new Uint8Array(bytes))) !== contentHash) return null;

  // Populate the colo cache with the immutable blob (a fresh copy so the returned
  // ArrayBuffer is never detached by the Response body). Skipped where the Cache
  // API is absent (the read already succeeded via R2).
  if (cache) {
    const fill = cache
      .put(
        cacheKey,
        new Response(bytes.slice(0), {
          headers: {
            "Cache-Control": `max-age=${PUBLIC_BLOB_CACHE_TTL_S}`,
            "Content-Type": "application/octet-stream",
          },
        }),
      )
      .catch(() => {
        /* best-effort: a colo-cache write fault must not fail the request */
      });
    if (ctx) ctx.waitUntil(fill);
    else await fill;
  }

  return { bytes, contentHash };
}

/** Structured verdict of an F1 shadow comparison (edge vs container). */
export type ShadowVerdict =
  | "parity_ok" // container HIT, edge HIT, identical bytes — the case we must prove
  | "parity_miss" // container HIT but edge MISSED — investigate (url_hash/region/blocklist drift)
  | "parity_bytes_diff" // both HIT but bytes differ — SERIOUS (hash-port bug)
  | "container_miss_edge_hit" // container filled/served MISS but edge already had it (concurrent fill)
  | "container_miss_edge_miss"; // both MISS — expected (blob not in `_public` yet)

/**
 * F1 SHADOW comparison. Serves nothing; called from `ctx.waitUntil` AFTER the
 * container response is chosen, to prove edge/container parity on real traffic
 * with zero user impact. The container response is authoritative; this only
 * reads a CLONE. `containerResp` MUST be a clone (its body is consumed here).
 *
 * Parity contract: a container `x-cache: HIT` MUST correspond to an edge HIT
 * with byte-identical content. A container MISS (upstream fill) legitimately has
 * no edge copy yet, so an edge MISS there is EXPECTED, not a defect.
 */
export async function shadowCompareEdgePublicRead(
  env: EdgePublicReadEnv,
  routeKind: EdgeRouteKind,
  rawPath: string,
  containerResp: Response,
): Promise<ShadowVerdict> {
  const containerHit = containerResp.headers.get("x-cache")?.toUpperCase() === "HIT";
  const edge = await readPublicHit(env, routeKind, rawPath);

  if (!containerHit) {
    return edge ? "container_miss_edge_hit" : "container_miss_edge_miss";
  }
  if (!edge) return "parity_miss";

  const containerBytes = new Uint8Array(await containerResp.arrayBuffer());
  const edgeBytes = new Uint8Array(edge.bytes);
  if (containerBytes.length !== edgeBytes.length) return "parity_bytes_diff";
  for (let i = 0; i < containerBytes.length; i++) {
    if (containerBytes[i] !== edgeBytes[i]) return "parity_bytes_diff";
  }
  return "parity_ok";
}
