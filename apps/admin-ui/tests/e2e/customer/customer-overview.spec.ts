/**
 * E2E — customer self-serve overview (Linear Home).
 *
 * Authenticated tenant member visits /en/customer and sees their tenant's
 * usage / billing / BYOK snapshot + recent activity. No operator chrome.
 *
 * The page is `HomeClient` (Linear kit). It renders a `home-loading` skeleton
 * until the mocked overview resolves, then `home-shell`. Human labels are shown
 * (plan "Team", status "Active"), never raw enums. Usage on this screen is the
 * storage gauge (+ a hidden `overview-cas-bytes` probe); the per-request
 * reads/writes counters live on the dedicated Usage screen, not here.
 */
import { test, expect } from "../fixtures/test";
import { LoginPage } from "../pages/LoginPage";

test.describe("customer overview", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("admin");
  });

  test("renders tenant + usage + billing + BYOK + activity cards", async ({ page }) => {
    await page.goto("/en/customer");
    await expect(page.locator("h1#customer-overview-heading")).toBeVisible({ timeout: 30_000 });

    // Loaded state (skeleton → shell once the mocked overview resolves).
    await expect(page.getByTestId("home-shell")).toBeVisible({ timeout: 30_000 });

    // Tenant identity — the "at a glance" card is labelled with the tenant name
    // (no tenant-id testid is surfaced on this screen).
    await expect(page.getByText("Acme Inc.")).toBeVisible();

    // Plan + storage usage snapshot (human label, not the raw enum).
    await expect(page.getByTestId("overview-plan")).toContainText("Team");
    await expect(page.getByTestId("home-storage-gauge")).toBeVisible();
    await expect(page.getByTestId("overview-cas-bytes")).toContainText("GiB");

    // Billing + BYOK snapshot — humanized status badges.
    await expect(page.getByTestId("home-billing-status")).toContainText("Active");
    await expect(page.getByTestId("overview-byok-status")).toContainText("Active");

    // Recent activity — the mock seeds events, so the list renders (not empty).
    await expect(page.getByTestId("overview-activity-list").locator("li")).not.toHaveCount(0);

    // Customer-side navigation, no operator routes leak in.
    const nav = page.getByTestId("customer-nav");
    await expect(nav).toBeVisible();
    await expect(nav.getByTestId("nav-overview")).toBeVisible();
    await expect(nav.getByText("Tenants")).toHaveCount(0);
    await expect(nav.getByText("Ops")).toHaveCount(0);
  });
});
