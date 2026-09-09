/**
 * WCAG 2.2 AA accessibility sweep — WI-S18-005 deliverable 3.
 *
 * Every public route is loaded; @axe-core/playwright runs with WCAG 2.2 AA
 * rule set; zero `serious` or `critical` violations allowed (per spec
 * contract S-18 §5.6 R-S18-14 + Quality Standard 14.s18.4 — `0 violations
 * sustained`).
 *
 * Manual screen-reader test (NVDA + VoiceOver + JAWS) is documented in
 * `specs/_audits/2026-05-14-s18-screen-reader-test.md` (deliverable 13
 * evidence pack); this spec is the automated gate.
 */

import { test, expect, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

// The docs are served under the canonical product path (Docusaurus
// `baseUrl: /corelink/docs/`), so every route is prefixed. Kept as a constant
// (overridable) so a future base-path change is a one-line edit.
const BASE_PATH = process.env.DOCS_BASE_PATH ?? "/corelink/docs";

const ROUTES = [
  "/",
  "/tutorial/",
  "/tutorial/getting-started/",
  "/how-to/",
  "/reference/",
  "/reference/reapi/",
  "/reference/cli/",
  "/reference/sdk/python/",
  "/reference/sdk/go/",
  "/reference/sdk/javascript/",
  "/explanation/",
  "/explanation/security/",
  "/compliance/",
  "/security/",
  "/pricing/",
].map((r) => (r === "/" ? `${BASE_PATH}/` : `${BASE_PATH}${r}`));

const FORBIDDEN_IMPACT = new Set(["serious", "critical"]);

async function scan(page: Page, route: string) {
  const response = await page.goto(route, { waitUntil: "networkidle" });
  // A missing route is an availability failure, not an accessibility pass.
  // Never skip a 4xx/5xx (or a navigation with no main response).
  if (!response || response.status() >= 400) {
    throw new Error(`route ${route} unavailable (status ${response?.status() ?? "no-response"})`);
  }

  const results = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"])
    .analyze();

  const blocking = results.violations.filter((v) =>
    FORBIDDEN_IMPACT.has(v.impact ?? ""),
  );

  if (blocking.length > 0) {
    const summary = blocking
      .map(
        (v) =>
          `  - [${v.impact}] ${v.id}: ${v.help} (${v.nodes.length} node(s))\n` +
          v.nodes
            .map(
              (n) =>
                `      target: ${JSON.stringify(n.target)}\n` +
                `      html:   ${n.html.replace(/\s+/g, " ").slice(0, 200)}\n` +
                [...n.any, ...n.all]
                  .map((c) => `      why:    ${c.message.replace(/\s+/g, " ").slice(0, 200)}`)
                  .join("\n"),
            )
            .join("\n") +
          `\n    ${v.helpUrl}`,
      )
      .join("\n");
    console.error(`a11y violations on ${route}:\n${summary}`);
  }

  expect(
    blocking,
    `serious/critical WCAG violations on ${route}`,
  ).toEqual([]);
}

for (const route of ROUTES) {
  test(`a11y: ${route}`, async ({ page }) => {
    await scan(page, route);
  });
}
