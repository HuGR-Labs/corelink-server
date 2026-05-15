// WI-S16-005 — /[locale]/admin/ops/[op_id] — op detail + dual-approval.

import React from "react";
import RbacGuard from "@/components/admin/RbacGuard";
import OpDetailViewClient from "@/components/admin/OpDetailViewClient";
import { adminClient } from "@/lib/admin-client";
import { getAuthContext, mfaFresh } from "@/lib/auth";
import type { AdminOp } from "@/lib/types";

export interface OpPageProps {
  params: { locale: string; op_id: string };
}

async function loadOp(opId: string): Promise<AdminOp | null> {
  try {
    return await adminClient.getOp(opId);
  } catch {
    return null;
  }
}

export default async function AdminOpDetailPage({
  params,
}: OpPageProps): Promise<React.ReactElement> {
  const op = await loadOp(params.op_id);
  const auth = await getAuthContext();

  return (
    <RbacGuard>
      <main aria-labelledby="op-heading">
        <h1 id="op-heading">Op {params.op_id}</h1>
        {!op && <p role="alert">Op not found.</p>}
        {op && (
          <OpDetailViewClient
            initialOp={op}
            currentUserId={auth.user_id}
            mfaFresh={mfaFresh(auth)}
          />
        )}
      </main>
    </RbacGuard>
  );
}
