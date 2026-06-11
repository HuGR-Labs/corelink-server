// /[locale]/customer/keys — PAT list + create/revoke + BYOK CMK status.

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import KeysClient from "@/components/customer/KeysClient";

export default function CustomerKeysPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-keys-heading">
        <h1 id="customer-keys-heading">Keys</h1>
        <p>Manage your personal access tokens and review BYOK CMK status.</p>
        <KeysClient />
      </main>
    </CustomerGuard>
  );
}
