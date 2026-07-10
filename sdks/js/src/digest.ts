import { blake3 } from "@noble/hashes/blake3";
import { bytesToHex } from "@noble/hashes/utils";

/**
 * A 64-character lowercase-hex BLAKE3-256 content digest.
 *
 * This is the canonical content-addressing key CoreLink stores blobs under.
 * The server independently re-hashes every uploaded body and rejects a `PUT`
 * whose claimed hash does not match (`crates/corelink-handler-cas`), so the
 * digest computed here is the single source of truth for the object's address.
 */
export type BlobDigest = string;

/** Regex for a canonical CoreLink digest: exactly 64 lowercase hex chars. */
export const DIGEST_RE = /^[0-9a-f]{64}$/;

/**
 * Compute the canonical BLAKE3-256 digest (64 lowercase hex chars) of `data`.
 *
 * Mirrors the server's content-addressing exactly: the container verifies
 * `blake3(body) == claimed_hash` on every write, so the value returned here is
 * the address the blob will live at.
 */
export function blake3Hex(data: Uint8Array): BlobDigest {
  return bytesToHex(blake3(data));
}

/**
 * Return `true` iff `d` is a syntactically canonical CoreLink digest
 * (64 lowercase hex chars). The server applies the identical check
 * (`is_canonical_digest`) and rejects anything else with `400` before it ever
 * touches storage, so validating client-side turns a wasted round-trip into a
 * synchronous throw.
 */
export function isCanonicalDigest(d: string): boolean {
  return DIGEST_RE.test(d);
}
