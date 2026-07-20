import { test, expect } from "../fixtures/auth.js";

/**
 * DSR JOURNEY — the customer self-service GDPR surface (the black-box PAT suite
 * gates it: the erasure request is Clerk-session authed). A signed-in user
 * opens the DSR portal, sees the 6 rights, and enters the erasure flow — the
 * exact path a data subject drives to request deletion of their own data.
 */

test("DSR portal lists the rights and the erasure flow renders authed", async ({
  authedPage: page,
}) => {
  await page.goto("https://humangr.com/corelink/en/dsr");
  await page.waitForLoadState("networkidle").catch(() => {});
  expect(page.url(), "DSR portal must be reachable authed").not.toContain("/sign-in");

  const rights = page.locator('[data-testid="dsr-rights-list"]');
  await expect(rights, "the DSR rights list must render").toBeVisible({ timeout: 30_000 });

  // Every GDPR right renders an action button; erasure is the one the money/DSR
  // gate cares about.
  const erasure = page.locator('[data-testid="dsr-action-button-erasure"]');
  await expect(erasure, "the erasure right must be offered").toBeVisible();

  await erasure.click();
  await page.waitForURL(/\/dsr\/erasure/, { timeout: 20_000 });
  await page.waitForLoadState("networkidle").catch(() => {});
  // eslint-disable-next-line no-console
  console.log(`[dsr] erasure action page = ${page.url().replace("https://humangr.com", "")}`);

  // The erasure action page must render its own island (heading + a real form:
  // erasure requires a reason, REASON_REQUIRED_ACTIONS) — not a bounce/blank.
  expect(page.url(), "erasure page must not bounce to sign-in").not.toContain("/sign-in");
  const heading = page.locator("#dsr-action-title, h1");
  await expect(heading, "erasure page renders its heading").toBeVisible({ timeout: 20_000 });
  const hasForm = await page
    .locator('form, textarea, input, button[type="submit"], [data-testid*="dsr"]')
    .first()
    .isVisible()
    .catch(() => false);
  expect(hasForm, "erasure page renders an actionable form/control").toBeTruthy();
});
