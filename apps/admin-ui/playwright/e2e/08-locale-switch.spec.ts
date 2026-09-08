/**
 * E2E #8 — Locale switching (en → pt → es).
 *
 * Asserts:
 *   - URL prefix changes match locale.
 *   - Headings differ between locales (translation is wired, not stubbed).
 *   - The shipped authenticated DSR surface renders in each locale.
 */

import { test, expect } from "../fixtures/test";

const LOCALES = [
  { code: "en", heading: "Your data rights" },
  { code: "pt", heading: "Seus direitos sobre dados" },
  { code: "es", heading: "Tus derechos sobre los datos" },
] as const;

// We probe a locale-prefixed page that exists in every locale: /[locale]/dsr.
const PROBE_PATH = "/dsr";

test.describe("Locale switch", () => {
  for (const l of LOCALES) {
    test(`renders ${l.code} with translated shipped DSR surface`, async ({ page }) => {
      await page.goto(`/${l.code}${PROBE_PATH}`);
      await expect(page).toHaveURL(new RegExp(`/corelink/${l.code}/dsr$`));
      const heading = await page.locator("h1").first().innerText();
      expect(heading.trim()).not.toBe("");
      // The locale layout supplies translations to the shipped page. The
      // root layout currently owns the outer html element, so the browser's
      // documentElement.lang is not a reliable locale signal here; URL + copy
      // are the observable contract until that layout is consolidated.
      expect(heading.trim()).toBe(l.heading);
    });
  }

  test("headings differ across locales (translation not stubbed)", async ({ page }) => {
    const headings: Record<string, string> = {};
    for (const l of LOCALES) {
      await page.goto(`/${l.code}${PROBE_PATH}`);
      const h = await page.locator("h1").first().innerText().catch(() => "");
      headings[l.code] = h.trim();
    }
    // At least one pair differs.
    const unique = new Set(Object.values(headings).filter(Boolean));
    expect(unique.size).toBeGreaterThan(1);
  });
});
