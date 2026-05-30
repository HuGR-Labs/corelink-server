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
  // CF Pages compat (@cloudflare/next-on-pages v1.13.7):
  //   - No `output: 'export'` or `output: 'standalone'` — both break next-on-pages.
  //   - Default server mode + nodejs_compat flag (declared in apps/admin-ui/wrangler.toml)
  //     enables Node.js APIs in the Pages Functions runtime.
  //   - Per-route `export const runtime = "edge"` (layout.tsx) + selective `"nodejs"`
  //     overrides (sign-in, sign-up) are the correct pattern for next-on-pages builds.
  //   - `pnpm pages:build` runs `next build && npx @cloudflare/next-on-pages`; output
  //     lands in `.vercel/output/static` (consumed by `pnpm pages:deploy`).
  //   - `outputFileTracingRoot`: points Next.js at the pnpm monorepo root so
  //     file-tracing resolves shared packages correctly.  Without this, Next.js
  //     warns "inferred workspace root may not be correct" and may miss
  //     transitive deps when building in a pnpm workspace or git worktree.
  //
  // 2026-05-29 fix: with `outputFileTracingRoot: ../../`, `npx
  // @cloudflare/next-on-pages` constructs Vercel build paths by joining
  // the workspace-relative path AGAIN with the build cwd, producing
  // `apps/admin-ui/apps/admin-ui/.next/routes-manifest.json` and an
  // ENOENT failure. Anchor to the app dir so paths are single-prefixed.
  outputFileTracingRoot: __dirname,
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
