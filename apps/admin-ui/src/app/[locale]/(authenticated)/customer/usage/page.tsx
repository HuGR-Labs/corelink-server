// /[locale]/customer/usage — CAS storage + reads/writes + daily breakdown.

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import UsageClient from "@/components/customer/UsageClient";

export default function CustomerUsagePage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-usage-heading">
        <h1 id="customer-usage-heading">Usage</h1>
        <p>CAS storage consumption, request counts, and daily breakdown for this period.</p>
        <UsageClient />
      </main>
    </CustomerGuard>
  );
}
