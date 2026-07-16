import { describe, it, expect } from "vitest";
import {
  isPublicPath,
  isProtectedPath,
  isSelfGatedPath,
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

  // ── basePath awareness (THE prod bug) ──────────────────────────────────────
  // OpenNext invokes the middleware with `req.nextUrl.pathname` STILL carrying
  // the `/corelink` basePath. Public paths must be public in that prod shape,
  // not only the basePath-less dev shape.
  describe("basePath-prefixed forms (prod / OpenNext shape)", () => {
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

  // The app is served on BOTH surfaces during the subdomain→path migration, so
  // the sign-in redirect must re-attach the SAME prefix the request carried:
  // `/corelink/sign-in` for a `/corelink/*` request (a bare `/sign-in` on
  // humangr.com resolves to the apex marketing site), `/sign-in` for a root
  // request (a `/corelink/sign-in` on the subdomain 404s).
  describe("sign-in redirect target is surface-correct", () => {
    it("exposes /corelink as the mount prefix", () => {
      expect(APP_BASE_PATH).toBe("/corelink");
    });

    it("re-attaches /corelink for path-surface requests", () => {
      expect(signInPathFor("/corelink/en/welcome")).toBe("/corelink/sign-in");
      expect(signInPathFor("/corelink")).toBe("/corelink/sign-in");
      expect(signInRedirectPath("/corelink/en/welcome", "/corelink/en/welcome")).toBe(
        "/corelink/sign-in?redirect_url=%2Fcorelink%2Fen%2Fwelcome",
      );
    });

    it("stays root-relative for root-surface (subdomain) requests", () => {
      expect(signInPathFor("/en/welcome")).toBe("/sign-in");
      expect(signInPathFor("/admin")).toBe("/sign-in");
      expect(signInRedirectPath("/en/welcome", "/en/welcome")).toBe(
        "/sign-in?redirect_url=%2Fen%2Fwelcome",
      );
    });

    it("omits the return URL when none is given", () => {
      expect(signInRedirectPath("/corelink/en/welcome")).toBe("/corelink/sign-in");
      expect(signInRedirectPath("/en/welcome")).toBe("/sign-in");
    });
  });
});
