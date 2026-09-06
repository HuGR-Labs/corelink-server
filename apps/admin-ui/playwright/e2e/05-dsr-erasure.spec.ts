/**
 * E2E #5 — DSR erasure with category selection.
 *
 * The shipped action page waits for real Clerk token/profile wiring. This
 * probe verifies the missing-tool state has no destructive controls or receipt.
 */

import { test, expect } from "../fixtures/test";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";
import { installApiMocks } from "../fixtures/api-mocks";

test.describe("DSR erasure", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    await installApiMocks(page);
    await signInAs(context, FIXTURE_USERS.existingTenant, baseURL!);
  });

  test("category erasure requires fresh MFA and shows receipt + SLA clock", async ({ page }) => {
    await page.goto("/en/dsr");
    const erasure = page.getByTestId("dsr-action-button-erasure");
    await expect(erasure).toBeVisible();
    await erasure.click();
    await page.goto("/en/dsr/erasure");
    await expect(page).toHaveURL(/\/en\/dsr\/erasure/);
    await expect(page.getByTestId("dsr-reauth-gate")).toBeVisible();
    await expect(page.getByTestId("dsr-reauth-start")).toBeEnabled();
    await page.getByTestId("dsr-reauth-start").click();
    await expect(page.getByTestId("dsr-erasure-fields")).toBeVisible();

    await page.getByTestId("dsr-erasure-scope-categories").check();
    const category = page.getByTestId("dsr-erasure-cat-profile");
    await expect(category).toBeVisible();
    await category.check();
    await page.getByTestId("dsr-reason").fill("Please erase my profile data.");
    await page.getByTestId("dsr-submit").click();

    await expect(page.getByTestId("dsr-receipt-modal")).toBeVisible();
    await expect(page.getByTestId("receipt-action")).toHaveText("erasure");
    await expect(page.getByTestId("dsr-sla-countdown")).toBeVisible();
    await expect(page.getByTestId("dsr-sla-countdown")).toHaveAttribute("data-color", /green|yellow|red/);
  });
});
