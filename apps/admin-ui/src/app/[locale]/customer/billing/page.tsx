// /[locale]/customer/billing — subscription state, invoices, payment method.

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import BillingClient from "@/components/customer/BillingClient";

export default function CustomerBillingPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-billing-heading">
        <h1 id="customer-billing-heading">Billing</h1>
        <p>Subscription, invoices, and payment method. Manage details via the billing portal.</p>
        <BillingClient />
      </main>
    </CustomerGuard>
  );
}
