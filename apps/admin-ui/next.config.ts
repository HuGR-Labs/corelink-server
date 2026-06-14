import type { NextConfig } from "next";
import path from "path";
import { withSentryConfig } from "@sentry/nextjs";
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
  // This app imports `next/image` zero times. Disabling the optimizer removes
  // the on-the-fly `/_next/image` endpoint as a CPU/cost amplification surface
  // on the Worker (it would otherwise re-encode per unique url+w+q tuple). The
  // CF rate-limit rule meters /_next/image, but turning it off entirely is the
  // belt-and-suspenders fix. Re-enable (drop this) if `next/image` is adopted.
  images: { unoptimized: true },
  // OpenNext (@opennextjs/cloudflare) compat:
  //   - `output: "standalone"` is REQUIRED — OpenNext expects
  //     `.next/standalone/apps/admin-ui/.next/server/pages-manifest.json`.
  //     Without it the build throws ENOENT on that path.
  //   - `outputFileTracingRoot` must be the monorepo root (../../) so that
  //     OpenNext resolves the monorepo-relative `.next/standalone/apps/admin-ui/`
  //     path correctly. Using __dirname (the next-on-pages workaround) is wrong
  //     for OpenNext and produces a double-prefix path.
  //   - All `export const runtime = "edge"` declarations are stripped from page/
  //     layout/api files — OpenNext requires edge functions to be defined in
  //     separate functions; it cannot process pages tagged as edge runtime.
  //   - Build: `pnpm cf:build` runs `opennextjs-cloudflare build`; output
  //     lands in `.open-next/` (worker.js + assets/).
  output: "standalone",
  outputFileTracingRoot: path.join(__dirname, "../../"),
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

/**
 * Sentry wrapper — automatically uploads source maps + tunnels client beacons
 * through `/monitoring` so ad-blockers don't strip them. The wrapper is a
 * no-op when neither `SENTRY_AUTH_TOKEN` nor `NEXT_PUBLIC_SENTRY_DSN` are
 * configured, so local builds and CI without Sentry credentials remain
 * unaffected. Gustavo wires the auth token + DSN per the Sentry setup
 * runbook (`specs/_audits/2026-05-27-sentry-setup-runbook.md`).
 *
 *   - `silent: true`  — suppress Sentry CLI chatter during build.
 *   - `widenClientFileUpload: true` — also map server bundles for SSR errors.
 *   - `tunnelRoute: "/monitoring"`  — proxy ingest through a same-origin path
 *                                     so privacy plugins / corporate proxies
 *                                     do not drop beacons. Documented in the
 *                                     admin-ui README.
 *   - `disableLogger: true` — strip `Sentry.logger.*` calls in production for
 *                              smaller client bundles.
 *   - `automaticVercelMonitors: false` — we deploy on Cloudflare Pages, not
 *                                         Vercel; this would create dead
 *                                         monitor sources.
 */
const sentryBuildOptions = {
  org: process.env.SENTRY_ORG ?? "corelink",
  project: process.env.SENTRY_PROJECT ?? "corelink-admin-ui",
  silent: true,
  widenClientFileUpload: true,
  tunnelRoute: "/monitoring" as const,
  disableLogger: true,
  automaticVercelMonitors: false,
};

export default withSentryConfig(withNextIntl(nextConfig), sentryBuildOptions);
