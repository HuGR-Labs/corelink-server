/**
 * Public landing page — `/` (root, non-localized public entry).
 *
 * ROADMAP-TO-GA Phase 1 L1: "Landing explaining what CoreLink is + 30-sec
 * demo + signup CTA; first-visitor 'get it'." Copy ported from
 * marketing/launch/PILOT-LANDING-PAGE-COPY.md.
 *
 * PUBLIC — no Clerk import, no auth. `/` is exempt from auth middleware via
 * the route-matcher (src/lib/route-matcher.ts treats non-protected static
 * routes as public). Server component (matches /[locale]/pricing). Strings
 * live in the `landing` + `brand` i18n namespaces (src/i18n/locales/*.json).
 *
 * Locale-aware links: this route is NOT under the `[locale]` segment, so
 * locale-prefixed targets (pricing / legal / privacy) hardcode the default
 * locale `en` — the same convention the /sign-up route uses for
 * `/en/welcome` (default locale is "en", src/i18n/request.ts).
 */

import type { Metadata } from "next";
import Link from "next/link";
import { useTranslations } from "next-intl";

// Default locale for locale-prefixed public links (next-intl requires the
// prefix; "en" is DEFAULT_LOCALE in src/i18n/request.ts). Keep in sync if the
// default ever changes.
const L = "en";

export const metadata: Metadata = {
  title: "CoreLink — Shared content-addressable cache for builds, packages & ML",
  description:
    "CoreLink deduplicates and audits the blobs your CI, registries, and model store re-upload every day — across regions and teams — with cryptographic tenant isolation and an append-only Merkle audit chain.",
};

export default function HomePage(): React.ReactElement {
  return <LandingPage />;
}

function LandingPage(): React.ReactElement {
  const t = useTranslations("landing");
  const brand = useTranslations("brand");

  return (
    <div className="flex min-h-screen flex-col bg-white text-slate-900">
      {/* ---- Top bar -------------------------------------------------- */}
      <header
        className="mx-auto flex w-full max-w-6xl items-center justify-between px-4 py-5 sm:px-6 lg:px-8"
        data-testid="landing-header"
      >
        <span className="text-lg font-bold tracking-tight">
          {brand("name")}
        </span>
        <nav className="flex items-center gap-4 text-sm" aria-label="Primary">
          <Link
            href={`/${L}/pricing`}
            className="font-medium text-slate-600 hover:text-slate-900"
          >
            {t("nav.pricing")}
          </Link>
          {/* Secondary auth path — primary CTA is sign-up (below). */}
          <Link
            href="/sign-in"
            className="font-medium text-slate-600 hover:text-slate-900"
            data-testid="landing-signin-link"
          >
            {t("nav.signIn")}
          </Link>
        </nav>
      </header>

      <main id="main" className="flex-1">
        {/* ---- Hero --------------------------------------------------- */}
        <section
          className="mx-auto w-full max-w-4xl px-4 pt-16 pb-12 text-center sm:px-6 sm:pt-24 lg:px-8"
          data-testid="landing-hero"
        >
          <p className="text-sm font-semibold uppercase tracking-wide text-indigo-600">
            {brand("tagline")}
          </p>
          <h1 className="mt-4 text-4xl font-bold tracking-tight text-slate-900 sm:text-5xl">
            {t("hero.heading")}
          </h1>
          <p className="mx-auto mt-6 max-w-2xl text-lg leading-relaxed text-slate-600">
            {t("hero.subheading")}
          </p>
          <div className="mt-10 flex flex-col items-center justify-center gap-3 sm:flex-row">
            <Link
              href="/sign-up"
              className="inline-flex w-full items-center justify-center rounded-lg bg-indigo-600 px-6 py-3 text-base font-semibold text-white transition-colors hover:bg-indigo-700 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-indigo-600 focus-visible:ring-offset-2 sm:w-auto"
              data-testid="landing-signup-cta"
            >
              {t("hero.ctaPrimary")}
            </Link>
            <a
              href="#demo"
              className="inline-flex w-full items-center justify-center rounded-lg border border-slate-300 px-6 py-3 text-base font-semibold text-slate-700 transition-colors hover:bg-slate-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-slate-400 focus-visible:ring-offset-2 sm:w-auto"
              data-testid="landing-demo-cta"
            >
              {t("hero.ctaSecondary")}
            </a>
          </div>
          <p className="mt-4 text-sm text-slate-500">{t("hero.ctaSub")}</p>
        </section>

        {/* ---- Demo slot ---------------------------------------------- */}
        {/*
         * TODO(owner / CF-dashboard ops, not code): wire the 30-second demo.
         * The demo is recorded + hosted outside this repo (Cloudflare Stream
         * or asciinema). To go live, replace the dashed placeholder <div>
         * below with the embed — e.g. Cloudflare Stream:
         *
         *   <div className="mx-auto mt-8 aspect-video w-full max-w-3xl
         *                   overflow-hidden rounded-xl border border-slate-200">
         *     <iframe
         *       src="https://customer-<ACCT>.cloudflarestream.com/<VIDEO_ID>/iframe"
         *       loading="lazy" allowFullScreen
         *       className="h-full w-full" title="CoreLink 90-second demo" />
         *   </div>
         *
         * (Or keep the placeholder card and turn the button below into a link:
         *  swap the disabled <button> for `<a href="<STREAM_URL>" target="_blank"
         *  rel="noopener noreferrer" …>`.) No fabricated video is shipped — the
         * slot is a clearly-marked, wireable placeholder until then.
         */}
        <section
          id="demo"
          className="mx-auto w-full max-w-4xl scroll-mt-20 px-4 py-12 sm:px-6 lg:px-8"
          data-testid="landing-demo"
          aria-labelledby="demo-heading"
        >
          <h2
            id="demo-heading"
            className="text-center text-2xl font-bold tracking-tight text-slate-900"
          >
            {t("demo.title")}
          </h2>
          <div className="mx-auto mt-8 flex aspect-video w-full max-w-3xl flex-col items-center justify-center rounded-xl border-2 border-dashed border-slate-300 bg-slate-50 p-8 text-center">
            <p className="text-base font-medium text-slate-700">
              {t("demo.placeholder")}
            </p>
            <p className="mt-2 max-w-md text-sm text-slate-500">
              {t("demo.placeholderHint")}
            </p>
            {/*
             * Disabled affordance until the operator wires the embed/link
             * above — we never ship a dead link. See the TODO above the
             * section for how to point this at the Stream/asciinema URL.
             */}
            <button
              type="button"
              disabled
              aria-disabled="true"
              className="mt-6 inline-flex cursor-not-allowed items-center justify-center rounded-lg bg-slate-900 px-5 py-2.5 text-sm font-semibold text-white opacity-60"
              data-testid="landing-demo-button"
            >
              {t("demo.button")}
            </button>
          </div>
        </section>

        {/* ---- Feature blocks ----------------------------------------- */}
        <section
          className="mx-auto w-full max-w-6xl px-4 py-12 sm:px-6 lg:px-8"
          data-testid="landing-features"
          aria-label="Features"
        >
          <div className="grid grid-cols-1 gap-8 md:grid-cols-3">
            <FeatureBlock
              title={t("features.isolation.title")}
              body={t("features.isolation.body")}
              bullets={[
                t("features.isolation.b1"),
                t("features.isolation.b2"),
                t("features.isolation.b3"),
              ]}
              testid="feature-isolation"
            />
            <FeatureBlock
              title={t("features.audit.title")}
              body={t("features.audit.body")}
              bullets={[
                t("features.audit.b1"),
                t("features.audit.b2"),
                t("features.audit.b3"),
              ]}
              testid="feature-audit"
            />
            <FeatureBlock
              title={t("features.multiRegion.title")}
              body={t("features.multiRegion.body")}
              bullets={[
                t("features.multiRegion.b1"),
                t("features.multiRegion.b2"),
                t("features.multiRegion.b3"),
              ]}
              testid="feature-multiregion"
            />
          </div>
        </section>

        {/* ---- Closing CTA -------------------------------------------- */}
        <section className="mx-auto w-full max-w-4xl px-4 py-16 text-center sm:px-6 lg:px-8">
          <h2 className="text-2xl font-bold tracking-tight text-slate-900 sm:text-3xl">
            {t("closing.heading")}
          </h2>
          <div className="mt-8 flex flex-col items-center justify-center gap-3 sm:flex-row">
            <Link
              href="/sign-up"
              className="inline-flex w-full items-center justify-center rounded-lg bg-indigo-600 px-6 py-3 text-base font-semibold text-white transition-colors hover:bg-indigo-700 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-indigo-600 focus-visible:ring-offset-2 sm:w-auto"
              data-testid="landing-signup-cta-closing"
            >
              {t("hero.ctaPrimary")}
            </Link>
            <Link
              href={`/${L}/pricing`}
              className="inline-flex w-full items-center justify-center rounded-lg border border-slate-300 px-6 py-3 text-base font-semibold text-slate-700 transition-colors hover:bg-slate-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-slate-400 focus-visible:ring-offset-2 sm:w-auto"
            >
              {t("closing.ctaSecondary")}
            </Link>
          </div>
        </section>
      </main>

      {/* ---- Footer --------------------------------------------------- */}
      <footer
        className="border-t border-slate-200 bg-slate-50"
        data-testid="landing-footer"
      >
        <div className="mx-auto flex w-full max-w-6xl flex-col items-center justify-between gap-4 px-4 py-8 text-sm text-slate-500 sm:flex-row sm:px-6 lg:px-8">
          <p>{t("footer.tagline")}</p>
          <nav
            className="flex flex-wrap items-center justify-center gap-x-6 gap-y-2"
            aria-label="Footer"
          >
            <Link
              href={`/${L}/pricing`}
              className="hover:text-slate-900"
              data-testid="footer-pricing-link"
            >
              {t("footer.pricing")}
            </Link>
            <Link
              href={`/${L}/legal/terms`}
              className="hover:text-slate-900"
              data-testid="footer-terms-link"
            >
              {t("footer.terms")}
            </Link>
            <Link
              href={`/${L}/privacy`}
              className="hover:text-slate-900"
              data-testid="footer-privacy-link"
            >
              {t("footer.privacy")}
            </Link>
          </nav>
        </div>
      </footer>
    </div>
  );
}

/** A single value-prop column: headline, ~60-word body, supporting bullets. */
function FeatureBlock(props: {
  title: string;
  body: string;
  bullets: readonly string[];
  testid: string;
}): React.ReactElement {
  return (
    <article
      className="flex flex-col rounded-xl border border-slate-200 bg-white p-6"
      data-testid={props.testid}
    >
      <h3 className="text-lg font-semibold text-slate-900">{props.title}</h3>
      <p className="mt-3 text-sm leading-relaxed text-slate-600">{props.body}</p>
      <ul className="mt-4 space-y-2" aria-label={props.title}>
        {props.bullets.map((b) => (
          <li key={b} className="flex items-start gap-2 text-sm text-slate-700">
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
            {b}
          </li>
        ))}
      </ul>
    </article>
  );
}
