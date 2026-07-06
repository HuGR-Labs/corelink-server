// /[locale]/customer/runners — ephemeral CI runners (W0 stub; filled by W9).

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";

export default function CustomerRunnersPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-runners-heading">
        <h1 id="customer-runners-heading">Runners</h1>
        <p>Ephemeral CI runners: flat per parallel runner, unlimited minutes, warmed by your cache.</p>
      </main>
    </CustomerGuard>
  );
}
