// WI-S16-005 — client component for op detail. Handles MFA re-auth state,
// approval/reject calls.

"use client";

import React from "react";
import type { AdminOp } from "@/lib/types";
import type { AdminClient } from "@/lib/admin-client";
import DualApprovalCard from "./DualApprovalCard";

export interface OpDetailViewProps {
  initialOp: AdminOp;
  currentUserId: string | null;
  mfaFresh: boolean;
  client: AdminClient;
  onMfaStepUp?: () => void;
}

export function OpDetailView({
  initialOp,
  currentUserId,
  mfaFresh,
  client,
  onMfaStepUp,
}: OpDetailViewProps): React.ReactElement {
  const [op, setOp] = React.useState<AdminOp>(initialOp);
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);

  const approve = async (reason: string) => {
    if (!mfaFresh) {
      onMfaStepUp?.();
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      const updated = await client.approveOp(op.op_id, reason);
      setOp(updated);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "unknown");
    } finally {
      setBusy(false);
    }
  };

  const reject = async (reason: string) => {
    if (!mfaFresh) {
      onMfaStepUp?.();
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      const updated = await client.rejectOp(op.op_id, reason);
      setOp(updated);
    } catch (e) {
      setErr(e instanceof Error ? e.message : "unknown");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div data-testid="op-detail-view" data-busy={busy}>
      <section aria-label="Op request">
        <h2>Request payload</h2>
        <pre>{JSON.stringify(op.payload, null, 2)}</pre>
      </section>

      <section aria-label="Impact summary">
        <h2>Impact</h2>
        <p>{op.impact_summary}</p>
        <p>Tenant scope: {op.tenant_scope.join(", ")}</p>
      </section>

      {!mfaFresh && (
        <p data-testid="mfa-warning" role="alert">
          Fresh MFA step-up required before signing. Approvals are disabled until you
          re-authenticate.
        </p>
      )}

      {err && (
        <p data-testid="op-error" role="alert">
          {err}
        </p>
      )}

      <DualApprovalCard
        op={op}
        currentUserId={currentUserId}
        onApprove={(reason) => void approve(reason)}
        onReject={(reason) => void reject(reason)}
      />
    </div>
  );
}

export default OpDetailView;
