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
          { key: "Content-Security-Policy-Report-Only", value: STATIC_CSP },
          ...STATIC_SECURITY_HEADERS.map((h) => ({ key: h.name, value: h.value })),
        ],
      },
    ];
  },
};

export default withNextIntl(nextConfig);
