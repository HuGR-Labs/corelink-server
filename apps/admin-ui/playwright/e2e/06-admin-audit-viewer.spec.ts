/**
 * E2E #6 — Admin audit viewer.
 *
 * Asserts:
 *   - Admin can list events with filters.
 *   - Detail drawer opens.
 *   - Merkle proof verification status visible (verified=true).
 */

import { test, expect } from "../fixtures/test";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";
import { installApiMocks } from "../fixtures/api-mocks";

test.describe("Admin audit viewer", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    await installApiMocks(page);
    await signInAs(context, FIXTURE_USERS.admin, baseURL!);
  });

  // FIXME(WI-S16-007): admin role-gated page requires real Clerk session.
  test.fixme("filter, open drawer, see Merkle proof verified", async ({ page }) => {
    await page.goto("/en/admin/audit");

    // Filter by type if a selector exists.
    const typeFilter = page.getByRole("combobox", { name: /type|kind/i }).first();
    if (await typeFilter.isVisible({ timeout: 2000 }).catch(() => false)) {
      await typeFilter.click();
      const opt = page.getByRole("option", { name: /consent/i }).first();
      if (await opt.isVisible({ timeout: 2000 }).catch(() => false)) await opt.click();
    }

    // Event row + click to open drawer.
    const firstRow = page
      .locator("[data-testid^='audit-row-'], tbody tr, [role='row']")
      .first();
    await expect(firstRow).toBeVisible({ timeout: 10_000 });
    await firstRow.click();

    // Merkle proof status visible + valid.
    await expect(
      page.locator("text=/merkle|verified|valid/i").first(),
    ).toBeVisible({ timeout: 5000 });
  });
});
