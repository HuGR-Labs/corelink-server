/**
 * Customer-facing "Plan & billing" page.
 *
 * The whole billing surface — the two product axes (cache tier + runner SKU),
 * the ONE Stripe portal control ("Manage subscription"), the ONE upgrade path
 * (free-tier → Stripe Checkout), payment method + invoices — lives inside
 * <BillingClient />. This page is a thin, guarded shell.
 *
 * Redundancy removed (customer-dashboard audit, W7): the page previously
 * rendered a SECOND portal button (<PortalLauncher />, which printed a raw
 * redirect URL) and a SECOND upgrade button. Both are gone — the client owns
 * the single canonical control for each, via the frozen customer-client
 * wrappers (`startBillingPortal`, and the shared <UpgradeButton />).
 *
 * Auth: gated by the `[locale]/customer/**` middleware matcher; <CustomerGuard>
 * is the belt-and-braces check. The client resolves tenant server-side from the
 * JWT — no customer-id is passed from here.
 */

import * as React from "react";
import BillingClient from "@/components/customer/BillingClient";
import CustomerGuard from "@/components/customer/CustomerGuard";

export default async function Page(): Promise<React.ReactElement> {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-billing-heading">
        <h1 id="customer-billing-heading">Plan &amp; billing</h1>
        <BillingClient />
      </main>
    </CustomerGuard>
  );
}
