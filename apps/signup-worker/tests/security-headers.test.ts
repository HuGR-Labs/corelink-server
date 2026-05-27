/**
 * Tests for security-header enforcement on the signup worker
 * (pre-HN-launch hardening).
 */

import { describe, it, expect } from "vitest";
import { SECURITY_HEADERS, withSecurityHeaders } from "../src/security-headers.js";

describe("signup-worker security headers", () => {
  it("exposes the six baseline header names", () => {
    const names = Object.keys(SECURITY_HEADERS);
    expect(names).toContain("Content-Security-Policy");
    expect(names).toContain("Strict-Transport-Security");
    expect(names).toContain("X-Content-Type-Options");
    expect(names).toContain("X-Frame-Options");
    expect(names).toContain("Referrer-Policy");
    expect(names).toContain("Permissions-Policy");
  });

  it("HSTS is preload-eligible (≥2y + includeSubDomains + preload)", () => {
    const v = SECURITY_HEADERS["Strict-Transport-Security"];
    expect(v).toContain("max-age=63072000");
    expect(v).toContain("includeSubDomains");
    expect(v).toContain("preload");
  });

  it("CSP never permits unsafe-inline or unsafe-eval", () => {
    const v = SECURITY_HEADERS["Content-Security-Policy"];
    expect(v).not.toContain("unsafe-inline");
    expect(v).not.toContain("unsafe-eval");
    expect(v).toContain("default-src 'none'");
    expect(v).toContain("frame-ancestors 'none'");
  });

  it("Permissions-Policy opts out of FLoC interest-cohort", () => {
    expect(SECURITY_HEADERS["Permissions-Policy"]).toContain("interest-cohort=()");
  });

  it("withSecurityHeaders fills gaps without overwriting existing headers", () => {
    const orig = new Response("{}", {
      status: 200,
      headers: {
        "Content-Type": "application/json",
        "Referrer-Policy": "strict-origin",
      },
    });
    const out = withSecurityHeaders(orig);
    expect(out.headers.get("Content-Type")).toBe("application/json");
    expect(out.headers.get("Referrer-Policy")).toBe("strict-origin");
    expect(out.headers.get("X-Frame-Options")).toBe("DENY");
    expect(out.headers.get("X-Content-Type-Options")).toBe("nosniff");
  });

  it("withSecurityHeaders preserves status and body", async () => {
    const orig = new Response("not_found", { status: 404 });
    const out = withSecurityHeaders(orig);
    expect(out.status).toBe(404);
    expect(await out.text()).toBe("not_found");
  });
});
