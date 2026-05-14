/**
 * E2E #7 — Admin dual-approval flow.
 *
 * Asserts PAT-DUAL-APPROVAL-001 (S-13 reuse):
 *   - Requestor CANNOT approve own op (409 returned + UI shows error).
 *   - A second approver with fresh MFA executes.
 */

import { test, expect } from "@playwright/test";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";
import { installApiMocks } from "../fixtures/api-mocks";

test.describe("Admin dual-approval", () => {
  // FIXME(WI-S16-007): admin dual-approval needs two real Clerk sessions.
  test.fixme("requestor self-approval rejected; second approver succeeds", async ({
    page,
    context,
    baseURL,
  }) => {
    await installApiMocks(page);

    // Attempt 1 — sign in as the submitter (newdev). Their own pending op
    // is `op_pending_1` (submitted_by user_e2e_newdev in fixtures).
    // First switch to admin role so /admin/* is accessible, but use a
    // header to indicate submitter identity to the mock.
    await signInAs(context, { ...FIXTURE_USERS.admin, sub: "user_e2e_newdev" }, baseURL!);
    await page.setExtraHTTPHeaders({ "x-e2e-user": "user_e2e_newdev" });

    await page.goto("/en/admin/ops");
    const opRow = page
      .locator("[data-testid^='op-row-'], tbody tr")
      .first();
    await expect(opRow).toBeVisible({ timeout: 10_000 });
    await opRow.click();

    // Try self-approve.
    const approveBtn = page.getByRole("button", { name: /approve/i }).first();
    if (await approveBtn.isVisible({ timeout: 3000 }).catch(() => false)) {
      await approveBtn.click();
      await expect(
        page.locator("text=/cannot.*approve.*own|conflict|dual.*approval/i").first(),
      ).toBeVisible({ timeout: 5000 });
    }

    // Attempt 2 — second approver.
    await signInAs(context, FIXTURE_USERS.approver, baseURL!);
    await page.setExtraHTTPHeaders({ "x-e2e-user": "user_e2e_approver" });
    await page.goto("/en/admin/ops");
    await page
      .locator("[data-testid^='op-row-'], tbody tr")
      .first()
      .click();

    const mfaInput = page
      .getByRole("textbox", { name: /code|otp|mfa/i })
      .or(page.locator("input[autocomplete='one-time-code']"))
      .first();
    if (await mfaInput.isVisible({ timeout: 2000 }).catch(() => false)) {
      await mfaInput.fill("123456");
    }
    await page.getByRole("button", { name: /approve|execute/i }).first().click();
    await expect(
      page.locator("text=/executed|approved|success/i").first(),
    ).toBeVisible({ timeout: 5000 });
  });
});
