// WI-S16-005 — DualApprovalCard tests covering all 4 visual states +
// requestor-cannot-approve-own-request grey-out.

import React from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import DualApprovalCard from "@/components/admin/DualApprovalCard";
import type { AdminOp } from "@/lib/types";

function makeOp(over: Partial<AdminOp> = {}): AdminOp {
  return {
    op_id: "op_1",
    op_type: "byok_cmk_rotation",
    requestor: "u_requestor",
    requested_at: "2026-05-14T10:00:00Z",
    status: "awaiting_approval",
    payload: {},
    impact_summary: "rotate cmk",
    tenant_scope: ["tenant_a"],
    approvals: [],
    ...over,
  };
}

describe("DualApprovalCard", () => {
  it("renders 'pending' state (0 approvals)", () => {
    render(<DualApprovalCard op={makeOp()} currentUserId="u_alice" />);
    expect(screen.getByTestId("approval-state")).toHaveTextContent("pending");
    expect(screen.getByTestId("awaiting-1")).toBeInTheDocument();
  });

  it("renders 'one-of-two' state (1 approval)", () => {
    const op = makeOp({
      approvals: [
        { approver: "u_alice", approved_at: "2026-05-14T10:01:00Z", reason: "ok" },
      ],
    });
    render(<DualApprovalCard op={op} currentUserId="u_bob" />);
    expect(screen.getByTestId("approval-state")).toHaveTextContent("one-of-two");
    expect(screen.getByTestId("awaiting-2")).toBeInTheDocument();
  });

  it("renders 'two-of-two' state (2 approvals)", () => {
    const op = makeOp({
      status: "approved",
      approvals: [
        { approver: "u_alice", approved_at: "2026-05-14T10:01:00Z", reason: "ok" },
        { approver: "u_bob", approved_at: "2026-05-14T10:02:00Z", reason: "ok" },
      ],
    });
    render(<DualApprovalCard op={op} currentUserId="u_carol" />);
    expect(screen.getByTestId("approval-state")).toHaveTextContent("two-of-two");
    expect(screen.queryByTestId("approval-actions")).not.toBeInTheDocument();
  });

  it("renders 'rejected' state with rejection details", () => {
    const op = makeOp({
      status: "rejected",
      rejection: {
        rejector: "u_bob",
        rejected_at: "2026-05-14T10:03:00Z",
        reason: "blast radius too wide",
      },
    });
    render(<DualApprovalCard op={op} currentUserId="u_carol" />);
    expect(screen.getByTestId("approval-state")).toHaveTextContent("rejected");
    expect(screen.getByTestId("rejection-detail")).toHaveTextContent(/blast radius/);
  });

  it("greys out approve button when current user is the requestor", () => {
    render(<DualApprovalCard op={makeOp()} currentUserId="u_requestor" />);
    const btn = screen.getByTestId("approve-btn") as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    expect(btn.title).toMatch(/cannot approve own/);
  });

  it("greys out approve button when current user already approved", () => {
    const op = makeOp({
      approvals: [
        { approver: "u_alice", approved_at: "2026-05-14T10:01:00Z", reason: "ok" },
      ],
    });
    render(<DualApprovalCard op={op} currentUserId="u_alice" />);
    const btn = screen.getByTestId("approve-btn") as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it("calls onApprove with reason when approve clicked", () => {
    const onApprove = vi.fn();
    render(
      <DualApprovalCard
        op={makeOp()}
        currentUserId="u_alice"
        onApprove={onApprove}
      />,
    );
    fireEvent.change(screen.getByTestId("approval-reason"), {
      target: { value: "rotation justified" },
    });
    fireEvent.click(screen.getByTestId("approve-btn"));
    expect(onApprove).toHaveBeenCalledWith("rotation justified");
  });
});
