// /[locale]/customer/connect — point a build tool at CoreLink (W2).

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import ConnectClient from "@/components/customer/ConnectClient";

export default function CustomerConnectPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-connect-heading">
        <h1 id="customer-connect-heading">Connect a tool</h1>
        <p>Point Bazel, Turborepo, sccache, npm or pip at your CoreLink cache with a copy-paste config.</p>
        <ConnectClient />
      </main>
    </CustomerGuard>
  );
}
