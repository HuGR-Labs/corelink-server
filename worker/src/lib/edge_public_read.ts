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
  db: D1Database,
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
): Promise<{ bytes: ArrayBuffer; contentHash: string } | null> {
  if (!env.R2_TDK_HEX || !env.R2_CAS_REGION) return null;

  const urlHash =
    routeKind === "brew" ? await brewUrlHash(rawPath) : pipUrlHash(rawPath);
  if (!urlHash) return null;

  const contentHash = await lookupPublicContentHash(env.CONFIG_DB, urlHash);
  if (!contentHash) return null; // map miss OR revoked

  const prefix = await derivePublicPrefix(env.R2_TDK_HEX);
  const key = publicR2Key(env.R2_CAS_REGION, prefix, contentHash);

  const obj = await env.CAS_BUCKET.get(key);
  if (!obj) return null; // bytes absent (e.g. revoked between map read and here)

  const bytes = await obj.arrayBuffer();

  // Re-hash-on-read: never serve bytes that do not hash to the mapped content_hash.
  if ((await blake3HexBytes(new Uint8Array(bytes))) !== contentHash) return null;

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
