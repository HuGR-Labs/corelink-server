/**
 * PUBLIC BUYER FUNNEL — pricing CTA destination regression lock.
 *
 * The defect this file exists to prevent (live in production until
 * 2026-08-03): every "start free / upgrade" CTA on `/[locale]/pricing` sent a
 * brand-new prospect to the SIGN-IN screen.
 *
 *   GET /corelink/en/sign-up
 *     -> 307  location: /corelink/sign-in?redirect_url=%2Fcorelink%2Fen%2Fsign-up
 *
 * The page composed `/${locale}${tier.ctaHref}` for every non-mailto tier, but
 * the Clerk sign-up route is mounted at `app/sign-up/[[...sign-up]]` — OUTSIDE
 * `app/[locale]`. `/en/sign-up` therefore matches no route, `isPublicPath()`
 * classified it as protected, and the middleware bounced the prospect to
 * sign-IN with a `redirect_url` pointing back at the same dead path. 5 of the
 * 6 cache tiers, in all 4 locales.
 *
 * WHY THE EXISTING TESTS DID NOT CATCH IT — and what this file does instead:
 *
 *   src/lib/pricing.test.ts asserted `card.ctaHref.startsWith("/")` — a
 *   CONSTANT-level assertion on an intermediate value. `/sign-up` passes it
 *   while the URL actually rendered (`/en/sign-up`) is dead. A render smoke
 *   test is no better: `/en/pricing` returned 200 the entire time it was
 *   broken, because the page rendered perfectly — only the DESTINATION was
 *   wrong.
 *
 *   So these tests render the real page, read the `href` attribute off the
 *   real anchor a user clicks, and then interrogate the DESTINATION through
 *   two independent oracles:
 *
 *     1. The filesystem router — does a Next route actually exist at that
 *        path? (the 404 oracle)
 *     2. `isPublicPath` / `isSelfGatedPath` — the exact predicates
 *        `src/middleware.ts` branches on for a signed-OUT visitor. (the
 *        "bounced to sign-in" oracle)
 *
 *   Both fail on the reverted code, independently.
 */

import * as React from "react";
import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { existsSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import PricingPage from "@/app/[locale]/pricing/page";
import { TIERS } from "@/lib/pricing";
import {
  APP_BASE_PATH,
  isPublicPath,
  isSelfGatedPath,
} from "@/lib/route-matcher";
import type { Locale } from "@/i18n/LocaleContext";

// The four canonical locales (src/i18n/request.ts LOCALES). The pricing page
// is localised, so a broken href is broken four times over — every locale is
// exercised, not just `en`.
const LOCALES: readonly Locale[] = ["en", "pt", "es", "de"];

const SRC_APP = resolve(dirname(fileURLToPath(import.meta.url)), "../src/app");

// ---------------------------------------------------------------------------
// Oracle 1 — the filesystem router.
//
// Resolves an app-relative pathname against the App Router conventions the
// real router uses: literal segments, `[dynamic]`, `[...catchAll]`,
// `[[...optionalCatchAll]]`, and `(route groups)` which match without
// consuming a segment. Returns true only when a renderable leaf
// (`page.tsx` / `route.ts`) is reached.
// ---------------------------------------------------------------------------

function childDirs(dir: string): string[] {
  if (!existsSync(dir)) return [];
  return readdirSync(dir).filter((entry) =>
    statSync(join(dir, entry)).isDirectory(),
  );
}

function hasLeaf(dir: string): boolean {
  return ["page.tsx", "page.ts", "route.ts", "route.tsx"].some((leaf) =>
    existsSync(join(dir, leaf)),
  );
}

const isGroup = (n: string): boolean => n.startsWith("(") && n.endsWith(")");
const isOptionalCatchAll = (n: string): boolean => n.startsWith("[[...");
const isCatchAll = (n: string): boolean => n.startsWith("[...");
const isDynamic = (n: string): boolean =>
  n.startsWith("[") && !isCatchAll(n) && !isOptionalCatchAll(n);

function matchSegments(dir: string, segments: readonly string[]): boolean {
  // Route groups are transparent to the URL.
  for (const group of childDirs(dir).filter(isGroup)) {
    if (matchSegments(join(dir, group), segments)) return true;
  }

  if (segments.length === 0) {
    if (hasLeaf(dir)) return true;
    // An optional catch-all also matches ZERO remaining segments — this is
    // what makes `/sign-up` resolve to `sign-up/[[...sign-up]]/page.tsx`.
    return childDirs(dir)
      .filter(isOptionalCatchAll)
      .some((d) => hasLeaf(join(dir, d)));
  }

  const [head, ...rest] = segments as [string, ...string[]];

  const literal = join(dir, head);
  if (existsSync(literal) && matchSegments(literal, rest)) return true;

  for (const d of childDirs(dir).filter(isDynamic)) {
    if (matchSegments(join(dir, d), rest)) return true;
  }
  for (const d of childDirs(dir).filter((n) => isCatchAll(n) || isOptionalCatchAll(n))) {
    if (hasLeaf(join(dir, d))) return true; // consumes every remaining segment
  }
  return false;
}

/**
 * The pathname a click on `href` puts on the wire — what the middleware
 * receives as `req.nextUrl.pathname` (never the query).
 */
function pathnameOf(href: string): string {
  return href.split("?")[0]!.split("#")[0]!;
}

/** True when a Next route renders at this app-relative pathname. */
function routeExists(href: string): boolean {
  const pathname = pathnameOf(href);
  const stripped = pathname.startsWith(APP_BASE_PATH)
    ? pathname.slice(APP_BASE_PATH.length) || "/"
    : pathname;
  return matchSegments(SRC_APP, stripped.split("/").filter(Boolean));
}

// Self-check: the oracle must agree with the routes we can see by eye,
// otherwise a green suite below would prove nothing.
describe("route oracle self-check", () => {
  it("resolves the routes that exist", () => {
    expect(routeExists("/sign-up")).toBe(true); // sign-up/[[...sign-up]]
    expect(routeExists("/sign-in")).toBe(true);
    expect(routeExists("/en/pricing")).toBe(true); // [locale]/pricing
    expect(routeExists("/en/upgrade?plan=pro")).toBe(true);
    expect(routeExists("/en/welcome")).toBe(true); // [locale]/(authenticated)/welcome
    expect(routeExists("/corelink/sign-up")).toBe(true); // basePath tolerant
  });

  it("rejects the routes that do not — including the shipped defect", () => {
    // THE BUG. There is no `app/[locale]/sign-up`.
    expect(routeExists("/en/sign-up")).toBe(false);
    expect(routeExists("/pt/sign-up")).toBe(false);
    expect(routeExists("/es/sign-up")).toBe(false);
    expect(routeExists("/de/sign-up")).toBe(false);
    expect(routeExists("/en/sign-in")).toBe(false);
    expect(routeExists("/en/no-such-page")).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// The funnel itself — rendered page, real anchors, real hrefs.
// ---------------------------------------------------------------------------

/** Render `/[locale]/pricing` and return the href of each tier's CTA anchor. */
async function renderCtaHrefs(
  locale: Locale,
): Promise<Record<string, string>> {
  const el = await PricingPage({ params: Promise.resolve({ locale }) });
  render(el);
  const hrefs: Record<string, string> = {};
  for (const tier of TIERS) {
    const anchor = screen.getByTestId(`tier-cta-${tier.id}`);
    hrefs[tier.id] = anchor.getAttribute("href") ?? "";
  }
  return hrefs;
}

/** The 5 self-serve cache tiers whose CTA must reach the sign-up form. */
const SIGNUP_TIER_IDS = ["free", "solo", "starter", "pro", "max"] as const;

describe.each(LOCALES)("public pricing funnel — locale %s", (locale) => {
  it("every sign-up CTA a prospect clicks resolves to the real sign-up route", async () => {
    const hrefs = await renderCtaHrefs(locale);
    for (const id of SIGNUP_TIER_IDS) {
      // The rendered href — NOT `tier.ctaHref`. Before the fix this was
      // `/${locale}/sign-up`.
      expect(hrefs[id], `tier ${id} @ ${locale}`).toBe("/sign-up");
    }
  });

  it("every rendered CTA points at a route that exists", async () => {
    const hrefs = await renderCtaHrefs(locale);
    for (const [id, href] of Object.entries(hrefs)) {
      if (href.startsWith("mailto:") || href.startsWith("http")) continue;
      expect(routeExists(href), `tier ${id} @ ${locale} -> ${href}`).toBe(true);
    }
  });

  it("no rendered CTA is bounced to the sign-IN screen by the middleware", async () => {
    const hrefs = await renderCtaHrefs(locale);
    for (const [id, href] of Object.entries(hrefs)) {
      if (href.startsWith("mailto:") || href.startsWith("http")) continue;
      // Exactly the branch `src/middleware.ts` takes for a signed-out visitor,
      // fed exactly what it is fed in production (`req.nextUrl.pathname`):
      // public -> render; self-gated -> the page owns its own redirect (and
      // preserves ?plan=); anything else -> `auth.protect()` 307s to sign-in.
      const path = pathnameOf(href);
      const reachable = isPublicPath(path) || isSelfGatedPath(path);
      expect(reachable, `tier ${id} @ ${locale} -> ${href} 307s to /sign-in`).toBe(
        true,
      );
    }
  });

  it("no rendered CTA carries the basePath (next/link would double it)", async () => {
    const hrefs = await renderCtaHrefs(locale);
    for (const [id, href] of Object.entries(hrefs)) {
      if (href.startsWith("mailto:") || href.startsWith("http")) continue;
      expect(
        href.startsWith(`${APP_BASE_PATH}/`),
        `tier ${id} @ ${locale} -> ${href} would render ${APP_BASE_PATH}${href}`,
      ).toBe(false);
      // …and the URL the browser finally requests is the one prod serves.
      expect(routeExists(`${APP_BASE_PATH}${href}`)).toBe(true);
    }
  });

  it("the Enterprise tier is a mailto inquiry, not a broken route", async () => {
    const hrefs = await renderCtaHrefs(locale);
    // The 6th cache tier is deliberately NOT checkout-able: the tier-select
    // backend 422s `enterprise` toward the inquiry form.
    expect(hrefs["enterprise"]).toBe("mailto:gustavo@humangr.com");
  });

  it("every runner CTA keeps its locale segment and its ?plan= intent", async () => {
    const hrefs = await renderCtaHrefs(locale);
    for (const tier of TIERS.filter((t) => t.group === "runner")) {
      // `/upgrade` IS `[locale]`-mounted, so this one keeps the prefix —
      // proving the fix discriminates rather than dropping the locale wholesale.
      expect(hrefs[tier.id]).toBe(`/${locale}/upgrade?plan=${tier.id}`);
      expect(isSelfGatedPath(pathnameOf(hrefs[tier.id]!))).toBe(true);
    }
  });
});

describe("post-sign-up destination", () => {
  it("lands the new user on a route that exists", () => {
    // `ClerkSignUp` pins force/fallbackRedirectUrl to
    // `withAppBasePath("/en/welcome")` (asserted in tests/clerk-basepath.test.tsx);
    // this asserts that target is a real page and not a second dead end.
    expect(routeExists(`${APP_BASE_PATH}/en/welcome`)).toBe(true);
  });
});
