// WI-S16-005 — visualise + verify the Merkle inclusion proof client-side.

"use client";

import React from "react";
import type { MerkleProof } from "@/lib/types";
import { verifyMerkleProof, type ProofVerificationResult } from "@/lib/merkle";
import { Badge, Card } from "@/components/ui/linear";

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

  const statusTone: "neutral" | "success" | "warn" | "danger" =
    status.kind === "valid"
      ? "success"
      : status.kind === "mismatch" || status.kind === "error"
        ? "danger"
        : status.kind === "unsupported"
          ? "warn"
          : "neutral";

  return (
    <section data-testid="merkle-proof-viewer" aria-label="Merkle inclusion proof">
      <Card title="Merkle inclusion proof" meta={`algorithm: ${proof.algorithm}`}>
        <ol data-testid="merkle-path" className="lin-checklist">
          <li>
            <span className="lin-card__meta">leaf</span> <code>{proof.leaf_hash}</code>
          </li>
          {proof.siblings.map((sib, idx) => (
            <li key={`${sib.hash}-${idx}`}>
              <span className="lin-card__meta">
                sibling[{idx}] ({sib.position})
              </span>{" "}
              <code>{sib.hash}</code>
            </li>
          ))}
          <li>
            <span className="lin-card__meta">expected root</span>{" "}
            <code>{proof.expected_root}</code>
          </li>
        </ol>

        <div role="status" data-testid="merkle-status" className="lin-mt">
          {status.kind === "pending" && (
            <Badge tone="neutral" dot>
              Verifying…
            </Badge>
          )}
          {status.kind === "valid" && (
            <span data-status="valid">
              <Badge tone={statusTone} dot>
                Proof valid. Computed root matches.
              </Badge>
            </span>
          )}
          {status.kind === "mismatch" && (
            <span data-status="mismatch">
              <Badge tone={statusTone} dot>
                Proof MISMATCH
              </Badge>{" "}
              Computed <code>{status.computed_root}</code> ≠ expected{" "}
              <code>{status.expected}</code>.
            </span>
          )}
          {status.kind === "unsupported" && (
            <span data-status="unsupported">
              <Badge tone={statusTone} dot>
                Unsupported
              </Badge>{" "}
              Proof algorithm <code>{status.algorithm}</code> not supported by this build.
            </span>
          )}
          {status.kind === "error" && (
            <span data-status="error">
              <Badge tone={statusTone} dot>
                Verification error
              </Badge>{" "}
              {status.message}
            </span>
          )}
        </div>
      </Card>
    </section>
  );
}

export default MerkleProofViewer;
