// /[locale]/customer/workspaces — clw snapshots + Pin (W10).

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import WorkspacesClient from "@/components/customer/WorkspacesClient";

export default function CustomerWorkspacesPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-workspaces-heading">
        <h1 id="customer-workspaces-heading">Workspaces</h1>
        <p>
          Snapshot a workspace to content-addressed storage and hydrate it in
          seconds. Pin the ones you use most for guaranteed warmth.
        </p>
        <WorkspacesClient />
      </main>
    </CustomerGuard>
  );
}
