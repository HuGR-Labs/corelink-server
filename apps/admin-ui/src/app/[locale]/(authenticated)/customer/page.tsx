// /[locale]/customer — self-serve overview (usage + audit + billing + activity).

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import OverviewClient from "@/components/customer/OverviewClient";

export default function CustomerOverviewPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-overview-heading">
        <h1 id="customer-overview-heading">Overview</h1>
        <p>Snapshot of your tenant: usage, billing, BYOK status, and recent activity.</p>
        <OverviewClient />
      </main>
    </CustomerGuard>
  );
}
