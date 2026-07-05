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
 */

import * as React from "react";
import Link from "next/link";
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
  return (
    <article
      role="listitem"
      data-testid={`tier-card-${tier.id}`}
      aria-label={`${tier.name} tier`}
      className={[
        "relative flex flex-col rounded-xl border p-6",
        tier.highlight
          ? "border-indigo-500 ring-2 ring-indigo-500 bg-indigo-50"
          : "border-slate-200 bg-white",
      ].join(" ")}
    >
      {/* Most popular badge */}
      {tier.highlight && (
        <span
          className="absolute -top-3 left-1/2 -translate-x-1/2 rounded-full bg-indigo-600 px-3 py-1 text-xs font-semibold text-white"
          data-testid={`most-popular-badge-${tier.id}`}
        >
          Most popular
        </span>
      )}

      {/* Tier name */}
      <h3
        className="text-lg font-semibold text-slate-900"
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
            <span className="text-4xl font-bold text-slate-900">
              {tier.price}
            </span>
            <span className="text-sm text-slate-500">{tier.cadence}</span>
          </>
        ) : (
          <span className="text-2xl font-semibold text-slate-700">
            Talk to us
          </span>
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
            className="flex items-start gap-2 text-sm text-slate-700"
          >
            {/* Checkmark — inline SVG, no extra dependency */}
            <svg
              aria-hidden="true"
              className="mt-0.5 h-4 w-4 shrink-0 text-indigo-500"
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
            className={[
              "block w-full rounded-lg px-4 py-2.5 text-center text-sm font-semibold transition-colors",
              "border border-slate-300 bg-white text-slate-700 hover:bg-slate-50",
            ].join(" ")}
          >
            {tier.cta}
          </a>
        ) : (
          <Link
            href={`/${locale}${tier.ctaHref}`}
            data-testid={`tier-cta-${tier.id}`}
            className={[
              "block w-full rounded-lg px-4 py-2.5 text-center text-sm font-semibold transition-colors",
              tier.highlight
                ? "bg-indigo-600 text-white hover:bg-indigo-700"
                : "border border-slate-300 bg-white text-slate-700 hover:bg-slate-50",
            ].join(" ")}
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
    <main
      className="mx-auto max-w-7xl px-4 py-16 sm:px-6 lg:px-8"
      data-testid="pricing-root"
    >
      {/* Header */}
      <div className="text-center" data-testid="pricing-header">
        <h1 className="text-4xl font-bold tracking-tight text-slate-900">
          Simple, transparent pricing
        </h1>
        <p className="mt-4 text-lg text-slate-600">
          R2 zero-egress means you pay for storage, not bandwidth.
          <br />
          No surprise bills when your CI cache hits.
        </p>
      </div>

      {/* Cache & storage ladder */}
      <section aria-labelledby="cache-tiers-heading" data-testid="pricing-cache-section">
        <h2
          id="cache-tiers-heading"
          className="mt-16 text-center text-2xl font-bold tracking-tight text-slate-900"
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
          className="mt-20 text-center text-2xl font-bold tracking-tight text-slate-900"
          data-testid="pricing-runner-heading"
        >
          CI runners
        </h2>
        <p
          className="mx-auto mt-3 max-w-2xl text-center text-slate-600"
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
        className="mt-12 text-center text-sm text-slate-500"
        data-testid="pricing-footer-note"
      >
        All paid tiers include R2 zero-egress bandwidth. Prices in USD. Billed
        monthly.{" "}
        <Link
          href={`/${locale}/legal/terms`}
          className="underline hover:text-slate-700"
        >
          Terms of Service
        </Link>
        {" · "}
        <Link
          href={`/${locale}/privacy`}
          className="underline hover:text-slate-700"
        >
          Privacy
        </Link>
      </p>
    </main>
  );
}
