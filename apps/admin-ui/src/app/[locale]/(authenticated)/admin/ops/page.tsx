// WI-S16-005 — /[locale]/admin/ops — queue of sensitive ops awaiting
// dual approval.

import React from "react";
import Link from "next/link";
import RbacGuard from "@/components/admin/RbacGuard";
import { adminClient } from "@/lib/admin-client";
import { Badge, Card, EmptyState } from "@/components/ui/linear";
import { opStatusTone } from "@/components/admin/tones";

export default async function AdminOpsPage(): Promise<React.ReactElement> {
  let ops: Awaited<ReturnType<typeof adminClient.listOps>> = [];
  try {
    ops = await adminClient.listOps();
  } catch {
    ops = [];
  }

  return (
    <RbacGuard>
      <div className="cx-shell lin">
        <div className="cx-main">
          <main aria-labelledby="ops-heading">
            <h1 id="ops-heading">Sensitive operations queue</h1>
            <p>
              BYOK CMK rotation, tenant data export, account deletion, and data residency
              changes require two distinct approvers.
            </p>

            <div className="lin-mt-lg">
              <Card title="Queue">
                {ops.length === 0 ? (
                  <EmptyState
                    title="No ops queued"
                    body="Sensitive operations awaiting dual approval will appear here."
                  />
                ) : (
                  <table className="lin-table" data-testid="ops-table">
                    <thead>
                      <tr>
                        <th>Op ID</th>
                        <th>Op type</th>
                        <th>Requestor</th>
                        <th>Requested</th>
                        <th>Status</th>
                        <th>Actions</th>
                      </tr>
                    </thead>
                    <tbody>
                      {ops.map((op) => (
                        <tr key={op.op_id} data-testid={`op-row-${op.op_id}`}>
                          <td>
                            <code>{op.op_id}</code>
                          </td>
                          <td>{op.op_type}</td>
                          <td>
                            <code>{op.requestor}</code>
                          </td>
                          <td>{op.requested_at}</td>
                          <td data-status={op.status}>
                            <Badge tone={opStatusTone(op.status)} dot>
                              {op.status}
                            </Badge>
                          </td>
                          <td>
                            <Link
                              className="lin-btn lin-btn--ghost lin-btn--sm"
                              href={`./ops/${op.op_id}`}
                            >
                              View
                            </Link>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                )}
              </Card>
            </div>
          </main>
        </div>
      </div>
    </RbacGuard>
  );
}
