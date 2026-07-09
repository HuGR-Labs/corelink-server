/**
 * E2E 3/5 — audit trail: table renders, filter narrows, row → detail drawer
 * (wt-r3-7).
 */
import { test, expect } from "./fixtures/test";
import { LoginPage } from "./pages/LoginPage";
import { AuditPage } from "./pages/AuditPage";

test.describe("audit trail viewer", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("admin");
  });

  test("renders rows, since-filter narrows, row click opens drawer with Merkle proof", async ({
    page,
  }) => {
    const audit = new AuditPage(page);
    await audit.visit("en");
    await audit.expectTableLoaded(3);

    // Filter to 2026-05-12 onwards → only evt_002 and evt_003 should remain
    // (evt_001 is at 2026-05-10).
    await audit.filterSince("2026-05-12");
    await expect(page.getByTestId("audit-row-evt_001")).toHaveCount(0);
    await expect(page.getByTestId("audit-row-evt_002")).toBeVisible();
    await expect(page.getByTestId("audit-row-evt_003")).toBeVisible();

    // Click into evt_002 → drawer opens with merkle tab available.
    await audit.openRow("evt_002");
    await audit.expectDrawerEvent("evt_002");
    await page.getByTestId("drawer-tab-merkle").click();
    await expect(page.getByTestId("drawer-panel-merkle")).toBeVisible();
  });
});
