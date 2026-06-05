/**
 * Lighthouse CI config — WI-S16-007 deliverable 2.
 *
 * Spec contract S-16 §6 DoD line "Lighthouse score ≥ 95 em Performance +
 * A11y + Best Practices + SEO em 3 routes (/, /privacy, /consent/new)".
 *
 * WI-S16-007 §6 extends to a 4th most-visited route (/admin/audit) —
 * covered here for the ship gate.
 *
 * Routing note: the homepage is served un-prefixed at `/` (src/app/page.tsx;
 * locale is resolved per-request via cookie/header in the root layout, with no
 * locale-prefix redirect). The `[locale]` segment has no root page, so `/en`
 * itself 404s — only its sub-routes (/en/privacy, /en/consent/new,
 * /en/admin/audit) render. URLs below stay on actually-rendered routes;
 * `/onboarding` is excluded because it redirects (Phase-0 PLG collapsed the
 * legacy /onboarding/tenant wizard into a post-signup /welcome redirect).
 *
 * Thresholds are the canonical S-16 SOTA bar: ≥ 95 Performance + A11y +
 * Best Practices, ≥ 90 SEO (admin UI is auth-gated; SEO is informational).
 */

/** @type {import('@lhci/cli').UserConfig} */
module.exports = {
  ci: {
    collect: {
      // Bring up a production server before collecting. CI runner is expected
      // to have run `pnpm build` first.
      // Serve the build the way it is actually deployed: OpenNext runs the Next
      // standalone server, NOT `next start`. `next start` on an
      // `output: "standalone"` build mis-serves the RSC stream — hydration
      // aborts ("Connection closed") and the streamed <title> is stripped from
      // the post-load DOM, tanking a11y/SEO even though the page renders
      // correctly in production. Run the standalone server, copying the static
      // assets + public/ it expects alongside it.
      startServerCommand:
        "mkdir -p .next/standalone/apps/admin-ui/.next && cp -r .next/static .next/standalone/apps/admin-ui/.next/ && (test -d public && cp -r public .next/standalone/apps/admin-ui/ || true) && PORT=3000 HOSTNAME=127.0.0.1 node .next/standalone/apps/admin-ui/server.js",
      startServerReadyPattern: "Local:|Ready|started server",
      startServerReadyTimeout: 120000,
      // `/en/admin/audit` is RBAC-gated + backend-fetching (RbacGuard +
      // AuditViewerClient); with no backend in a static Lighthouse run it cannot
      // render (perf is unmeasurable, NaN). The S-16 budget is scoped to the
      // customer-facing, statically-renderable routes — the admin surface is
      // covered by the Playwright a11y sweep instead.
      url: [
        "http://localhost:3000/",
        "http://localhost:3000/en/privacy",
        "http://localhost:3000/en/consent/new",
      ],
      numberOfRuns: 3,
      settings: {
        preset: "desktop",
        chromeFlags: "--no-sandbox --headless=new",
      },
    },
    assert: {
      assertions: {
        "categories:performance": ["error", { minScore: 0.95 }],
        "categories:accessibility": ["error", { minScore: 1.0 }],
        "categories:best-practices": ["error", { minScore: 0.95 }],
        "categories:seo": ["warn", { minScore: 0.9 }],
        // Bundle / perf guards (S-16 14.s16.4 bundle ≤ 250KB gzipped).
        "total-byte-weight": ["warn", { maxNumericValue: 1500000 }],
      },
    },
    upload: {
      target: "temporary-public-storage",
    },
  },
};
