// WI-S16-005 — visual state for a dual-approval op (pending / 1-of-2 /
// 2-of-2 / rejected).
//
// Server enforces "requestor cannot approve own request"; UI mirrors the rule
// by greying out the approve button when the current operator is the
// requestor or has already approved.
//
// Linear kit: glass Card, state Badge, Field + Textarea, primary/danger Buttons.

"use client";

import React from "react";
import type { AdminOp } from "@/lib/types";
import { Badge, Button, Field, Textarea } from "@/components/ui/linear";

export type DualApprovalState =
  | "pending"
  | "one-of-two"
  | "two-of-two"
  | "rejected";

export function dualApprovalState(op: AdminOp): DualApprovalState {
  if (op.status === "rejected") return "rejected";
  if (op.approvals.length === 0) return "pending";
  if (op.approvals.length === 1) return "one-of-two";
  return "two-of-two";
}

const STATE_TONE: Record<DualApprovalState, "neutral" | "success" | "warn" | "danger"> = {
  pending: "warn",
  "one-of-two": "warn",
  "two-of-two": "success",
  rejected: "danger",
};

export interface DualApprovalCardProps {
  op: AdminOp;
  currentUserId: string | null;
  onApprove?: (reason: string) => void;
  onReject?: (reason: string) => void;
}

export function DualApprovalCard({
  op,
  currentUserId,
  onApprove,
  onReject,
}: DualApprovalCardProps): React.ReactElement {
  const [reason, setReason] = React.useState("");
  const state = dualApprovalState(op);

  const isRequestor = currentUserId !== null && currentUserId === op.requestor;
  const alreadyApproved =
    currentUserId !== null &&
    op.approvals.some((a) => a.approver === currentUserId);
  const isTerminal = state === "two-of-two" || state === "rejected";
  const approveDisabled =
    isRequestor || alreadyApproved || isTerminal || reason.trim().length === 0;
  const rejectDisabled = isTerminal || reason.trim().length === 0;

  return (
    <section
      data-testid="dual-approval-card"
      data-state={state}
      aria-label={`Dual approval status: ${state}`}
      className="lin-card lin-card--pad"
    >
      <header className="lin-card__head">
        <div>
          <h3 className="lin-card__title">
            {op.op_type} ·{" "}
            <span data-testid="approval-state">
              <Badge tone={STATE_TONE[state]} dot>
                {state}
              </Badge>
            </span>
          </h3>
          <p className="lin-card__meta">
            requested by <code>{op.requestor}</code> at <code>{op.requested_at}</code>
          </p>
        </div>
      </header>

      <ol data-testid="approval-list" className="lin-checklist lin-mt">
        {op.approvals.map((a, idx) => (
          <li key={`${a.approver}-${idx}`}>
            <strong>approver {idx + 1}:</strong> <code>{a.approver}</code> at{" "}
            <code>{a.approved_at}</code>
            <br />
            <span className="lin-card__meta">
              reason: <em>{a.reason}</em>
            </span>
          </li>
        ))}
        {state === "pending" && <li data-testid="awaiting-1">awaiting approver 1</li>}
        {state === "one-of-two" && <li data-testid="awaiting-2">awaiting approver 2</li>}
        {state === "rejected" && op.rejection && (
          <li data-testid="rejection-detail">
            rejected by <code>{op.rejection.rejector}</code> at{" "}
            <code>{op.rejection.rejected_at}</code> — {op.rejection.reason}
          </li>
        )}
      </ol>

      {!isTerminal && (
        <div data-testid="approval-actions" className="lin-mt">
          <Field label="Reason (required for audit trail)" htmlFor="reason">
            <Textarea
              id="reason"
              value={reason}
              onChange={(e) => setReason(e.target.value)}
              data-testid="approval-reason"
            />
          </Field>
          <div className="lin-mt">
            <Button
              variant="primary"
              size="sm"
              data-testid="approve-btn"
              disabled={approveDisabled}
              aria-disabled={approveDisabled}
              title={
                isRequestor
                  ? "Requestor cannot approve own request"
                  : alreadyApproved
                    ? "You already approved this op"
                    : undefined
              }
              onClick={() => onApprove?.(reason)}
            >
              Approve
            </Button>{" "}
            <Button
              variant="danger"
              size="sm"
              data-testid="reject-btn"
              disabled={rejectDisabled}
              aria-disabled={rejectDisabled}
              onClick={() => onReject?.(reason)}
            >
              Reject
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}

export default DualApprovalCard;
