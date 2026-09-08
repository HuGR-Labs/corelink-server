/**
 * E2E #2 — Consent capture flow.
 *
 * The shipped consent UI is retired until its ledger endpoint exists. This
 * authenticated probe protects that boundary and rejects a false receipt.
 */

import { test, expect } from "../fixtures/test";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";

test.describe("Consent capture", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    await signInAs(context, FIXTURE_USERS.existingTenant, baseURL!);
  });

  test("retired consent surface → 404 without a receipt or mutation", async ({ page }) => {
    // Consent capture is intentionally retired until a real ledger endpoint
    // exists. Keep this probe in the authenticated suite so re-enabling a
    // stub page cannot silently solicit a record that is never persisted.
    const response = await page.goto("/en/consent/new", { waitUntil: "domcontentloaded" });
    expect(response?.status()).toBe(404);
    // A route-local retirement marker proves this is the compiled consent
    // route's deliberate 404, rather than Next's generic route-tree miss.
    await expect(page.getByTestId("consent-capture-retired")).toBeVisible();
    await expect(page.getByTestId("consent-capture")).toHaveCount(0);
    await expect(page.locator("body")).not.toContainText(/receipt|jwt/i);
  });
});
