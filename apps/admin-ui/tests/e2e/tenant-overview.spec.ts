/**
 * E2E 2/5 — tenant overview: list, search, click → deep-dive (wt-r3-7).
 */
import { test, expect } from "./fixtures/test";
import { LoginPage } from "./pages/LoginPage";
import { TenantPage } from "./pages/TenantPage";

test.describe("tenant overview", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("admin");
  });

  test("list renders, search filters results, click row → deep-dive", async ({ page }) => {
    const tenants = new TenantPage(page);
    await tenants.visit("en");
    await tenants.expectListLoaded();

    // Page 1 should contain 10 tenants (cursor-based pagination, page size 10).
    await tenants.expectRowCount(10, 10);

    // Search narrows the result set to the matching tenant_id.
    await tenants.searchFor("tenant_001");
    await expect(page.getByTestId("tenant-row-tenant_001")).toBeVisible();
    await tenants.expectRowCount(1, 1);

    // Clear search, then click into tenant_002 deep-dive.
    await tenants.searchFor("");
    await expect(page.getByTestId("tenant-row-tenant_002")).toBeVisible();
    await tenants.clickTenant("tenant_002");
    await tenants.expectDeepDive("tenant_002");
  });
});
