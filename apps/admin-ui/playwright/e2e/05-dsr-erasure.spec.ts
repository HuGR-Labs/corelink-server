/**
 * E2E #5 — DSR erasure with category selection.
 *
 * Asserts:
 *   - Conditional categories presented (LGPD Art. 16; GDPR Art. 17 carve-outs).
 *   - User must explicitly check a category before submit.
 *   - Receipt + SLA clock 30d visible.
 */

import { test, expect } from "@playwright/test";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";
import { installApiMocks } from "../fixtures/api-mocks";

test.describe("DSR erasure", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    await installApiMocks(page);
    await signInAs(context, FIXTURE_USERS.existingTenant, baseURL!);
  });

  // FIXME(WI-S16-007): same as DSR access — needs real Clerk session.
  test.fixme("erasure with category selection → receipt + SLA clock", async ({ page }) => {
    await page.goto("/en/dsr/erasure");

    // Select at least one category checkbox.
    const cats = page.getByRole("checkbox");
    const n = await cats.count();
    expect(n).toBeGreaterThan(0);
    await cats.first().check();

    // MFA + submit.
    const mfaInput = page
      .getByRole("textbox", { name: /code|otp|mfa/i })
      .or(page.locator("input[autocomplete='one-time-code']"))
      .first();
    if (await mfaInput.isVisible({ timeout: 2000 }).catch(() => false)) {
      await mfaInput.fill("123456");
    }
    await page.getByRole("button", { name: /submit|send|request|erase/i }).first().click();

    await expect(
      page.locator("[data-testid='dsr-receipt'], text=/receipt|request.*submitted/i").first(),
    ).toBeVisible({ timeout: 10_000 });

    // SLA clock visible (30d countdown text or testid).
    const sla = page
      .locator("[data-testid='sla-clock'], text=/30.*day|due/i")
      .first();
    await expect(sla).toBeVisible({ timeout: 5000 });
  });
});
