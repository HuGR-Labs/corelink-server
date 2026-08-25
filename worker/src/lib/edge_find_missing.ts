/**
 * Edge-native `findMissingBlobs` — pure derivation half (F0).
 *
 * WHY THIS EXISTS. Measured from inside the fabric, the container answers
 * `findMissingBlobs` at ~13 digests/second: n=100 costs 8.6-9.1 s, the curve is
 * linear, and sixteen SEPARATE single-digest requests (1.19 s wall) are no
 * faster than one sixteen-digest request (1.59-1.82 s) — so the ceiling is the
 * 0.25-vCPU instance and its public-S3 path to R2, not our fan-out. Reaching
 * the owner's 15 ms ceiling means probing R2 in-colo through the Worker's
 * native binding instead. See
 * `docs/design/2026-08-25-adr-edge-native-find-missing.md`.
 *
 * This module is DERIVATION ONLY: given a tenant and a digest, what R2 key does
 * the container use? Nothing here touches R2, D1, or auth. Serving is F2.
 *
 * The key format and the prefix derivation are pinned by
 * `worker/tests/vectors/tenant_prefix_vectors.json`, which the Rust source of
 * truth asserts too (`crates/tenant-path/tests/edge_parity_vectors.rs`). A
 * one-sided test would let the edge derive keys the container never wrote —
 * every blob would read as missing, which a cache client acts on by uploading
 * everything.
 */

/** `TENANT_PREFIX_LEN` (`crates/tenant-path/src/prefix.rs`). */
export const TENANT_PREFIX_LEN = 16;

/** Digest algorithms the CAS keyspace distinguishes (`DigestAlgo`). */
export type CasDigestAlgo = "blake3" | "sha256";

/**
 * Tenant ids that must never reach key derivation. These are sentinels the
 * Worker uses for "no tenant resolved yet"; deriving a prefix for one would
 * create a keyspace SHARED by every unresolved caller. Mirrors the container's
 * `TENANT_SENTINELS` (`routes/cas.rs`, `routes/ac.rs`, …).
 */
const TENANT_SENTINELS = new Set(["_anonymous", "_unknown", "_system", "_pending", "_public", ""]);

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** REAPI v2 digests are hex sha256. */
const SHA256_HEX_RE = /^[0-9a-f]{64}$/;

function hexToBytes(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) {
    out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function base64UrlNoPad(bytes: Uint8Array): string {
  let bin = "";
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/**
 * Canonical UUID → the 16 raw bytes, big-endian — `Uuid::as_bytes()`, which is
 * what `derive_prefix` HMACs. Returns null for anything that is not a canonical
 * lowercase UUID, because the container's `tenant_prefix` FAILS CLOSED on a
 * non-UUID tenant rather than inventing a prefix, and the edge must refuse
 * exactly where the container refuses.
 */
export function uuidToBytes(tenantId: string): Uint8Array | null {
  if (!UUID_RE.test(tenantId)) return null;
  return hexToBytes(tenantId.replace(/-/g, ""));
}

/**
 * Port of `derive_prefix(tdk, tenant_uuid)` (`crates/tenant-path/src/prefix.rs`):
 * HMAC-SHA256(key = 32-byte TDK, msg = 16-byte UUID) → base64url-no-pad → first
 * 16 chars. Generalises `edge_public_read.ts::derivePublicPrefix`, which is the
 * same primitive fixed to the `_public` namespace UUID.
 *
 * Returns null — never a guess — when the tenant is a sentinel or not a
 * canonical UUID. A caller that gets null must fall back to the container.
 */
export async function deriveTenantPrefix(
  tdkHex: string,
  tenantId: string,
): Promise<string | null> {
  if (TENANT_SENTINELS.has(tenantId)) return null;
  const uuidBytes = uuidToBytes(tenantId);
  if (!uuidBytes) return null;

  const tdk = hexToBytes(tdkHex);
  if (tdk.length !== 32) throw new Error("R2_TDK_HEX must be 32 bytes (64 hex chars)");

  const key = await crypto.subtle.importKey(
    "raw",
    tdk,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const mac = new Uint8Array(await crypto.subtle.sign("HMAC", key, uuidBytes));
  return base64UrlNoPad(mac).slice(0, TENANT_PREFIX_LEN);
}

/**
 * Port of `R2S3Client::blob_key`. REAPI digests are sha256 and live under a
 * SEPARATE `bazel/sha256/` keyspace from native BLAKE3 blobs — probing the
 * native form for a REAPI digest would answer "missing" for blobs that exist.
 */
export function casBlobKey(
  region: string,
  tenantPrefix: string,
  digest: string,
  algo: CasDigestAlgo,
): string {
  return algo === "sha256"
    ? `${region}/${tenantPrefix}/bazel/sha256/${digest}`
    : `${region}/${tenantPrefix}/${digest}`;
}

/**
 * The R2 key a `findMissingBlobs` digest is probed at, or null when the edge
 * must not answer: sentinel/non-UUID tenant, or a digest that is not canonical
 * lowercase hex sha256. Malformed digests are the container's to reject, with
 * its own error taxonomy — the edge declining is not the edge accepting.
 */
export async function findMissingProbeKey(
  tdkHex: string,
  region: string,
  tenantId: string,
  digestHex: string,
): Promise<string | null> {
  if (!SHA256_HEX_RE.test(digestHex)) return null;
  const prefix = await deriveTenantPrefix(tdkHex, tenantId);
  if (!prefix) return null;
  return casBlobKey(region, prefix, digestHex, "sha256");
}
