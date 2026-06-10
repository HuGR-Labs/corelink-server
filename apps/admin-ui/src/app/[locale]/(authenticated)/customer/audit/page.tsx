// /[locale]/customer/audit — tenant-scoped audit trail.

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import AuditClient from "@/components/customer/AuditClient";

export default function CustomerAuditPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-audit-heading">
        <h1 id="customer-audit-heading">Audit trail</h1>
        <p>
          Events for your tenant: sign-ins, PAT lifecycle, BYOK rotations, team changes.
          For Merkle proof verification, contact your operator.
        </p>
        <AuditClient />
      </main>
    </CustomerGuard>
  );
}
