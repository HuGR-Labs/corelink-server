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
 * Mirrors the conventions of the sibling public page
 * `[locale]/pricing/page.tsx`: a server component, `params` as a Promise
 * (Next 15), Tailwind styling, hardcoded English copy. We do NOT call a
 * translation function here (pricing/page.tsx renders literals the same
 * way); the corresponding `upgraded.*` message keys live in
 * `src/i18n/locales/en.json` alongside the existing `pricing.*` keys for
 * consistency / future wiring.
 *
 * PUBLIC w.r.t. middleware in the same sense as pricing — no Clerk import
 * and no backend call. (In practice the visitor is authenticated because
 * they just completed Checkout, but we render nothing tenant-specific, so
 * the page is safe and fast even if the session has lapsed.)
 */

import * as React from "react";
import Link from "next/link";
import type { Locale } from "@/i18n/LocaleContext";

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
    <main
      className="mx-auto flex max-w-2xl flex-col items-center px-4 py-24 text-center sm:px-6 lg:px-8"
      data-testid="upgraded-root"
    >
      {/* Success badge — inline SVG, no extra dependency (mirrors the
          checkmark style used in pricing/page.tsx). */}
      <span
        className="flex h-16 w-16 items-center justify-center rounded-full bg-indigo-50 ring-2 ring-indigo-500"
        data-testid="upgraded-badge"
        aria-hidden="true"
      >
        <svg
          className="h-8 w-8 text-indigo-600"
          viewBox="0 0 24 24"
          fill="none"
          xmlns="http://www.w3.org/2000/svg"
        >
          <path
            d="M5 13l4 4L19 7"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </span>

      <h1
        className="mt-8 text-4xl font-bold tracking-tight text-slate-900"
        data-testid="upgraded-heading"
      >
        Subscription started
      </h1>

      <p className="mt-4 text-lg text-slate-600" data-testid="upgraded-subtitle">
        Thank you — your payment went through. Your plan activates in a
        moment. We&rsquo;re finishing the last step in the background; you
        don&rsquo;t need to do anything.
      </p>

      <p className="mt-4 text-sm text-slate-500" data-testid="upgraded-note">
        Activation is confirmed by Stripe and can take a few seconds to
        appear. If your plan still shows the old tier, refresh your billing
        page shortly.
      </p>

      {/* CTAs — back to the dashboard (primary) and billing (secondary).
          Styling matches the pricing-page CTA buttons. */}
      <div
        className="mt-10 flex flex-col gap-3 sm:flex-row"
        data-testid="upgraded-actions"
      >
        <Link
          href={`/${locale}/customer`}
          data-testid="upgraded-cta-dashboard"
          className="block w-full rounded-lg bg-indigo-600 px-5 py-2.5 text-center text-sm font-semibold text-white transition-colors hover:bg-indigo-700 sm:w-auto"
        >
          Go to dashboard
        </Link>
        <Link
          href={`/${locale}/customer/billing`}
          data-testid="upgraded-cta-billing"
          className="block w-full rounded-lg border border-slate-300 bg-white px-5 py-2.5 text-center text-sm font-semibold text-slate-700 transition-colors hover:bg-slate-50 sm:w-auto"
        >
          View billing
        </Link>
      </div>

      {/* Reference id — display-only, helps support correlate a query with
          the Stripe Checkout session. Rendered only when present. */}
      {sessionId ? (
        <p
          className="mt-12 break-all text-xs text-slate-400"
          data-testid="upgraded-session-ref"
        >
          Reference: <code>{sessionId}</code>
        </p>
      ) : null}
    </main>
  );
}
