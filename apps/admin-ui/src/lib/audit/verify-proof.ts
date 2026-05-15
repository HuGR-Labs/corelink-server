// Audit-chain client-side proof verifier (wt/r-prep-audit-chain-viz).
//
// CoreLink's customer-facing audit chain uses BLAKE3 leaves + nodes (per
// `corelink-audit-chain` `chain.rs` — RFC 8785 JCS canonicalization + BLAKE3
// link hash). The admin UI already ships `apps/admin-ui/src/lib/merkle.ts` for
// the SHA-256 audit Merkle tree (S-09 spec §5.3 chose SHA-256 for browser-
// native verification on the admin side). This module is the BLAKE3 sibling
// for the customer-facing visualization page.
//
// Implementation strategy:
//   - Tries to load `@corelink/client` (the WASM bundle produced by
//     `crates/corelink-wasm` wrapping `corelink-client-verify` — single Rust
//     truth per ADR-0016) via dynamic `import()`. The package is published
//     to npm separately; in this repo it's an optional peer dep.
//   - Falls back to SHA-256 via SubtleCrypto when `algorithm === "sha256"`
//     (delegates to the existing `merkle.ts` verifier).
//   - Returns a typed result discriminating between "valid" / "mismatch" /
//     "wasm_unavailable" / "algorithm_unsupported" so the UI can render
//     specific copy.
//
// Test mode: when the WASM bundle is unavailable, single-leaf proofs (zero
// siblings) still verify because the computed root equals the leaf hash
// regardless of which hash function we'd use. This lets Playwright assert
// the "valid" path without spinning up the wasm-pack output in CI.

import type { MerkleProof } from "@/lib/types";
import { verifyMerkleProof, type ProofVerificationResult } from "@/lib/merkle";

export type AuditProofResult =
  | { ok: true; computed_root: string; verifier: "wasm_blake3" | "subtle_sha256" | "trivial" }
  | {
      ok: false;
      reason: "mismatch" | "wasm_unavailable" | "algorithm_unsupported" | "invalid_hex";
      computed_root?: string;
      verifier?: "wasm_blake3" | "subtle_sha256" | "trivial";
    };

/**
 * Shape of the BLAKE3 verifier we expect the `@corelink/client` WASM package
 * to expose. Defined here as an interface so we don't take a hard dep at
 * typecheck time on a package that may not be installed (it's published
 * separately by the `corelink-wasm` crate build pipeline).
 *
 * The Rust side is `corelink_wasm::ClientVerifier::verify_merkle_proof` —
 * input is hex-encoded leaf + siblings; output is the computed root hex.
 */
export interface WasmBlake3MerkleVerifier {
  /** Compute the Merkle root by walking siblings; returns hex-encoded root. */
  computeRoot(args: {
    leaf_hash_hex: string;
    siblings: Array<{ hash_hex: string; position: "left" | "right" }>;
  }): Promise<string> | string;
}

/**
 * Lazy WASM verifier loader. Returns null when the WASM bundle is not
 * available (e.g. dev server without `pnpm wasm:build`, or CSP block).
 *
 * The dynamic import is wrapped in a try/catch so a missing package never
 * crashes the page — the UI surfaces "Browser verifier unavailable" copy
 * and points the user at the `corelink audit verify` CLI.
 */
let cachedVerifier: WasmBlake3MerkleVerifier | null | undefined;

export async function loadWasmVerifier(): Promise<WasmBlake3MerkleVerifier | null> {
  if (cachedVerifier !== undefined) return cachedVerifier;
  try {
    // The package name is the published `@corelink/client` from `corelink-wasm`.
    // We use Function-constructed import to avoid bundler resolution failure
    // when the package isn't installed in the workspace (production builds
    // install it via `pnpm add @corelink/client`).
    const dynamicImport = new Function("p", "return import(p)") as (
      p: string,
    ) => Promise<unknown>;
    const mod = (await dynamicImport("@corelink/client")) as {
      MerkleVerifier?: new () => WasmBlake3MerkleVerifier;
    };
    if (typeof mod.MerkleVerifier !== "function") {
      cachedVerifier = null;
      return null;
    }
    cachedVerifier = new mod.MerkleVerifier();
    return cachedVerifier;
  } catch {
    cachedVerifier = null;
    return null;
  }
}

/** Test-only: override the cached verifier. */
export function __setWasmVerifierForTest(v: WasmBlake3MerkleVerifier | null): void {
  cachedVerifier = v;
}

function normalizeHex(hex: string): string {
  return hex.toLowerCase().replace(/^0x/, "");
}

function isValidHex(hex: string, byteLen?: number): boolean {
  const clean = normalizeHex(hex);
  if (byteLen !== undefined && clean.length !== byteLen * 2) return false;
  return /^[0-9a-f]*$/.test(clean) && clean.length % 2 === 0;
}

/**
 * Verify an audit-chain inclusion proof.
 *
 * For BLAKE3 proofs we attempt the WASM verifier; if unavailable, single-leaf
 * proofs (siblings.length === 0) still verify trivially because the computed
 * root is the leaf hash by definition (Merkle tree of a single element).
 *
 * For SHA-256 proofs we delegate to the existing `merkle.ts` verifier.
 */
export async function verifyAuditProof(proof: MerkleProof): Promise<AuditProofResult> {
  if (!isValidHex(proof.leaf_hash, 32) || !isValidHex(proof.expected_root, 32)) {
    return { ok: false, reason: "invalid_hex" };
  }
  for (const sib of proof.siblings) {
    if (!isValidHex(sib.hash, 32)) return { ok: false, reason: "invalid_hex" };
  }

  // SHA-256 path: delegate to the canonical admin-side verifier (subtle).
  if (proof.algorithm === "sha256") {
    const result: ProofVerificationResult = await verifyMerkleProof(proof);
    if (result.ok) {
      return { ok: true, computed_root: result.computed_root, verifier: "subtle_sha256" };
    }
    if (result.reason === "mismatch") {
      return {
        ok: false,
        reason: "mismatch",
        computed_root: result.computed_root,
        verifier: "subtle_sha256",
      };
    }
    return { ok: false, reason: "algorithm_unsupported" };
  }

  // BLAKE3 path.
  if (proof.algorithm !== "blake3") {
    return { ok: false, reason: "algorithm_unsupported" };
  }

  // Trivial single-leaf proof: a Merkle tree of one element has root == leaf.
  // No WASM required.
  if (proof.siblings.length === 0) {
    const leaf = normalizeHex(proof.leaf_hash);
    const expected = normalizeHex(proof.expected_root);
    if (leaf === expected) {
      return { ok: true, computed_root: leaf, verifier: "trivial" };
    }
    return { ok: false, reason: "mismatch", computed_root: leaf, verifier: "trivial" };
  }

  const wasm = await loadWasmVerifier();
  if (!wasm) {
    return { ok: false, reason: "wasm_unavailable" };
  }

  let computed: string;
  try {
    computed = normalizeHex(
      await Promise.resolve(
        wasm.computeRoot({
          leaf_hash_hex: normalizeHex(proof.leaf_hash),
          siblings: proof.siblings.map((s) => ({
            hash_hex: normalizeHex(s.hash),
            position: s.position,
          })),
        }),
      ),
    );
  } catch {
    return { ok: false, reason: "wasm_unavailable" };
  }

  const expected = normalizeHex(proof.expected_root);
  if (computed === expected) {
    return { ok: true, computed_root: computed, verifier: "wasm_blake3" };
  }
  return { ok: false, reason: "mismatch", computed_root: computed, verifier: "wasm_blake3" };
}

export const __testing = { normalizeHex, isValidHex };
