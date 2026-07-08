// WI-S16-005 — client component for op detail. Handles MFA re-auth state,
// approval/reject calls.

"use client";

import React from "react";
import type { AdminOp } from "@/lib/types";
import type { AdminClient } from "@/lib/admin-client";
import { Callout, Card, CodeBlock, InlineError } from "@/components/ui/linear";
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
      <Card title="Request payload">
        <CodeBlock code={JSON.stringify(op.payload, null, 2)} lang="json" />
      </Card>

      <Card title="Impact" className="lin-mt">
        <p>{op.impact_summary}</p>
        <p className="lin-card__meta lin-mt">Tenant scope: {op.tenant_scope.join(", ")}</p>
      </Card>

      {!mfaFresh && (
        <div data-testid="mfa-warning" role="alert" className="lin-mt">
          <Callout tone="warn">
            Fresh MFA step-up required before signing. Approvals are disabled until you
            re-authenticate.
          </Callout>
        </div>
      )}

      {err && (
        <div data-testid="op-error" role="alert" className="lin-mt">
          <InlineError error={err} />
        </div>
      )}

      <div className="lin-mt">
        <DualApprovalCard
          op={op}
          currentUserId={currentUserId}
          onApprove={(reason) => void approve(reason)}
          onReject={(reason) => void reject(reason)}
        />
      </div>
    </div>
  );
}

export default OpDetailView;
