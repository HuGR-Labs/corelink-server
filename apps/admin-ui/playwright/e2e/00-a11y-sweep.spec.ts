/**
 * axe-core full-page sweep — WI-S16-007 deliverable 3.
 *
 * Visits every public + auth-gated page and asserts ZERO serious/critical
 * WCAG 2.2 AA violations. Moderate/minor violations are logged but do not
 * fail the build (per spec contract S-16 §6 DoD + 10.s16.4).
 */

import { test, expect } from "../fixtures/test";
import AxeBuilder from "@axe-core/playwright";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";

const PAGES = [
  { path: "/", auth: "none" as const, name: "landing" },
  { path: "/sign-in", auth: "none" as const, name: "sign-in" },
  { path: "/en/welcome", auth: "newDev" as const, name: "welcome" },
  // `/en/consent/new` removed: the consent surface is RETIRED and answers 404
  // (`src/app/[locale]/consent/retired.ts`), which fails the <400 status
  // assertion below. Restore this entry when `CONSENT_UI_RETIRED` flips back.
  { path: "/en/dsr", auth: "existingTenant" as const, name: "dsr-landing" },
  { path: "/en/admin/audit", auth: "admin" as const, name: "admin-audit" },
  { path: "/en/privacy", auth: "none" as const, name: "privacy" },
];

for (const p of PAGES) {
  test(`a11y: ${p.name} (${p.path}) — zero serious/critical violations`, async ({
    page,
    context,
    baseURL,
  }) => {
    if (p.auth !== "none") {
      await signInAs(context, FIXTURE_USERS[p.auth], baseURL!);
    }
    const resp = await page.goto(p.path, { waitUntil: "domcontentloaded" });
    // Accept 200..399 (redirects to locale-prefixed paths are OK).
    expect(resp?.status() ?? 0).toBeLessThan(400);

    const results = await new AxeBuilder({ page })
      .withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"])
      .analyze();

    const blocking = results.violations.filter(
      (v) => v.impact === "serious" || v.impact === "critical",
    );
    if (blocking.length > 0) {
      console.error(
        `[a11y:${p.name}] serious/critical:`,
        JSON.stringify(
          blocking.map((v) => ({ id: v.id, impact: v.impact, help: v.help })),
          null,
          2,
        ),
      );
    }
    expect(blocking).toEqual([]);
  });
}
