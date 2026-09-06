/**
 * E2E #4 — DSR access (LGPD Art. 18 II + GDPR Art. 15).
 *
 * The shipped action page waits for real Clerk token/profile wiring. This
 * probe verifies that missing tooling cannot become a fabricated receipt.
 */

import { test, expect } from "../fixtures/test";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";
import { installApiMocks } from "../fixtures/api-mocks";

test.describe("DSR access", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    await installApiMocks(page);
    await signInAs(context, FIXTURE_USERS.existingTenant, baseURL!);
  });

  test("fresh MFA → access receipt → status keeps the request id", async ({ page }) => {
    await page.goto("/en/dsr");
    const access = page.getByTestId("dsr-action-button-access");
    await expect(access).toBeVisible();
    await access.click();
    await page.goto("/en/dsr/access");
    await expect(page).toHaveURL(/\/en\/dsr\/access/);

    // The form is not rendered until the fresh re-auth gate is satisfied.
    await expect(page.getByTestId("dsr-reauth-gate")).toBeVisible();
    await expect(page.getByTestId("dsr-form-access")).toHaveCount(0);
    await expect(page.getByTestId("dsr-reauth-start")).toBeEnabled();
    await page.getByTestId("dsr-reauth-start").click();
    await expect(page.getByTestId("dsr-reauth-verified")).toBeVisible();
    await expect(page.getByTestId("dsr-form-access")).toBeVisible();

    await page.getByTestId("dsr-submit").click();
    await expect(page.getByTestId("dsr-receipt-modal")).toBeVisible();
    const requestId = await page.getByTestId("receipt-request-id").innerText();
    expect(requestId).toMatch(/^dsr_e2e_/);
    await expect(page.getByTestId("receipt-action")).toHaveText("access");
    await expect(page.getByTestId("receipt-deadline")).toContainText(/2026/);

    await page.goto("/en/dsr/status");
    await expect(page.getByTestId(`dsr-row-${requestId}`)).toBeVisible();
    await expect(page.getByTestId(`dsr-row-view-${requestId}`)).toBeVisible();
  });
});
