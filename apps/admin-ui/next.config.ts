import type { NextConfig } from "next";
import path from "path";
import { withSentryConfig } from "@sentry/nextjs";
import createNextIntlPlugin from "next-intl/plugin";
import { securityHeaderRoutes } from "./src/lib/csp";

const withNextIntl = createNextIntlPlugin("./src/i18n/request.ts");

const nextConfig: NextConfig = {
  reactStrictMode: true,
  poweredByHeader: false,
  // Canonical app surface is `humangr.com/corelink` (the retired
  // `corelink-app.humangr.com` subdomain served the app at `/`). Without a
  // basePath, OpenNext serves every route at `/` and the path-mounted surface
  // 404s. Next auto-prefixes framework links, the Sentry `tunnelRoute`
  // (`/monitoring` → `/corelink/monitoring`), and the static `headers()` source
  // with this basePath; hand-built URLs (middleware sign-in redirects, the
  // `/upgrade` forwarder, and the checkout success/cancel URLs handed to Stripe)
  // re-attach it explicitly via `APP_BASE_PATH` in `src/lib/route-matcher.ts`.
  basePath: "/corelink",
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
  // CSP is emitted ONLY by middleware, with the real per-request nonce as the
  // single source of truth. We deliberately do NOT emit a static
  // Content-Security-Policy here: a static header carries a fixed placeholder
  // nonce (defeating the per-request nonce — any attacker could reuse it) and,
  // because Next merges headers() on top of middleware, it would OVERRIDE the
  // real per-request CSP.
  //
  // The nonce-free static security headers are emitted here for EXACTLY the
  // paths the middleware matcher skips (`/_next/static`, `/_next/image`,
  // favicon, robots, sitemap) and nowhere else — the middleware `set()` loop
  // owns every other response. This partition is the fix for the headers being
  // emitted twice on the render path (a `"/:path*"` source here overlapped the
  // matcher, and OpenNext merged the two case-divergent copies into one folded
  // line). The reasoning, the live evidence, and why neither emitter could be
  // deleted outright live on `SECURITY_HEADER_ROUTE_SOURCES` in src/lib/csp.ts;
  // `tests/security-headers-single-emitter.test.ts` fails if the partition is
  // broken from either side.
  async headers() {
    return securityHeaderRoutes();
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
  // Sentry v10: `disableLogger` and top-level `automaticVercelMonitors` were
  // deprecated in favour of the nested `webpack.*` options (they only apply to
  // the Webpack build path — under Next 16's default Turbopack build they are
  // no-ops, but we keep them for the `--webpack` fallback).
  webpack: {
    treeshake: { removeDebugLogging: true }, // was: disableLogger: true
    automaticVercelMonitors: false,
  },
};

export default withSentryConfig(withNextIntl(nextConfig), sentryBuildOptions);
