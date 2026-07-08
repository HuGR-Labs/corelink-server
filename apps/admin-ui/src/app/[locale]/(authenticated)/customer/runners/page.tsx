// /[locale]/customer/runners — ephemeral CI runners (W9).
//
// Flat per parallel runner, unlimited minutes, warmed by your cache. The
// GitHub-App install is the ONE wired action; entitlement / consumed-vCPU-h /
// repo-allowlist are [not-wired → BE-10] and render honest teaching states.

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import RunnersClient from "@/components/customer/RunnersClient";

export default function CustomerRunnersPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-runners-heading">
        <h1 id="customer-runners-heading">Runners</h1>
        <p>
          Ephemeral CI runners: flat per parallel runner, unlimited minutes, warmed
          by your cache. Install the GitHub App to connect your repositories.
        </p>
        <RunnersClient />
      </main>
    </CustomerGuard>
  );
}
