// WI-S16-005 — /[locale]/admin/audit — paginated audit log viewer.

import React from "react";
import RbacGuard from "@/components/admin/RbacGuard";
import AuditViewerClient from "@/components/admin/AuditViewerClient";

export default function AdminAuditPage(): React.ReactElement {
  return (
    <RbacGuard>
      <div className="cx-shell lin">
        <div className="cx-main">
          <main aria-labelledby="audit-heading">
            <h1 id="audit-heading">Audit log</h1>
            <p>
              Per-tenant operator audit viewer. Filter, inspect CloudEvent envelope, verify
              Merkle proof, and export sanitized snapshots.
            </p>
            <div className="lin-mt-lg">
              <AuditViewerClient />
            </div>
          </main>
        </div>
      </div>
    </RbacGuard>
  );
}
