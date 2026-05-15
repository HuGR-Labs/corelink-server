<<<<<<< HEAD
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
=======
/**
 * Customer-facing billing page — entry point for the Stripe Customer
 * Portal redirect flow.
 *
 * Scope (wt/r-prep-stripe-portal):
 *   - Render a short "Manage subscription" panel.
 *   - "Open Stripe Portal" button → POST
 *     `/api/v1/customer/billing/portal-session` → redirect.
 *
 * Out of scope (deferred to follow-ups):
 *   - Inline plan summary widget (lives in customer dashboard S-16).
 *   - In-app invoice viewer — covered by the Stripe Portal itself.
 *
 * Auth: gated by the `[locale]/customer/**` middleware matcher. We DO
 * NOT call any backend customer-id resolver here — the session POST
 * carries `tenant_id` which the server cross-checks against the JWT.
 */

import * as React from "react";
import { getAuthContext } from "@/lib/auth";
import { PortalLauncher } from "./PortalLauncher";

export default async function Page(props: {
  params: Promise<{ locale: string }>;
}): Promise<React.ReactElement> {
  const { locale } = await props.params;
  const auth = await getAuthContext();
  const tenantId = auth.org_id ?? auth.user_id ?? "tenant_unknown";

  return (
    <main aria-labelledby="billing-heading">
      <h1 id="billing-heading">Manage subscription</h1>
      <p>
        Update payment method, download invoices, change plan, or cancel
        your subscription. Clicking the button below opens the secure
        Stripe Customer Portal in this window. You will return to this
        page when you close the portal.
      </p>
      <PortalLauncher locale={locale} tenantId={tenantId} />
      <section aria-labelledby="portal-faq-heading">
        <h2 id="portal-faq-heading">What you can do in the portal</h2>
        <ul>
          <li>View and download past invoices.</li>
          <li>Update or remove credit cards.</li>
          <li>
            Switch between Free, Starter, Team, and Enterprise tiers
            (tier-matrix constraints apply — see the
            <a href="/how-to/billing/manage-subscription">customer guide</a>).
          </li>
          <li>Cancel your subscription.</li>
        </ul>
        <p>
          <strong>Note:</strong> usage metering, BYOK key rotation, and
          tenant-level configuration stay in CoreLink admin UI. The
          Stripe Portal only covers the billing surface.
        </p>
      </section>
    </main>
>>>>>>> wt/r-prep-stripe-portal
  );
}
