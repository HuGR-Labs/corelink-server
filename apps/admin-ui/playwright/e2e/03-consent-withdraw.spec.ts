/**
 * E2E #3 — Consent withdraw with MFA challenge.
 *
 * The shipped consent UI is retired with capture until its ledger endpoint
 * exists. This authenticated probe ensures withdrawal cannot imply mutation.
 */

import { test, expect } from "../fixtures/test";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";

test.describe("Consent withdraw", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    await signInAs(context, FIXTURE_USERS.existingTenant, baseURL!);
  });

  test("retired consent history → 404 without a withdrawal mutation", async ({ page }) => {
    // Withdrawal is kept unavailable with capture: no UI may imply that a
    // consent ledger exists while its backend is absent.
    const response = await page.goto("/en/consent/history", { waitUntil: "domcontentloaded" });
    expect(response?.status()).toBe(404);
    // Require the history segment's retirement boundary so a generic 404 from
    // an uncompiled route cannot make this safety assertion vacuous.
    await expect(page.getByTestId("consent-history-retired")).toBeVisible();
    await expect(page.getByTestId("withdraw-form")).toHaveCount(0);
    await expect(page.locator("body")).not.toContainText(/withdrawn|revoked|receipt/i);
  });
});
