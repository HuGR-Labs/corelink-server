/**
 * E2E — customer audit trail (r-prep).
 *
 * The tenant-scoped audit table loads and the `from` filter narrows the
 * rows. No Merkle proof tab (operator-only surface).
 */
import { test, expect } from "@playwright/test";
import { LoginPage } from "../pages/LoginPage";

test.describe("customer audit", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("admin");
  });

  test("table loads and from-filter narrows rows", async ({ page }) => {
    await page.goto("/en/customer/audit");
    await expect(page.locator("h1#customer-audit-heading")).toBeVisible({ timeout: 30_000 });

    const table = page.getByTestId("customer-audit-table");
    await expect(table).toBeVisible();
    await expect
      .poll(async () => page.locator("[data-testid^='customer-audit-row-']").count(), {
        timeout: 15_000,
      })
      .toBeGreaterThanOrEqual(3);

    // cevt_001 is 2026-05-10 → from=2026-05-12 should drop it.
    const since = page.getByTestId("customer-audit-since");
    const fetchPromise = page.waitForResponse(
      (r) => r.url().includes("/v1/customer/audit") && r.url().includes("from="),
      { timeout: 10_000 },
    );
    await since.fill("2026-05-12");
    await fetchPromise.catch(() => undefined);

    await expect(page.getByTestId("customer-audit-row-cevt_001")).toHaveCount(0);
    await expect(page.getByTestId("customer-audit-row-cevt_002")).toBeVisible();
    await expect(page.getByTestId("customer-audit-row-cevt_003")).toBeVisible();
  });
});
