import { describe, it, expect } from "vitest";
import {
  buildCspDirectives,
  buildCspHeader,
  buildCspHeaderValue,
  containsUnsafeDirective,
  generateNonce,
  STATIC_SECURITY_HEADERS,
} from "@/lib/csp";

describe("CSP header generation", () => {
  it("includes the nonce in script-src and style-src", () => {
    const nonce = "test-nonce-12345";
    const directives = buildCspDirectives(nonce);
    const script = directives.find((d) => d.startsWith("script-src"));
    const style = directives.find((d) => d.startsWith("style-src"));
    expect(script).toContain(`'nonce-${nonce}'`);
    expect(style).toContain(`'nonce-${nonce}'`);
  });

  it("never emits unsafe-inline or unsafe-eval", () => {
    const value = buildCspHeaderValue("any-nonce");
    expect(containsUnsafeDirective(value)).toBe(false);
    expect(value).not.toContain("'unsafe-inline'");
    expect(value).not.toContain("'unsafe-eval'");
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

  it("allows clerk.corelink.humangr.com for script + connect", () => {
    const value = buildCspHeaderValue("n");
    expect(value).toContain("https://clerk.corelink.humangr.com");
    expect(value).toContain("https://api.corelink.humangr.com");
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
