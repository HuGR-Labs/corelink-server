/**
 * Tests for security-header enforcement on the analytics worker
 * (pre-HN-launch hardening).
 *
 * Coverage:
 *   - withSecurityHeaders fills gaps without clobbering CORS / Content-Type.
 *   - SECURITY_HEADERS includes the six baseline headers.
 *   - CSP never includes `unsafe-inline` or `unsafe-eval`.
 *   - Permissions-Policy opts out of interest-cohort (FLoC).
 *   - HSTS includes preload + includeSubDomains + 2y max-age.
 */

import { describe, expect, it } from "vitest";
import { SECURITY_HEADERS, withSecurityHeaders } from "../src/security-headers";

describe("analytics-worker security headers", () => {
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

    it("withSecurityHeaders fills missing headers but preserves existing ones", () => {
        const orig = new Response("ok", {
            status: 200,
            headers: {
                "Content-Type": "application/json",
                "Access-Control-Allow-Origin": "https://app.example.com",
                // Pre-existing Referrer-Policy must NOT be overwritten.
                "Referrer-Policy": "no-referrer",
            },
        });
        const out = withSecurityHeaders(orig);
        expect(out.headers.get("Content-Type")).toBe("application/json");
        expect(out.headers.get("Access-Control-Allow-Origin")).toBe(
            "https://app.example.com",
        );
        expect(out.headers.get("Referrer-Policy")).toBe("no-referrer");
        // Newly added.
        expect(out.headers.get("X-Frame-Options")).toBe("DENY");
        expect(out.headers.get("X-Content-Type-Options")).toBe("nosniff");
        expect(out.headers.get("Strict-Transport-Security")).toContain("preload");
        expect(out.headers.get("Content-Security-Policy")).toContain("default-src 'none'");
    });

    it("withSecurityHeaders preserves response status and body", async () => {
        const orig = new Response("not found", { status: 404 });
        const out = withSecurityHeaders(orig);
        expect(out.status).toBe(404);
        expect(await out.text()).toBe("not found");
    });
});
