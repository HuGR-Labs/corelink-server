import { describe, it, expect } from "vitest";
import {
  buildCspDirectives,
  buildCspHeader,
  buildCspHeaderValue,
  containsUnsafeDirective,
  containsScriptUnsafeInline,
  generateNonce,
  STATIC_SECURITY_HEADERS,
} from "@/lib/csp";

describe("CSP header generation", () => {
  it("nonce-hardens script-src; style-src uses 'unsafe-inline' WITHOUT a nonce", () => {
    const nonce = "test-nonce-12345";
    const directives = buildCspDirectives(nonce);
    const script = directives.find((d) => d.startsWith("script-src"));
    const style = directives.find((d) => d.startsWith("style-src"));
    expect(script).toContain(`'nonce-${nonce}'`);
    // style-src MUST NOT carry a nonce: per CSP3 a nonce disables 'unsafe-inline',
    // which blocks Clerk's runtime-injected inline widget styles (prod sign-in
    // rendered unstyled). 'unsafe-inline' is the intended style policy.
    expect(style).toContain("'unsafe-inline'");
    expect(style).not.toContain("nonce-");
  });

  it("never emits unsafe-eval or unsafe-hashes", () => {
    const value = buildCspHeaderValue("any-nonce");
    // containsUnsafeDirective checks 'unsafe-eval' + unsafe-hashes (never acceptable).
    // 'unsafe-inline' is intentionally present on style-src (Tailwind + Clerk);
    // it is validated absent from script-src in the separate test below.
    expect(containsUnsafeDirective(value)).toBe(false);
    expect(value).not.toContain("'unsafe-eval'");
  });

  it("'unsafe-inline' absent from script-src (only allowed on style-src)", () => {
    const value = buildCspHeaderValue("any-nonce");
    expect(containsScriptUnsafeInline(value)).toBe(false);
  });

  it("emits frame-ancestors 'none' and form-action 'self'", () => {
    const value = buildCspHeaderValue("n");
    expect(value).toContain("frame-ancestors 'none'");
    expect(value).toContain("form-action 'self'");
    expect(value).toContain("base-uri 'self'");
  });

  it("emits report-uri /api/csp-report", () => {
    expect(buildCspHeaderValue("n")).toContain("report-uri /api/csp-report");
  });

  it("allows clerk.corelink-app.humangr.com for script + connect", () => {
    const value = buildCspHeaderValue("n");
    expect(value).toContain("https://clerk.corelink-app.humangr.com");
    expect(value).toContain("https://corelink-api.humangr.com");
  });

  it("allows Plausible Analytics on script-src + connect-src (pre-HN-launch)", () => {
    const directives = buildCspDirectives("n");
    const script = directives.find((d) => d.startsWith("script-src")) ?? "";
    const connect = directives.find((d) => d.startsWith("connect-src")) ?? "";
    expect(script).toContain("https://plausible.io");
    expect(connect).toContain("https://plausible.io");
  });

  it("allows Stripe Checkout / portal endpoints for script + connect + frame", () => {
    // Source: https://docs.stripe.com/security/guide#content-security-policy
    const directives = buildCspDirectives("n");
    const script = directives.find((d) => d.startsWith("script-src")) ?? "";
    const connect = directives.find((d) => d.startsWith("connect-src")) ?? "";
    const frame = directives.find((d) => d.startsWith("frame-src")) ?? "";
    expect(script).toContain("https://js.stripe.com");
    expect(connect).toContain("https://api.stripe.com");
    expect(connect).toContain("https://m.stripe.network");
    // Added in csp-allowlist-validation audit (2026-05-27):
    expect(connect).toContain("https://checkout.stripe.com");
    expect(connect).toContain("https://billing.stripe.com");
    expect(frame).toContain("https://js.stripe.com");
    expect(frame).toContain("https://hooks.stripe.com");
  });

  it("allows Clerk frame-src for modal/popup auth steps", () => {
    // Source: https://clerk.com/docs/security/content-security-policy
    const directives = buildCspDirectives("n");
    const frame = directives.find((d) => d.startsWith("frame-src")) ?? "";
    expect(frame).toContain("https://clerk.corelink-app.humangr.com");
  });

  it("allows Clerk Smart-CAPTCHA / Turnstile: script-src + frame-src challenges, worker-src blob:", () => {
    // Clerk bot-protection (Cloudflare Turnstile) injects a script from
    // challenges.cloudflare.com at runtime, renders it in a frame, AND spawns a
    // Web Worker from a blob: URL. Missing any of these breaks sign-in/up under
    // enforce-mode CSP. Source: https://clerk.com/docs/security/content-security-policy
    const directives = buildCspDirectives("n");
    const script = directives.find((d) => d.startsWith("script-src")) ?? "";
    const frame = directives.find((d) => d.startsWith("frame-src")) ?? "";
    const worker = directives.find((d) => d.startsWith("worker-src")) ?? "";
    expect(script).toContain("https://challenges.cloudflare.com");
    expect(frame).toContain("https://challenges.cloudflare.com");
    expect(worker).toContain("'self'");
    expect(worker).toContain("blob:");
    // blob: workers must NOT be permitted on script-src (only worker-src).
    expect(script).not.toContain("blob:");
  });

  it("allows the first-party analytics sink + Clerk telemetry on connect-src", () => {
    const connect =
      buildCspDirectives("n").find((d) => d.startsWith("connect-src")) ?? "";
    // PLG event beacon (src/lib/analytics.ts default endpoint).
    expect(connect).toContain("https://corelink-analytics.humangr.com");
    // Clerk SDK telemetry (default-on; avoids enforce-mode violation noise).
    expect(connect).toContain("https://clerk-telemetry.com");
  });

  it("style-src includes 'unsafe-inline' (required by Tailwind + Clerk widgets)", () => {
    // 'unsafe-inline' is acceptable on style-src per OWASP guidance when nonce is
    // also present; required by Tailwind CSS JIT and Clerk modal overlay styles.
    // MUST NOT appear on script-src.
    const directives = buildCspDirectives("n");
    const style = directives.find((d) => d.startsWith("style-src")) ?? "";
    const script = directives.find((d) => d.startsWith("script-src")) ?? "";
    expect(style).toContain("'unsafe-inline'");
    expect(script).not.toContain("'unsafe-inline'");
  });

  it("connect-src is explicit per host — never permits wildcard", () => {
    const directives = buildCspDirectives("n");
    const connect = directives.find((d) => d.startsWith("connect-src")) ?? "";
    expect(connect).not.toContain(" *");
    expect(connect).not.toContain("https:*");
    // Wildcard token alone (not preceded by a domain char) would be ` * `.
    expect(/\s\*(\s|$)/.test(connect)).toBe(false);
  });

  it("toggles report-only header name based on opts", () => {
    const enforce = buildCspHeader({ nonce: "n", reportOnly: false });
    const reportOnly = buildCspHeader({ nonce: "n", reportOnly: true });
    expect(enforce.name).toBe("Content-Security-Policy");
    expect(reportOnly.name).toBe("Content-Security-Policy-Report-Only");
  });

  it("static security headers include HSTS + DENY + nosniff", () => {
    const names = STATIC_SECURITY_HEADERS.map((h) => h.name);
    expect(names).toContain("Strict-Transport-Security");
    expect(names).toContain("X-Frame-Options");
    expect(names).toContain("X-Content-Type-Options");
    expect(names).toContain("Referrer-Policy");
    const hsts = STATIC_SECURITY_HEADERS.find(
      (h) => h.name === "Strict-Transport-Security",
    );
    expect(hsts?.value).toContain("max-age=63072000");
    expect(hsts?.value).toContain("includeSubDomains");
    expect(hsts?.value).toContain("preload");
  });

  it("Permissions-Policy opts out of FLoC interest-cohort (pre-HN-launch)", () => {
    const pp = STATIC_SECURITY_HEADERS.find(
      (h) => h.name === "Permissions-Policy",
    );
    expect(pp?.value).toContain("interest-cohort=()");
  });
});

describe("nonce generation", () => {
  it("returns a unique value across many invocations", () => {
    const seen = new Set<string>();
    for (let i = 0; i < 1000; i++) {
      const n = generateNonce();
      expect(n.length).toBeGreaterThan(8);
      expect(seen.has(n)).toBe(false);
      seen.add(n);
    }
    expect(seen.size).toBe(1000);
  });

  it("emits URL-safe base64 (no padding)", () => {
    for (let i = 0; i < 20; i++) {
      const n = generateNonce();
      expect(n.endsWith("=")).toBe(false);
      expect(/^[A-Za-z0-9+/]+$/.test(n)).toBe(true);
    }
  });
});
