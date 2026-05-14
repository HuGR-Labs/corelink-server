// WI-S16-005 — client-side Merkle inclusion proof verification.
//
// Hash algorithm choice:
//   The CoreLink backend uses BLAKE3 throughout (content addressing). HOWEVER,
//   the audit-log Merkle tree (S-09 spec §5.3) is published with **SHA-256**
//   leaves+nodes because the spec wants browser-native verification with zero
//   WASM dependency for compliance auditors. The proof envelope carries an
//   `algorithm` discriminator so we can add BLAKE3 (via `@noble/hashes` WASM)
//   without breaking the API.
//
// Currently we implement SHA-256 only; for BLAKE3 we surface
// "algorithm_unsupported" rather than silently returning "valid".

import type { MerkleProof } from "./types";

export type ProofVerificationResult =
  | { ok: true; computed_root: string }
  | { ok: false; reason: "mismatch" | "algorithm_unsupported"; computed_root?: string };

const subtle = (): SubtleCrypto | null => {
  if (typeof globalThis === "undefined") return null;
  const c = (globalThis as { crypto?: Crypto }).crypto;
  return c?.subtle ?? null;
};

function toHex(buf: ArrayBuffer): string {
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function fromHex(hex: string): Uint8Array {
  const clean = hex.toLowerCase().replace(/^0x/, "");
  if (clean.length % 2 !== 0) throw new Error("invalid hex length");
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < clean.length; i += 2) {
    const byte = Number.parseInt(clean.slice(i, i + 2), 16);
    if (Number.isNaN(byte)) throw new Error(`invalid hex byte at ${i}`);
    out[i / 2] = byte;
  }
  return out;
}

async function sha256(bytes: Uint8Array): Promise<Uint8Array> {
  const s = subtle();
  if (!s) throw new Error("SubtleCrypto unavailable");
  // Cast: TS lib.dom narrows BufferSource to Uint8Array<ArrayBuffer>, but our
  // Uint8Array allocations are over ArrayBufferLike (which includes
  // SharedArrayBuffer at the type level). Backing storage is always a normal
  // ArrayBuffer at runtime for our `new Uint8Array(n)` calls.
  const buf = await s.digest("SHA-256", bytes as unknown as BufferSource);
  return new Uint8Array(buf);
}

/**
 * Verify a Merkle inclusion proof.
 *
 * Walk: start from `leaf_hash`. For each sibling, concatenate based on sibling
 * position (`left` → sibling || current, `right` → current || sibling) and
 * hash. Final value must equal `expected_root`.
 */
export async function verifyMerkleProof(proof: MerkleProof): Promise<ProofVerificationResult> {
  if (proof.algorithm !== "sha256") {
    return { ok: false, reason: "algorithm_unsupported" };
  }

  let current = fromHex(proof.leaf_hash);
  for (const sibling of proof.siblings) {
    const sib = fromHex(sibling.hash);
    const concat = new Uint8Array(current.length + sib.length);
    if (sibling.position === "left") {
      concat.set(sib, 0);
      concat.set(current, sib.length);
    } else {
      concat.set(current, 0);
      concat.set(sib, current.length);
    }
    current = await sha256(concat);
  }

  // toHex reads from the Uint8Array view (not the underlying buffer); pass the
  // view via a fresh ArrayBuffer copy to satisfy strict `ArrayBuffer` typing.
  const copy = new Uint8Array(current.length);
  copy.set(current);
  const computedRoot = toHex(copy.buffer);
  const expected = proof.expected_root.toLowerCase().replace(/^0x/, "");
  if (computedRoot === expected) {
    return { ok: true, computed_root: computedRoot };
  }
  return { ok: false, reason: "mismatch", computed_root: computedRoot };
}

export const __testing = { toHex, fromHex, sha256 };
