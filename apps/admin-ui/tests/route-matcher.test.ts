import { describe, it, expect } from "vitest";
import {
  isPublicPath,
  isProtectedPath,
  isSelfGatedPath,
  isLocaleLessPath,
  canonicalAuthPathFor,
  signInPathFor,
  signInRedirectPath,
  APP_BASE_PATH,
  PUBLIC_PATH_PREFIXES,
} from "@/lib/route-matcher";

describe("route matcher", () => {
  it("treats canonical public paths as public", () => {
    expect(isPublicPath("/sign-in")).toBe(true);
    expect(isPublicPath("/sign-up")).toBe(true);
    expect(isPublicPath("/api/health")).toBe(true);
    expect(isPublicPath("/api/csp-report")).toBe(true);
    expect(isPublicPath("/api/newsletter/subscribe")).toBe(true);
    expect(isPublicPath("/_next/static/chunks/main.js")).toBe(true);
    expect(isPublicPath("/locales/en.json")).toBe(true);
    expect(isPublicPath("/llms.txt")).toBe(true);
    expect(isPublicPath("/pricing.txt")).toBe(true);
  });

  it("treats the public top-of-funnel pages as public", () => {
    // `/` is the public landing page (signup CTA target from docs).
    expect(isPublicPath("/")).toBe(true);
    // Locale-prefixed marketing/compliance pages.
    expect(isPublicPath("/en/pricing")).toBe(true);
    expect(isPublicPath("/pt/legal/terms")).toBe(true);
    expect(isPublicPath("/es/privacy/sub-processors")).toBe(true);
    expect(isPublicPath("/de/security/policy")).toBe(true);
    expect(isPublicPath("/en/403")).toBe(true);
    // The consent-CAPTURE leaf is public (pre-auth compliance page, Lighthouse-
    // audited) — but ONLY the leaf; the user-specific consent sub-routes stay
    // protected.
    expect(isPublicPath("/en/consent/new")).toBe(true);
    expect(isPublicPath("/consent/new")).toBe(true);
    expect(isProtectedPath("/en/consent/history")).toBe(true);
    expect(isProtectedPath("/en/consent/withdraw/abc123")).toBe(true);
    expect(isProtectedPath("/en/consent")).toBe(true);
    // Locale-less shapes stay public too.
    expect(isPublicPath("/pricing")).toBe(true);
  });

  it("treats tenant routes as protected", () => {
    expect(isProtectedPath("/dashboard")).toBe(true);
    expect(isProtectedPath("/settings/security")).toBe(true);
    expect(isProtectedPath("/admin/audit")).toBe(true);
    expect(isProtectedPath("/en/admin/audit")).toBe(true);
    expect(isProtectedPath("/en/customer/keys")).toBe(true);
    expect(isProtectedPath("/billing")).toBe(true);
    expect(isProtectedPath("/en/welcome")).toBe(true);
    // Prefix tricks don't leak protection: a page merely *starting* with a
    // public page name is still protected.
    expect(isProtectedPath("/en/pricingx")).toBe(true);
    expect(isProtectedPath("/en/securityx")).toBe(true);
  });

  it("marks /upgrade as self-gated (Clerk context, no middleware protect)", () => {
    expect(isSelfGatedPath("/upgrade")).toBe(true);
    expect(isSelfGatedPath("/en/upgrade")).toBe(true);
    expect(isSelfGatedPath("/en/upgraded")).toBe(false);
    expect(isSelfGatedPath("/en/customer")).toBe(false);
    // Self-gated routes are still NOT public — clerkMiddleware must run.
    expect(isPublicPath("/en/upgrade")).toBe(false);
  });

  it("public prefix list is non-empty and frozen-shape", () => {
    expect(PUBLIC_PATH_PREFIXES.length).toBeGreaterThan(0);
    expect(PUBLIC_PATH_PREFIXES).toContain("/sign-in");
  });

  // ── basePath awareness ─────────────────────────────────────────────────────
  // CORRECTION (2026-08-01): this block used to assert that "OpenNext invokes
  // the middleware with `req.nextUrl.pathname` STILL carrying the `/corelink`
  // basePath". That is FALSE, and believing it is what broke the sign-in
  // redirect — `requestBasePath` branched on the prefix being present, never
  // matched, and emitted a bare `/sign-in` pointing at the apex marketing site.
  //
  // Decisive evidence from prod (before the fix): the middleware's own 307
  // carried `?redirect_url=%2Fdashboard`. That value is built from
  // `req.nextUrl.pathname + req.nextUrl.search` (`middleware.ts`), so the
  // pathname the middleware received was `/dashboard` — basePath ALREADY
  // stripped, not `/corelink/dashboard`.
  //
  // These prefixed cases are still worth keeping as DEFENSIVE coverage (a
  // future rewrite or a direct call could hand in the prefixed shape, and the
  // matchers must not gate it), but they are NOT the production shape. The
  // production shape is the prefix-less block above.
  describe("basePath-prefixed forms (defensive — NOT the prod shape)", () => {
    it("treats /corelink-prefixed public paths as public", () => {
      expect(isPublicPath("/corelink/sign-in")).toBe(true);
      expect(isPublicPath("/corelink/sign-up")).toBe(true);
      expect(isPublicPath("/corelink/api/health")).toBe(true);
      expect(isPublicPath("/corelink/api/csp-report")).toBe(true);
      expect(isPublicPath("/corelink/api/newsletter/subscribe")).toBe(true);
      expect(isPublicPath("/corelink/_next/static/chunks/main.js")).toBe(true);
      expect(isPublicPath("/corelink/locales/en.json")).toBe(true);
      expect(isPublicPath("/corelink/llms.txt")).toBe(true);
      expect(isPublicPath("/corelink/pricing.txt")).toBe(true);
    });

    it("treats /corelink-prefixed marketing/compliance pages as public", () => {
      expect(isPublicPath("/corelink/en/consent/new")).toBe(true);
      expect(isProtectedPath("/corelink/en/consent/history")).toBe(true);
      expect(isPublicPath("/corelink/en/pricing")).toBe(true);
      expect(isPublicPath("/corelink/pt/legal/terms")).toBe(true);
      expect(isPublicPath("/corelink/es/privacy/sub-processors")).toBe(true);
      expect(isPublicPath("/corelink/de/security/policy")).toBe(true);
      expect(isPublicPath("/corelink/en/403")).toBe(true);
      expect(isPublicPath("/corelink/pricing")).toBe(true);
    });

    it("treats the bare basePath root as the public landing page", () => {
      expect(isPublicPath("/corelink")).toBe(true);
      expect(isPublicPath("/corelink/")).toBe(true);
    });

    it("tolerates a trailing slash and a defensively-doubled basePath", () => {
      expect(isPublicPath("/corelink/sign-up/")).toBe(true);
      expect(isPublicPath("/corelink/en/pricing/")).toBe(true);
      // Belt-and-suspenders: a future rewrite double-prefixing must not gate.
      expect(isPublicPath("/corelink/corelink/sign-up")).toBe(true);
    });

    it("keeps /corelink-prefixed tenant + checkout paths PROTECTED", () => {
      expect(isProtectedPath("/corelink/en/welcome")).toBe(true);
      expect(isProtectedPath("/corelink/en/customer/keys")).toBe(true);
      expect(isProtectedPath("/corelink/admin/audit")).toBe(true);
      expect(isProtectedPath("/corelink/settings/security")).toBe(true);
      // Checkout REQUIRES auth — must stay protected under basePath too.
      expect(isProtectedPath("/corelink/api/checkout/session")).toBe(true);
      // Prefix tricks don't leak protection under basePath either.
      expect(isProtectedPath("/corelink/en/pricingx")).toBe(true);
      expect(isProtectedPath("/corelink/en/securityx")).toBe(true);
    });

    it("marks /corelink-prefixed /upgrade as self-gated (not public)", () => {
      expect(isSelfGatedPath("/corelink/upgrade")).toBe(true);
      expect(isSelfGatedPath("/corelink/en/upgrade")).toBe(true);
      expect(isPublicPath("/corelink/en/upgrade")).toBe(false);
    });
  });

  // The app is served on exactly ONE surface: `humangr.com/corelink/*`. The
  // subdomain surface this block was originally written for — where a bare
  // `/sign-in` was the correct output — is RETIRED (`corelink-app` and
  // `corelink-admin` both had their `custom_domain` bindings removed).
  //
  // So the sign-in redirect must ALWAYS re-attach `/corelink`. A bare
  // `/sign-in` on `humangr.com` resolves to the apex marketing site, which is
  // a different app — that was the live breakage this suite failed to catch.
  describe("sign-in redirect target is surface-correct", () => {
    it("exposes /corelink as the mount prefix", () => {
      expect(APP_BASE_PATH).toBe("/corelink");
    });

    // REGRESSION GUARD. Next strips `basePath` BEFORE middleware runs, so the
    // pathnames below are what the middleware ACTUALLY receives in production
    // for `humangr.com/corelink/*` — already prefix-less. The previous version
    // of this suite fed the prefixed form (an input production never produces)
    // and asserted the prefix-less OUTPUT as correct in a companion case, so it
    // passed green while every logged-out user was 307'd to the apex marketing
    // site instead of this app's sign-in.
    it("re-attaches /corelink to the basePath-stripped pathname the middleware sees", () => {
      expect(signInPathFor("/en/welcome")).toBe("/corelink/sign-in");
      expect(signInPathFor("/admin")).toBe("/corelink/sign-in");
      expect(signInPathFor("/dashboard")).toBe("/corelink/sign-in");
      expect(signInRedirectPath("/en/welcome", "/en/welcome")).toBe(
        "/corelink/sign-in?redirect_url=%2Fcorelink%2Fen%2Fwelcome",
      );
    });

    it("never emits a bare /sign-in — that path is the apex marketing site", () => {
      for (const p of ["/", "/dashboard", "/billing", "/en/dashboard", "/corelink/x", "/admin"]) {
        expect(signInPathFor(p).startsWith(`${APP_BASE_PATH}/`)).toBe(true);
        expect(signInRedirectPath(p, p).startsWith(`${APP_BASE_PATH}/sign-in`)).toBe(true);
      }
    });

    // The SECOND half of the same defect, found live after the sign-in PATH was
    // already fixed and deployed: the redirect TARGET was `/corelink/sign-in`
    // but the `redirect_url` VALUE was still bare, so the buyer signed in and
    // was then handed to the marketing site. Measured in prod 2026-08-01:
    //
    //   GET /corelink/dashboard -> 307 /corelink/sign-in?redirect_url=%2Fdashboard
    //   GET humangr.com/dashboard -> 200 "HuGR — CoreLink · …" (marketing)
    //
    // Clerk consumes `redirect_url` and navigates by plain assignment, so
    // nothing re-attaches the prefix downstream.
    it("the redirect_url VALUE carries the basePath — Clerk navigates it verbatim", () => {
      for (const p of ["/dashboard", "/en/welcome", "/billing", "/en/customer/keys"]) {
        const url = new URL(signInRedirectPath(p, p), "https://humangr.com");
        const back = url.searchParams.get("redirect_url");
        expect(back).toBe(`${APP_BASE_PATH}${p}`);
        // The exact shape that was live and broken.
        expect(back).not.toBe(p);
      }
    });

    it("preserves the query string on the return URL", () => {
      const url = new URL(
        signInRedirectPath("/en/upgrade", "/en/upgrade?plan=max"),
        "https://humangr.com",
      );
      expect(url.searchParams.get("redirect_url")).toBe("/corelink/en/upgrade?plan=max");
    });

    it("re-attachment is idempotent — an already-prefixed returnTo never doubles", () => {
      // `/[locale]/upgrade/page.tsx` builds its own `${APP_BASE_PATH}/…` value.
      // If that caller ever routes through here, it must not become
      // `/corelink/corelink/…` (which 404s).
      const url = new URL(
        signInRedirectPath("/en/upgrade", `${APP_BASE_PATH}/en/upgrade?plan=max`),
        "https://humangr.com",
      );
      expect(url.searchParams.get("redirect_url")).toBe("/corelink/en/upgrade?plan=max");
    });

    it("omits the return URL when none is given", () => {
      expect(signInRedirectPath("/en/welcome")).toBe("/corelink/sign-in");
      expect(signInRedirectPath("/dashboard")).toBe("/corelink/sign-in");
    });
  });
});

/**
 * The locale-prefixed auth URL trap — the public buyer funnel break of
 * 2026-08-03. `/en/sign-up` matches NO route (the Clerk auth pages are mounted
 * outside `app/[locale]`), so `isPublicPath` calls it protected and the
 * middleware 307s a brand-new prospect to the sign-IN screen carrying a
 * `redirect_url` that points back at the same dead path. The link builders now
 * emit the locale-less shape; this is the middleware-side net for every URL
 * already in the wild.
 */
describe("locale-less auth routes", () => {
  it("recognises the routes that have no locale-prefixed shape", () => {
    expect(isLocaleLessPath("/sign-up")).toBe(true);
    expect(isLocaleLessPath("/sign-in")).toBe(true);
    expect(isLocaleLessPath("/sign-up/verify-email-address")).toBe(true);
    expect(isLocaleLessPath("/sign-in?redirect_url=%2Fcorelink%2Fen%2Fupgrade")).toBe(true);
    expect(isLocaleLessPath(`${APP_BASE_PATH}/sign-up`)).toBe(true);
  });

  it("leaves the locale-mounted routes alone", () => {
    expect(isLocaleLessPath("/upgrade?plan=pro")).toBe(false);
    expect(isLocaleLessPath("/en/pricing")).toBe(false);
    expect(isLocaleLessPath("/signup")).toBe(false); // no `/` boundary confusion
    expect(isLocaleLessPath("mailto:gustavo@humangr.com")).toBe(false);
  });

  it("canonicalizes every locale's dead auth URL onto the real route", () => {
    for (const locale of ["en", "pt", "es", "de"]) {
      expect(canonicalAuthPathFor(`/${locale}/sign-up`)).toBe(
        `${APP_BASE_PATH}/sign-up`,
      );
      expect(canonicalAuthPathFor(`/${locale}/sign-in`)).toBe(
        `${APP_BASE_PATH}/sign-in`,
      );
      // Clerk's own step routes under the widget path must survive.
      expect(canonicalAuthPathFor(`/${locale}/sign-up/verify-email-address`)).toBe(
        `${APP_BASE_PATH}/sign-up/verify-email-address`,
      );
    }
  });

  it("returns null for anything already canonical — no redirect loop", () => {
    expect(canonicalAuthPathFor("/sign-up")).toBeNull();
    expect(canonicalAuthPathFor("/sign-in")).toBeNull();
    expect(canonicalAuthPathFor(`${APP_BASE_PATH}/sign-up`)).toBeNull();
    expect(canonicalAuthPathFor("/en/pricing")).toBeNull();
    expect(canonicalAuthPathFor("/en/upgrade")).toBeNull();
    expect(canonicalAuthPathFor("/dashboard")).toBeNull();
    expect(canonicalAuthPathFor("/")).toBeNull();
  });

  it("the canonical target it emits is itself PUBLIC (else the net feeds the bounce)", () => {
    const target = canonicalAuthPathFor("/pt/sign-up");
    expect(target).not.toBeNull();
    expect(isPublicPath(target as string)).toBe(true);
  });
});
