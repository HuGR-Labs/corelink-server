// /[locale]/customer/trust — trust & compliance center (W8).

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import TrustClient from "@/components/customer/TrustClient";

export default function CustomerTrustPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-trust-heading">
        <h1 id="customer-trust-heading">Trust &amp; compliance</h1>
        <p>Encryption (BYOK), data residency, the verifiable audit chain, and data-subject requests.</p>
        <TrustClient />
      </main>
    </CustomerGuard>
  );
}
