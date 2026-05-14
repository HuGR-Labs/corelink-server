// WI-S16-005 — /[locale]/admin/ops — queue of sensitive ops awaiting
// dual approval.

import React from "react";
import Link from "next/link";
import RbacGuard from "@/components/admin/RbacGuard";
import { adminClient } from "@/lib/admin-client";

export default async function AdminOpsPage(): Promise<React.ReactElement> {
  let ops: Awaited<ReturnType<typeof adminClient.listOps>> = [];
  try {
    ops = await adminClient.listOps();
  } catch {
    ops = [];
  }

  return (
    <RbacGuard>
      <main aria-labelledby="ops-heading">
        <h1 id="ops-heading">Sensitive operations queue</h1>
        <p>
          BYOK CMK rotation, tenant data export, account deletion, and data residency
          changes require two distinct approvers.
        </p>
        <table data-testid="ops-table">
          <thead>
            <tr>
              <th>op_id</th>
              <th>op_type</th>
              <th>requestor</th>
              <th>requested_at</th>
              <th>status</th>
              <th>actions</th>
            </tr>
          </thead>
          <tbody>
            {ops.length === 0 && (
              <tr>
                <td colSpan={6}>No ops queued.</td>
              </tr>
            )}
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
                <td data-status={op.status}>{op.status}</td>
                <td>
                  <Link href={`./ops/${op.op_id}`}>view</Link>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </main>
    </RbacGuard>
  );
}
