// /[locale]/customer/audit — tenant-scoped audit trail.

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import AuditClient from "@/components/customer/AuditClient";

export default function CustomerAuditPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-audit-heading">
        <h1 id="customer-audit-heading">Audit log</h1>
        <p>
          A tenant-scoped record of who did what: sign-ins, token lifecycle, encryption-key
          rotations, and team changes. Filter by date, event type, or severity — cryptographic
          chain verification (export + offline proof) is on the way.
        </p>
        <AuditClient />
      </main>
    </CustomerGuard>
  );
}
