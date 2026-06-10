/**
 * Customer-facing billing page — entry point for both:
 *   - The Stripe Customer Portal redirect flow (existing tenants
 *     managing their subscription).
 *   - The Stripe Checkout Session upgrade flow (Phase 0.C — PLG
 *     defer-billing: free-tier tenants upgrading to Pro).
 *
 * Scope:
 *   - Render "Upgrade to Pro" → POST `/api/checkout/session` →
 *     Stripe-hosted Checkout. (Phase 0.C / launch-readiness §2.)
 *   - Render "Manage subscription" → POST
 *     `/api/v1/customer/billing/portal-session` → Stripe Customer
 *     Portal.
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
import { UpgradeButton } from "@/components/UpgradeButton";
import BillingClient from "@/components/customer/BillingClient";
import CustomerGuard from "@/components/customer/CustomerGuard";

export default async function Page(props: {
  params: Promise<{ locale: string }>;
}): Promise<React.ReactElement> {
  const { locale } = await props.params;
  const auth = await getAuthContext();
  const tenantId = auth.org_id ?? auth.user_id ?? "tenant_unknown";

  return (
    <CustomerGuard>
      <main aria-labelledby="customer-billing-heading">
        <h1 id="customer-billing-heading">Billing</h1>

        {/* Subscription summary + invoice list (BillingClient fetches /v1/customer/billing) */}
        <BillingClient />

        <section aria-labelledby="upgrade-heading">
          <h2 id="upgrade-heading">Upgrade to Pro</h2>
          <p>
            Get unmetered cache hits, BYOK across all regions, and
            priority support. Clicking the button below opens Stripe
            Checkout in this window. CoreLink never sees your card
            number — Stripe handles all payment data.
          </p>
          <UpgradeButton locale={locale} tier="pro" />
        </section>

        <section aria-labelledby="manage-heading">
          <h2 id="manage-heading">Manage subscription via Stripe portal</h2>
          <p>
            Clicking the button below opens the secure Stripe Customer Portal
            in this window. You will return to this page when you close the
            portal.
          </p>
          {/* PortalLauncher posts to /api/v1/customer/billing/portal-session */}
          <PortalLauncher locale={locale} tenantId={tenantId} />
        </section>

        <section aria-labelledby="portal-faq-heading">
          <h2 id="portal-faq-heading">What you can do in the portal</h2>
          <ul>
            <li>View and download past invoices.</li>
            <li>Update or remove credit cards.</li>
            <li>
              Switch between Free, Starter, Team, and Enterprise tiers
              (tier-matrix constraints apply — see the
              {/* Cross-app link to docs site — not a Next.js page */}
              {/* eslint-disable-next-line @next/next/no-html-link-for-pages */}
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
    </CustomerGuard>
  );
}
