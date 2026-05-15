/**
 * E2E — customer self-serve overview (r-prep).
 *
 * Authenticated tenant member visits /en/customer and sees their tenant's
 * usage / billing / BYOK snapshot + recent activity. No operator chrome.
 */
import { test, expect } from "@playwright/test";
import { LoginPage } from "../pages/LoginPage";

test.describe("customer overview", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("admin");
  });

  test("renders tenant + usage + billing + BYOK + activity cards", async ({ page }) => {
    await page.goto("/en/customer");
    await expect(page.locator("h1#customer-overview-heading")).toBeVisible({ timeout: 30_000 });

    await expect(page.getByTestId("overview-tenant-id")).toContainText("tenant_acme");
    await expect(page.getByTestId("overview-plan")).toContainText("team");
    await expect(page.getByTestId("overview-cas-bytes")).toContainText("GiB");
    await expect(page.getByTestId("overview-reads")).toBeVisible();
    await expect(page.getByTestId("overview-writes")).toBeVisible();
    await expect(page.getByTestId("overview-billing-status")).toContainText("active");
    await expect(page.getByTestId("overview-byok-status")).toContainText("active");
    await expect(page.getByTestId("overview-activity-list").locator("li")).not.toHaveCount(0);

    // Customer-side navigation, no operator routes leak in.
    const nav = page.getByTestId("customer-nav");
    await expect(nav).toBeVisible();
    await expect(nav.getByText("Overview")).toBeVisible();
    await expect(nav.getByText("Tenants")).toHaveCount(0);
    await expect(nav.getByText("Ops")).toHaveCount(0);
  });
});
