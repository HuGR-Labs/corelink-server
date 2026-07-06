// /[locale]/customer/keys — PAT list + create/revoke + BYOK CMK status.

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import KeysClient from "@/components/customer/KeysClient";

export default function CustomerKeysPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-keys-heading">
        <h1 id="customer-keys-heading">Tokens</h1>
        <p>
          Create and manage personal access tokens (PATs) that authenticate your
          build tools to CoreLink, and review your BYOK key status.
        </p>
        <KeysClient />
      </main>
    </CustomerGuard>
  );
}
