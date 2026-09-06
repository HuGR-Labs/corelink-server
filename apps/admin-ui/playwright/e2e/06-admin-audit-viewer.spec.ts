/**
 * E2E #6 — Admin audit viewer.
 *
 * Asserts:
 *   - Admin can list events with filters.
 *   - Detail drawer opens.
 *   - Merkle proof verification status visible (the shipped fixture is
 *     deliberately non-matching, so the UI must surface mismatch).
 */

import { test, expect } from "../fixtures/test";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";

test.describe("Admin audit viewer", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    await signInAs(context, FIXTURE_USERS.admin, baseURL!);
  });

  test("filter, open drawer, see Merkle proof verification status", async ({ page }) => {
    await page.goto("/en/admin/audit");

    await expect(page.getByTestId("audit-table")).toBeVisible({ timeout: 10_000 });
    const eventFilter = page.getByTestId("filter-event-auth.login");
    await eventFilter.check();

    // Event row + click to open drawer.
    const firstRow = page
      .locator("[data-testid^='audit-row-']")
      .first();
    await expect(firstRow).toBeVisible({ timeout: 10_000 });
    await firstRow.click();

    // Merkle proof status is visible, including the fixture's mismatch.
    await page.getByTestId("drawer-tab-merkle").click();
    await expect(page.getByTestId("merkle-proof-viewer")).toBeVisible({ timeout: 5000 });
    await expect(page.locator('[data-status="mismatch"]')).toBeVisible({ timeout: 10_000 });
  });
});
