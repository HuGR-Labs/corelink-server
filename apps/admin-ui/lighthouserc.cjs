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
      startServerCommand: "pnpm start",
      startServerReadyPattern: "Ready in|started server on|Local:",
      startServerReadyTimeout: 120000,
      url: [
        "http://localhost:3000/",
        "http://localhost:3000/en/privacy",
        "http://localhost:3000/en/consent/new",
        "http://localhost:3000/en/admin/audit",
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
