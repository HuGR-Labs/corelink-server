// /[locale]/customer/usage — CAS storage + reads/writes + daily breakdown.

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import UsageClient from "@/components/customer/UsageClient";

export default function CustomerUsagePage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-usage-heading">
        <h1 id="customer-usage-heading">Usage &amp; savings</h1>
        <p>
          Your storage cap, request usage, and the ROI your cache is earning — hit
          rate, build time saved, and dollars saved as your cache warms up.
        </p>
        <UsageClient />
      </main>
    </CustomerGuard>
  );
}
