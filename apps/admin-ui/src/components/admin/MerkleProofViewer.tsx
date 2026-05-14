// WI-S16-005 — visualise + verify the Merkle inclusion proof client-side.

"use client";

import React from "react";
import type { MerkleProof } from "@/lib/types";
import { verifyMerkleProof, type ProofVerificationResult } from "@/lib/merkle";

export interface MerkleProofViewerProps {
  proof: MerkleProof;
}

type Status =
  | { kind: "pending" }
  | { kind: "valid"; computed_root: string }
  | { kind: "mismatch"; computed_root: string; expected: string }
  | { kind: "unsupported"; algorithm: string }
  | { kind: "error"; message: string };

export function MerkleProofViewer({ proof }: MerkleProofViewerProps): React.ReactElement {
  const [status, setStatus] = React.useState<Status>({ kind: "pending" });

  React.useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const result: ProofVerificationResult = await verifyMerkleProof(proof);
        if (cancelled) return;
        if (result.ok) {
          setStatus({ kind: "valid", computed_root: result.computed_root });
          return;
        }
        if (result.reason === "algorithm_unsupported") {
          setStatus({ kind: "unsupported", algorithm: proof.algorithm });
          return;
        }
        setStatus({
          kind: "mismatch",
          computed_root: result.computed_root ?? "",
          expected: proof.expected_root,
        });
      } catch (err) {
        if (cancelled) return;
        setStatus({ kind: "error", message: err instanceof Error ? err.message : "unknown" });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [proof]);

  return (
    <section data-testid="merkle-proof-viewer" aria-label="Merkle inclusion proof">
      <header>
        <h3>Merkle inclusion proof</h3>
        <p>
          algorithm: <code>{proof.algorithm}</code>
        </p>
      </header>

      <ol data-testid="merkle-path">
        <li>
          leaf: <code>{proof.leaf_hash}</code>
        </li>
        {proof.siblings.map((sib, idx) => (
          <li key={`${sib.hash}-${idx}`}>
            sibling[{idx}] ({sib.position}): <code>{sib.hash}</code>
          </li>
        ))}
        <li>
          expected root: <code>{proof.expected_root}</code>
        </li>
      </ol>

      <div role="status" data-testid="merkle-status">
        {status.kind === "pending" && <span>Verifying…</span>}
        {status.kind === "valid" && (
          <span data-status="valid">Proof valid. Computed root matches.</span>
        )}
        {status.kind === "mismatch" && (
          <span data-status="mismatch">
            Proof MISMATCH. Computed <code>{status.computed_root}</code> ≠ expected{" "}
            <code>{status.expected}</code>.
          </span>
        )}
        {status.kind === "unsupported" && (
          <span data-status="unsupported">
            Proof algorithm <code>{status.algorithm}</code> not supported by this build.
          </span>
        )}
        {status.kind === "error" && (
          <span data-status="error">Verification error: {status.message}</span>
        )}
      </div>
    </section>
  );
}

export default MerkleProofViewer;
