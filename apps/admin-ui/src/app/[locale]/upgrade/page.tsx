/**
 * /[locale]/upgrade — public pricing CTA → Stripe Checkout bridge (#49).
 *
 * Every public docs pricing CTA targets
 * `https://humangr.com/corelink/upgrade?plan=<tier>` (see
 * `apps/docs/src/pages/pricing.tsx` `ctaForTier`). Before this page
 * existed that URL 404'd — the checkout plumbing
 * (`/api/checkout/session` + `<UpgradeButton />`) was built but had no
 * routable entry point. The locale-less `/upgrade` shape the CTAs use is
 * forwarded here by `src/app/upgrade/route.ts` (default locale `en`,
 * same convention as the `/` landing page and `/sign-up` → `/en/welcome`).
 *
 * Flow:
 *   1. Validate `?plan=` against the checkout-able tiers
 *      (`CHECKOUT_TIER_IDS` — solo/starter/team/pro/max); missing or
 *      invalid values fall back to the anchor SKU `pro`
 *      (`normalizeCheckoutTier`, src/lib/pricing.ts — the same set the
 *      `/api/checkout/session` gate enforces, so validation can't drift).
 *   2. SIGNED-OUT → `redirect(/sign-in?redirect_url=/<locale>/upgrade?plan=<tier>)`.
 *      The session probe mirrors `getSessionToken()` in
 *      `app/api/checkout/session/route.ts` verbatim (lazy Clerk import →
 *      `auth().getToken()`), so this page gates on EXACTLY the predicate
 *      the checkout POST will 401 on. Clerk's `<SignIn />` widget honours
 *      the standard `redirect_url` query param (the sign-in page sets only
 *      `fallbackRedirectUrl`, NEVER `forceRedirectUrl` — force would
 *      unconditionally override `redirect_url` and strand the buyer on the
 *      dashboard; regression-locked in tests/clerk-basepath.test.tsx), so
 *      after sign-in the visitor lands back here
 *      and checkout auto-fires. Brand-NEW users who choose "Sign up"
 *      inside the widget land on `/en/welcome` instead — that page sets
 *      `forceRedirectUrl` by design (one-time PAT reveal + DPA-first
 *      ordering must precede any tier-select; the backend would 403 the
 *      checkout anyway until the DPA is accepted).
 *   3. SIGNED-IN → render a minimal tier card and immediately drive the
 *      checkout via `<UpgradeButton autoStart />` — the exact POST →
 *      `window.location.assign(checkout_url)` path the billing page
 *      button runs, including the inline error + manual-retry fallback.
 *
 * Middleware: no matcher change needed — `/[locale]/upgrade` is not in
 * `PUBLIC_PATH_PREFIXES` (src/lib/route-matcher.ts), so the root
 * middleware already runs `clerkMiddleware` for it, which is what makes
 * the server-side `auth()` probe above resolvable.
 *
 * UI: migrated to the Linear design language (frozen kit + globals.css
 * tokens), matching the customer dashboard. The dark canvas comes from the
 * page-scoped `.cx-shell` / `.cx-main` wrapper (no shared layout touched).
 */

import * as React from "react";
import Link from "next/link";
import { redirect } from "next/navigation";
import type { Locale } from "@/i18n/LocaleContext";
import { TIERS, normalizeCheckoutTier } from "@/lib/pricing";
import { withAppBasePath } from "@/lib/route-matcher";
import { UpgradeButton } from "@/components/UpgradeButton";
import { Callout, Card } from "@/components/ui/linear";

/**
 * Resolve the Clerk session token at the server boundary. Verbatim
 * mirror of `getSessionToken()` in `app/api/checkout/session/route.ts`
 * (lazy-import so tests / non-Clerk environments don't crash) — this
 * page must gate on the same predicate the checkout POST authenticates
 * with.
 */
async function getSessionToken(): Promise<string | null> {
  const mod = await import("@clerk/nextjs/server").catch(() => null);
  if (!mod) return null;
  try {
    const session = await (
      mod as { auth: () => Promise<{ getToken: () => Promise<string | null> }> }
    ).auth();
    return await session.getToken();
  } catch {
    return null;
  }
}

export default async function UpgradePage(props: {
  params: Promise<{ locale: Locale }>;
  searchParams: Promise<{ plan?: string | string[] }>;
}): Promise<React.ReactElement> {
  const { locale } = await props.params;
  const { plan } = await props.searchParams;

  // Spec #49 (1): solo|starter|team|pro|max; invalid/missing → "pro".
  const tier = normalizeCheckoutTier(plan);

  // Spec #49 (2): signed-out visitors round-trip through Clerk sign-in
  // and return to this exact page (normalized plan preserved).
  const token = await getSessionToken();
  if (!token) {
    // The `redirect()` TARGET is basePath-auto-applied by Next, but the
    // `redirect_url` VALUE is consumed by Clerk — which navigates without
    // Next's router and therefore without basePath (proven live: a plain
    // sign-in landed on `humangr.com/en/customer`, the marketing site). So
    // the return URL must carry `/corelink` explicitly, or the buyer bounces
    // out of the app instead of back here to auto-fire checkout.
    redirect(
      `/sign-in?redirect_url=${encodeURIComponent(
        withAppBasePath(`/${locale}/upgrade?plan=${tier}`),
      )}`,
    );
  }

  // Display data from the launch rate card. Legacy `team` is checkout-able
  // but no longer on the public 6-tier card — fall back to a capitalized
  // name with no price line rather than inventing a number.
  const card = TIERS.find((t) => t.id === tier);
  const tierName = card?.name ?? tier.charAt(0).toUpperCase() + tier.slice(1);

  return (
    <div className="cx-shell lin">
      <main
        className="cx-main"
        data-testid="upgrade-root"
        aria-labelledby="upgrade-heading"
      >
        <h1 id="upgrade-heading" data-testid="upgrade-heading">
          Upgrade to {tierName}
        </h1>

        <Card>
          {card?.price ? (
            <div className="lin-stat__value" data-testid="upgrade-price">
              {card.price}{" "}
              <span className="lin-card__meta">{card.cadence}</span>
            </div>
          ) : null}

          {/* Redirect status — announced to screen readers while the
              auto-started checkout POST mints the Stripe session and the
              browser hands off. */}
          <div
            className="lin-mt"
            role="status"
            aria-live="polite"
            data-testid="upgrade-redirect-status"
          >
            <Callout tone="info">
              Taking you to secure Stripe Checkout&hellip; CoreLink never sees
              your card number — Stripe handles all payment data.
            </Callout>
          </div>

          {/* Auto-start + manual fallback in one control: the POST fires on
              mount; if it fails (e.g. the DPA-first 403) the error surfaces
              inline and this same button retries. */}
          <div className="lin-mt" data-testid="upgrade-action">
            <UpgradeButton locale={locale} tier={tier} autoStart />
          </div>
        </Card>

        <p className="lin-mt lin-card__meta" data-testid="upgrade-note">
          Not redirected automatically? Use the button above, or{" "}
          <Link href={`/${locale}/pricing`} data-testid="upgrade-pricing-link">
            compare all plans
          </Link>
          .
        </p>
      </main>
    </div>
  );
}
