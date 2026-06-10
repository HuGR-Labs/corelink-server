import { describe, it, expect } from "vitest";
import {
  isPublicPath,
  isProtectedPath,
  isSelfGatedPath,
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
});
