/**
 * E2E — customer billing surface (r-prep).
 *
 * View subscription + invoice list, then click "Manage payment method" and
 * verify the portal redirect URL surfaces.
 */
import { test, expect } from "@playwright/test";
import { LoginPage } from "../pages/LoginPage";

test.describe("customer billing", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("admin");
  });

  test("subscription state renders and portal returns a redirect url", async ({ page }) => {
    await page.goto("/en/customer/billing");
    await expect(page.locator("h1#customer-billing-heading")).toBeVisible({ timeout: 30_000 });

    await expect(page.getByTestId("billing-plan")).toContainText("team");
    await expect(page.getByTestId("billing-status")).toContainText("active");
    await expect(page.getByTestId("billing-amount-due")).toContainText("$199.00");
    await expect(page.getByTestId("billing-brand")).toContainText("visa");
    await expect(page.getByTestId("billing-last4")).toContainText("4242");
    await expect(page.getByTestId("billing-invoice-inv_2026_04")).toBeVisible();

    // Portal click → POST /v1/customer/billing/portal → redirect URL surfaces.
    const portalResp = page.waitForResponse(
      (r) =>
        r.url().endsWith("/v1/customer/billing/portal") && r.request().method() === "POST",
      { timeout: 10_000 },
    );
    await page.getByTestId("billing-portal-btn").click();
    await portalResp;
    await expect(page.getByTestId("billing-portal-url")).toContainText("portal/session_test");
  });
});
