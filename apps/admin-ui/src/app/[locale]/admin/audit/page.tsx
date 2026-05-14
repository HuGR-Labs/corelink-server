// WI-S16-005 — /[locale]/admin/audit — paginated audit log viewer.

import React from "react";
import RbacGuard from "@/components/admin/RbacGuard";
import AuditViewer from "@/components/admin/AuditViewer";
import { adminClient } from "@/lib/admin-client";

export default function AdminAuditPage(): React.ReactElement {
  return (
    <RbacGuard>
      <main aria-labelledby="audit-heading">
        <h1 id="audit-heading">Audit log</h1>
        <p>
          Per-tenant operator audit viewer. Filter, inspect CloudEvent envelope, verify
          Merkle proof, and export sanitized snapshots.
        </p>
        <AuditViewer client={adminClient} />
      </main>
    </RbacGuard>
  );
}
