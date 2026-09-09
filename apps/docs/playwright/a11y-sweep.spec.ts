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

import { Buffer } from "node:buffer";
import { readFileSync } from "node:fs";
import { join } from "node:path";

import { test, expect, type Page, type TestInfo } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

// Read the shared JSON directly: Playwright 1.61's Node loader cannot resolve
// local TypeScript imports on every supported Node 22 patch release.
const ROUTE_PATHS: string[] = JSON.parse(
  readFileSync(join(__dirname, "../scripts/a11y-routes.json"), "utf8"),
);
const BASE_PATH = process.env.DOCS_BASE_PATH ?? "/corelink/docs";
const A11Y_ROUTES = ROUTE_PATHS.map((route) =>
  route === "/" ? `${BASE_PATH}/` : `${BASE_PATH}${route}`,
);

const FORBIDDEN_IMPACT = new Set(["serious", "critical"]);

async function scan(page: Page, route: string, testInfo: TestInfo) {
  const response = await page.goto(route, { waitUntil: "networkidle" });
  const audit = {
    route,
    url: new URL(route, process.env.DOCS_BASE_URL ?? "http://localhost:3000")
      .href,
    violations: [] as Array<{
      id: string;
      impact: string | null;
      help: string;
      helpUrl: string;
      nodes: number;
      tags: string[];
    }>,
    ...(response ? {} : { error: "navigation returned no main response" }),
  };

  // A missing route is an availability failure, not an accessibility pass.
  // Never skip a 4xx/5xx (or a navigation with no main response).
  if (!response || response.status() >= 400) {
    if (response) {
      Object.assign(audit, { error: `navigation returned HTTP ${response.status()}` });
    }
    await testInfo.attach("corelink-a11y-result", {
      body: Buffer.from(JSON.stringify(audit)),
      contentType: "application/json",
    });
    throw new Error(
      `route ${route} unavailable (status ${response?.status() ?? "no-response"})`,
    );
  }

  const results = await new AxeBuilder({ page })
    .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa", "best-practice"])
    .analyze();

  audit.violations = results.violations.map((violation) => ({
    id: violation.id,
    impact: violation.impact,
    help: violation.help,
    helpUrl: violation.helpUrl,
    nodes: violation.nodes.length,
    tags: violation.tags,
  }));
  await testInfo.attach("corelink-a11y-result", {
    body: Buffer.from(JSON.stringify(audit)),
    contentType: "application/json",
  });

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

for (const route of A11Y_ROUTES) {
  test(`a11y: ${route}`, async ({ page }, testInfo) => {
    await scan(page, route, testInfo);
  });
}
