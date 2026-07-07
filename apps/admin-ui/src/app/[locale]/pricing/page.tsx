/**
 * /[locale]/pricing — public pricing page.
 *
 * PUBLIC — no Clerk import, no auth middleware required.
 * Server component. Tier data sourced from src/lib/pricing.ts.
 * Org-wide SoT: docs/POSITIONING.md.
 *
 * Renders TWO distinct card sections so the two product axes are never
 * conflated:
 *   - Cache & storage ladder: Free | Solo | Starter | Pro (highlighted)
 *     | Max | Enterprise (the `group: "cache"` / default tiers).
 *   - CI runners (self-serve, separate product): Runner Starter …
 *     Runner Max (the `group: "runner"` tiers) — priced on concurrency +
 *     monthly vCPU-h, not storage.
 *
 * Linear-doctrine (dark) surface — matches the customer dashboard. Uses the
 * frozen kit (Card / Badge / Button) + a11y-validated tokens (--t1/--t2 on
 * --bg). No raw hex, no off-grid px, no inline styles.
 */

import * as React from "react";
import Link from "next/link";
import { PublicShell } from "@/components/public/PublicShell";
import { PublicPageHeader } from "@/components/public/PublicPageHeader";
import { Badge } from "@/components/ui/linear";
import type { Locale } from "@/i18n/LocaleContext";
import { TIERS, type Tier } from "@/lib/pricing";

/**
 * A single tier card. Shared by both the cache-ladder and the runner
 * grids so the markup can never drift between the two sections.
 */
function TierCard(props: {
  tier: Tier;
  locale: Locale;
}): React.ReactElement {
  const { tier, locale } = props;
  const cardClasses = [
    "lin-card lin-card--pad lin-card--hover relative flex flex-col",
    tier.highlight ? "border-[var(--line-2)] ring-1 ring-[var(--line-2)]" : "",
  ].join(" ");

  const ctaClasses = [
    "lin-btn w-full",
    tier.highlight ? "lin-btn--primary" : "lin-btn--ghost",
  ].join(" ");

  return (
    <article
      role="listitem"
      data-testid={`tier-card-${tier.id}`}
      aria-label={`${tier.name} tier`}
      className={cardClasses}
    >
      {/* Most popular badge */}
      {tier.highlight && (
        <span
          className="absolute -top-3 left-1/2 -translate-x-1/2"
          data-testid={`most-popular-badge-${tier.id}`}
        >
          <Badge tone="neutral">Most popular</Badge>
        </span>
      )}

      {/* Tier name */}
      <h3
        className="text-base font-[560] text-[var(--t1)]"
        data-testid={`tier-name-${tier.id}`}
      >
        {tier.name}
      </h3>

      {/* Price */}
      <div
        className="mt-4 flex items-baseline gap-1"
        data-testid={`tier-price-${tier.id}`}
      >
        {tier.price ? (
          <>
            <span className="text-[28px] font-[560] tracking-[-0.02em] text-[var(--t1)]">
              {tier.price}
            </span>
            <span className="text-sm text-[var(--t3)]">{tier.cadence}</span>
          </>
        ) : (
          <span className="text-xl font-[560] text-[var(--t2)]">Talk to us</span>
        )}
      </div>

      {/* Feature list */}
      <ul
        className="mt-6 flex-1 space-y-3"
        data-testid={`tier-features-${tier.id}`}
        aria-label={`${tier.name} features`}
      >
        {tier.features.map((feature) => (
          <li
            key={feature}
            className="flex items-start gap-2 text-sm text-[var(--t2)]"
          >
            {/* Checkmark — inline SVG, no extra dependency */}
            <svg
              aria-hidden="true"
              className="mt-0.5 h-4 w-4 shrink-0 text-[var(--t3)]"
              viewBox="0 0 16 16"
              fill="none"
              xmlns="http://www.w3.org/2000/svg"
            >
              <circle cx="8" cy="8" r="8" fill="currentColor" opacity="0.15" />
              <path
                d="M4.5 8.5L7 11L11.5 5.5"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
            {feature}
          </li>
        ))}
      </ul>

      {/* CTA */}
      <div className="mt-8">
        {tier.ctaHref.startsWith("mailto:") ? (
          <a
            href={tier.ctaHref}
            data-testid={`tier-cta-${tier.id}`}
            className={ctaClasses}
          >
            {tier.cta}
          </a>
        ) : (
          <Link
            href={`/${locale}${tier.ctaHref}`}
            data-testid={`tier-cta-${tier.id}`}
            className={ctaClasses}
          >
            {tier.cta}
          </Link>
        )}
      </div>
    </article>
  );
}

export default async function PricingPage(props: {
  params: Promise<{ locale: Locale }>;
}): Promise<React.ReactElement> {
  const { locale } = await props.params;

  // Split the single-source-of-truth catalog by product axis. `group`
  // defaults to the cache ladder when omitted (see src/lib/pricing.ts).
  const cacheTiers = TIERS.filter((t) => (t.group ?? "cache") === "cache");
  const runnerTiers = TIERS.filter((t) => t.group === "runner");

  return (
    <PublicShell width="wide" data-testid="pricing-root">
      {/* Header */}
      <div className="text-center" data-testid="pricing-header">
        <h1 className="text-[28px] font-[560] tracking-[-0.022em] text-[var(--t1)]">
          Simple, transparent pricing
        </h1>
        <p className="mx-auto mt-4 max-w-[60ch] text-base text-[var(--t2)]">
          R2 zero-egress means you pay for storage, not bandwidth.
          <br />
          No surprise bills when your CI cache hits.
        </p>
      </div>

      {/* Cache & storage ladder */}
      <section aria-labelledby="cache-tiers-heading" data-testid="pricing-cache-section">
        <h2
          id="cache-tiers-heading"
          className="mt-16 text-center text-2xl font-[560] tracking-[-0.012em] text-[var(--t1)]"
          data-testid="pricing-cache-heading"
        >
          Cache &amp; storage
        </h2>
        <div
          className="mt-8 grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-5"
          data-testid="pricing-grid"
          role="list"
        >
          {cacheTiers.map((tier) => (
            <TierCard key={tier.id} tier={tier} locale={locale} />
          ))}
        </div>
      </section>

      {/* CI runners — a SEPARATE, self-serve product axis. */}
      <section
        aria-labelledby="runner-tiers-heading"
        data-testid="pricing-runner-section"
      >
        <h2
          id="runner-tiers-heading"
          className="mt-20 text-center text-2xl font-[560] tracking-[-0.012em] text-[var(--t1)]"
          data-testid="pricing-runner-heading"
        >
          CI runners
        </h2>
        <p
          className="mx-auto mt-3 max-w-[60ch] text-center text-[var(--t2)]"
          data-testid="pricing-runner-subtitle"
        >
          Ephemeral, cache-accelerated runners for your CI. Priced on
          concurrency and monthly vCPU-hours — a separate add-on from cache
          storage.
        </p>
        <div
          className="mt-8 grid grid-cols-1 gap-6 sm:grid-cols-2 lg:grid-cols-5"
          data-testid="pricing-runner-grid"
          role="list"
        >
          {runnerTiers.map((tier) => (
            <TierCard key={tier.id} tier={tier} locale={locale} />
          ))}
        </div>
      </section>

      {/* Footer note */}
      <p
        className="mt-12 text-center text-sm text-[var(--t3)]"
        data-testid="pricing-footer-note"
      >
        All paid tiers include R2 zero-egress bandwidth. Prices in USD. Billed
        monthly.{" "}
        <Link
          href={`/${locale}/legal/terms`}
          className="text-[var(--t2)] underline decoration-[var(--line-2)] underline-offset-2 hover:text-[var(--t1)]"
        >
          Terms of Service
        </Link>
        {" · "}
        <Link
          href={`/${locale}/privacy`}
          className="text-[var(--t2)] underline decoration-[var(--line-2)] underline-offset-2 hover:text-[var(--t1)]"
        >
          Privacy
        </Link>
      </p>
    </PublicShell>
  );
}
