// WI-S16-005 — Merkle proof verification tests (SHA-256 path).

import { describe, it, expect } from "vitest";
import { verifyMerkleProof } from "@/lib/merkle";
import type { MerkleProof } from "@/lib/types";

async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const buf = await crypto.subtle.digest("SHA-256", bytes as unknown as BufferSource);
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function fromHex(hex: string): Uint8Array {
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    out[i / 2] = Number.parseInt(hex.slice(i, i + 2), 16);
  }
  return out;
}

function concat(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length);
  out.set(a, 0);
  out.set(b, a.length);
  return out;
}

async function buildProof(): Promise<{ proof: MerkleProof; expected_root: string }> {
  // 4-leaf tree: L0, L1, L2, L3. We prove inclusion of L0.
  const leaves = ["leaf0", "leaf1", "leaf2", "leaf3"].map((s) =>
    new TextEncoder().encode(s),
  );
  const leafHashes = await Promise.all(leaves.map(sha256Hex));

  // Level 1: H(L0||L1), H(L2||L3).
  const l01 = await sha256Hex(concat(fromHex(leafHashes[0]!), fromHex(leafHashes[1]!)));
  const l23 = await sha256Hex(concat(fromHex(leafHashes[2]!), fromHex(leafHashes[3]!)));
  // Root: H(l01||l23).
  const root = await sha256Hex(concat(fromHex(l01), fromHex(l23)));

  // Inclusion proof for L0:
  //   sibling 1: L1 (right of current)
  //   sibling 2: l23 (right of current)
  const proof: MerkleProof = {
    leaf_hash: leafHashes[0]!,
    siblings: [
      { hash: leafHashes[1]!, position: "right" },
      { hash: l23, position: "right" },
    ],
    expected_root: root,
    algorithm: "sha256",
  };
  return { proof, expected_root: root };
}

describe("verifyMerkleProof", () => {
  it("returns ok=true for a valid proof", async () => {
    const { proof } = await buildProof();
    const result = await verifyMerkleProof(proof);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.computed_root).toBe(proof.expected_root);
    }
  });

  it("returns ok=false with reason='mismatch' for tampered leaf", async () => {
    const { proof } = await buildProof();
    const tampered: MerkleProof = {
      ...proof,
      leaf_hash:
        "0000000000000000000000000000000000000000000000000000000000000000",
    };
    const result = await verifyMerkleProof(tampered);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toBe("mismatch");
      expect(result.computed_root).not.toBe(proof.expected_root);
    }
  });

  it("returns ok=false with reason='mismatch' for tampered sibling", async () => {
    const { proof } = await buildProof();
    const tampered: MerkleProof = {
      ...proof,
      siblings: [
        {
          hash: "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
          position: "right",
        },
        ...proof.siblings.slice(1),
      ],
    };
    const result = await verifyMerkleProof(tampered);
    expect(result.ok).toBe(false);
  });

  it("returns 'algorithm_unsupported' for blake3 proofs (WASM path not built)", async () => {
    const result = await verifyMerkleProof({
      leaf_hash: "00",
      siblings: [],
      expected_root: "00",
      algorithm: "blake3",
    });
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toBe("algorithm_unsupported");
    }
  });
});
