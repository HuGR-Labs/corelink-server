import type { NextConfig } from "next";
import createNextIntlPlugin from "next-intl/plugin";
import { buildCspHeaderValue, STATIC_SECURITY_HEADERS } from "./src/lib/csp";

const withNextIntl = createNextIntlPlugin("./src/i18n/request.ts");

/**
 * Static-config CSP fallback (matched by middleware which injects the real
 * per-request nonce). The placeholder NONCE here is replaced at runtime; we
 * still emit a static header so paths bypassing middleware (e.g. truly static
 * exports) still carry hardened headers.
 */
const STATIC_CSP = buildCspHeaderValue("STATIC");

/**
 * WI-S16-007 deliverable 4 — CSP report-only → enforce rollout flag.
 *
 *   CSP_ENFORCEMENT=report-only  → emits `Content-Security-Policy-Report-Only`
 *                                  (default during the staging observation
 *                                  window; spec contract S-16 §15 row 11).
 *   CSP_ENFORCEMENT=enforce      → emits `Content-Security-Policy` (production
 *                                  default after 30d report-only baseline
 *                                  has converged < 5 violations/dia).
 *
 * Documented in `apps/admin-ui/README.md` under "CSP rollout".
 */
function resolveCspMode(): "report-only" | "enforce" {
  const raw = (process.env["CSP_ENFORCEMENT"] ?? "").toLowerCase();
  if (raw === "enforce") return "enforce";
  if (raw === "report-only") return "report-only";
  // Default: enforce in production, report-only elsewhere.
  return process.env["NODE_ENV"] === "production" ? "enforce" : "report-only";
}

const CSP_HEADER_KEY =
  resolveCspMode() === "enforce"
    ? "Content-Security-Policy"
    : "Content-Security-Policy-Report-Only";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  poweredByHeader: false,
  // We use middleware for the real per-request CSP nonce.
  async headers() {
    return [
      {
        source: "/:path*",
        headers: [
          // Conservative static CSP — middleware overrides per-request with nonce.
          { key: CSP_HEADER_KEY, value: STATIC_CSP },
          ...STATIC_SECURITY_HEADERS.map((h) => ({ key: h.name, value: h.value })),
        ],
      },
    ];
  },
};

export default withNextIntl(nextConfig);
