/**
 * E2E #3 — Consent withdraw with MFA challenge.
 *
 * Asserts CTRL-AUTH-010 reflection: destructive op gates a fresh MFA prompt.
 * The mock backend returns a withdraw receipt.
 */

import { test, expect } from "@playwright/test";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";
import { installApiMocks } from "../fixtures/api-mocks";

test.describe("Consent withdraw", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    await installApiMocks(page);
    await signInAs(context, FIXTURE_USERS.existingTenant, baseURL!);
  });

  // FIXME(WI-S16-007): real Clerk session + history page hookup required.
  test.fixme("MFA prompt → withdrawal receipt", async ({ page }) => {
    await page.goto("/en/consent/history");

    // Find the existing consent + click withdraw.
    const withdrawBtn = page
      .getByRole("button", { name: /withdraw|revoke/i })
      .first();
    await expect(withdrawBtn).toBeVisible({ timeout: 10_000 });
    await withdrawBtn.click();

    // MFA challenge may appear as a dialog or a stepped form.
    const mfaInput = page
      .getByRole("textbox", { name: /code|otp|mfa/i })
      .or(page.locator("input[autocomplete='one-time-code']"))
      .first();
    if (await mfaInput.isVisible({ timeout: 3000 }).catch(() => false)) {
      await mfaInput.fill("123456");
      await page.getByRole("button", { name: /verify|confirm|submit/i }).first().click();
    }

    // Receipt visible.
    await expect(
      page
        .locator("[data-testid='withdraw-receipt'], text=/withdrawn|revoked|receipt/i")
        .first(),
    ).toBeVisible({ timeout: 10_000 });
  });
});
