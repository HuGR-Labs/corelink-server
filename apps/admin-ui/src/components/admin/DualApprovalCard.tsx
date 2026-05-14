// WI-S16-005 — visual state for a dual-approval op (pending / 1-of-2 /
// 2-of-2 / rejected).
//
// Server enforces "requestor cannot approve own request"; UI mirrors the rule
// by greying out the approve button when the current operator is the
// requestor or has already approved.

"use client";

import React from "react";
import type { AdminOp } from "@/lib/types";

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
    >
      <header>
        <h3>
          {op.op_type} · <span data-testid="approval-state">{state}</span>
        </h3>
        <p>
          requested by <code>{op.requestor}</code> at <code>{op.requested_at}</code>
        </p>
      </header>

      <ol data-testid="approval-list">
        {op.approvals.map((a, idx) => (
          <li key={`${a.approver}-${idx}`}>
            <strong>approver {idx + 1}:</strong> <code>{a.approver}</code> at{" "}
            <code>{a.approved_at}</code>
            <br />
            reason: <em>{a.reason}</em>
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
        <div data-testid="approval-actions">
          <label htmlFor="reason">Reason (required for audit trail)</label>
          <textarea
            id="reason"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            data-testid="approval-reason"
          />
          <button
            type="button"
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
          </button>
          <button
            type="button"
            data-testid="reject-btn"
            disabled={rejectDisabled}
            aria-disabled={rejectDisabled}
            onClick={() => onReject?.(reason)}
          >
            Reject
          </button>
        </div>
      )}
    </section>
  );
}

export default DualApprovalCard;
