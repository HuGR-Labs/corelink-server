/**
 * /[locale]/upgraded — Stripe Checkout success redirect target (Launch L3).
 *
 * This is the page Stripe sends the customer to after a successful
 * Checkout, via the `success_url` minted in
 * `apps/admin-ui/src/app/api/checkout/session/route.ts`:
 *
 *     `${origin}/${locale}/upgraded?session_id={CHECKOUT_SESSION_ID}`
 *
 * Stripe substitutes the real `cs_…` id for `{CHECKOUT_SESSION_ID}` on
 * redirect. Before this page existed the customer hit a 404 after paying.
 *
 * ⚠️  CORRECTNESS — this page is PURELY INFORMATIONAL. The presence of a
 * `session_id` in the URL is NOT proof of payment and is NEVER treated as
 * such here:
 *   - Anyone can craft `…/upgraded?session_id=anything`.
 *   - The authoritative tier activation (`tenants.plan = 'pro'`) is owned
 *     by the `checkout.session.completed` Stripe WEBHOOK, handled
 *     idempotently in `crates/corelink-tier-selection` (dedup by Stripe
 *     `evt_*` id — see `route.ts` header + `ledger.rs`). That webhook may
 *     land a beat after the browser redirect, hence the copy ("activates
 *     in a moment") and the link to Billing where the live plan state is
 *     fetched from `/v1/customer/billing` by `BillingClient`.
 *
 * UI: migrated to the Linear design language (frozen kit + globals.css
 * tokens), matching the customer dashboard. The dark canvas comes from the
 * page-scoped `.cx-shell` / `.cx-main` wrapper (no shared layout touched).
 *
 * PUBLIC w.r.t. middleware in the same sense as pricing — no Clerk import
 * and no backend call. (In practice the visitor is authenticated because
 * they just completed Checkout, but we render nothing tenant-specific, so
 * the page is safe and fast even if the session has lapsed.)
 */

import * as React from "react";
import Link from "next/link";
import type { Locale } from "@/i18n/LocaleContext";
import { Badge, Card } from "@/components/ui/linear";

export default async function UpgradedPage(props: {
  params: Promise<{ locale: Locale }>;
  searchParams: Promise<{ session_id?: string | string[] }>;
}): Promise<React.ReactElement> {
  const { locale } = await props.params;
  const { session_id } = await props.searchParams;

  // Normalise to a single string for display only. Never used for auth or
  // to gate any privileged action — see the file header.
  const sessionId = Array.isArray(session_id) ? session_id[0] : session_id;

  return (
    <div className="cx-shell lin">
      <main
        className="cx-main"
        data-testid="upgraded-root"
        aria-labelledby="upgraded-heading"
      >
        <div data-testid="upgraded-badge">
          <Badge tone="success" dot>
            Payment received
          </Badge>
        </div>

        <h1 id="upgraded-heading" data-testid="upgraded-heading">
          Subscription started
        </h1>

        <Card>
          <p data-testid="upgraded-subtitle">
            Thank you — your payment went through. Your plan activates in a
            moment. We&rsquo;re finishing the last step in the background; you
            don&rsquo;t need to do anything.
          </p>

          <p className="lin-mt lin-card__meta" data-testid="upgraded-note">
            Activation is confirmed by Stripe and can take a few seconds to
            appear. If your plan still shows the old tier, refresh your billing
            page shortly.
          </p>

          {/* CTAs — back to the dashboard (primary) and billing (ghost). */}
          <div
            className="lin-mt lin-card__actions"
            data-testid="upgraded-actions"
          >
            <Link
              href={`/${locale}/customer`}
              data-testid="upgraded-cta-dashboard"
              className="lin-btn lin-btn--primary lin-btn--sm"
            >
              Go to dashboard
            </Link>
            <Link
              href={`/${locale}/customer/billing`}
              data-testid="upgraded-cta-billing"
              className="lin-btn lin-btn--ghost lin-btn--sm"
            >
              View billing
            </Link>
          </div>
        </Card>

        {/* Reference id — display-only, helps support correlate a query with
            the Stripe Checkout session. Rendered only when present. */}
        {sessionId ? (
          <p
            className="lin-mt lin-card__meta"
            data-testid="upgraded-session-ref"
          >
            Reference: <code>{sessionId}</code>
          </p>
        ) : null}
      </main>
    </div>
  );
}
