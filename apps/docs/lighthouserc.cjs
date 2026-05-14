/**
 * Lighthouse CI config — WI-S18-005 deliverable 5.
 *
 * Asserts Performance ≥ 95, Accessibility = 100, Best Practices ≥ 95,
 * SEO ≥ 90 across the 5 sprint-contract S-18 §5.6 R-S18-13 routes:
 *   /                       (homepage)
 *   /tutorial/              (5-min quickstart entry, WI-S18-002)
 *   /pricing/               (pricing page, WI-S18-004)
 *   /reference/reapi/       (auto-gen REAPI reference, WI-S18-002)
 *   /explanation/security/  (security/compliance page, WI-S18-004)
 *
 * CI runs nightly + on tag (`.github/workflows/docs-lighthouse.yml`). PR
 * preview gating happens in the Lote 10.18 P1 canonical workflow once the
 * CF Pages preview URL is wired in (out of scope for this WI; see
 * `WI-S18-001` deliverables for the CF Pages preview integration).
 */

/** @type {import('@lhci/cli').UserConfig} */
const BASE_URL = process.env.LH_BASE_URL ?? "http://localhost:3000";

module.exports = {
  ci: {
    collect: {
      startServerCommand: process.env.LH_START_COMMAND ?? "pnpm serve --no-open --port 3000",
      startServerReadyPattern: "Local:|Serving|started server on|ready",
      startServerReadyTimeout: 120000,
      url: [
        `${BASE_URL}/`,
        `${BASE_URL}/tutorial/`,
        `${BASE_URL}/pricing/`,
        `${BASE_URL}/reference/reapi/`,
        `${BASE_URL}/explanation/security/`,
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
        "categories:seo": ["error", { minScore: 0.9 }],
      },
    },
    upload: {
      target: "temporary-public-storage",
    },
  },
};
