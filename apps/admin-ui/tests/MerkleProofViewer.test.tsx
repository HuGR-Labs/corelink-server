// WI-S16-005 — MerkleProofViewer renders "valid" or "MISMATCH" status.

import React from "react";
import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import MerkleProofViewer from "@/components/admin/MerkleProofViewer";
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

async function makeValidProof(): Promise<MerkleProof> {
  const leaf = new TextEncoder().encode("leaf-payload");
  const sibling = new TextEncoder().encode("sibling-payload");
  const leafHash = await sha256Hex(leaf);
  const siblingHash = await sha256Hex(sibling);
  const root = await sha256Hex(concat(fromHex(leafHash), fromHex(siblingHash)));
  return {
    leaf_hash: leafHash,
    siblings: [{ hash: siblingHash, position: "right" }],
    expected_root: root,
    algorithm: "sha256",
  };
}

describe("MerkleProofViewer", () => {
  it("surfaces 'Proof valid' for a correct proof", async () => {
    const proof = await makeValidProof();
    render(<MerkleProofViewer proof={proof} />);
    await waitFor(() => {
      const status = screen.getByTestId("merkle-status");
      expect(status.textContent).toMatch(/Proof valid/);
    });
  });

  it("surfaces 'Proof MISMATCH' when expected_root is tampered", async () => {
    const proof = await makeValidProof();
    const tampered: MerkleProof = {
      ...proof,
      expected_root:
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
    };
    render(<MerkleProofViewer proof={tampered} />);
    await waitFor(() => {
      const status = screen.getByTestId("merkle-status");
      expect(status.textContent).toMatch(/Proof MISMATCH/);
    });
  });
});
