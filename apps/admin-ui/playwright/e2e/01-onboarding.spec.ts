/**
 * E2E #1 — Collapsed onboarding (signup + /welcome).
 *
 * Per Phase-0 PLG framework §4 the wizard is one screen post-signup. The
 * tenant + region + plan + PAT are provisioned server-side on the Clerk
 * `user.created` webhook (apps/signup-worker). Until that webhook provisions
 * the synthetic user, the shipped safe branch must return to sign-in.
 *
 * Critical assertion:
 *   - An unprovisioned session never receives a fabricated PAT or installer.
 */

import { test, expect } from "../fixtures/test";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";

test.describe("Onboarding (collapsed signup + welcome)", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    await signInAs(context, FIXTURE_USERS.newDev, baseURL!);
  });

  test("unprovisioned session → /welcome returns to sign-in without a PAT", async ({ page }) => {
    // The synthetic user deliberately has no tenant. The shipped page must
    // take its documented safe branch while the signup webhook is pending;
    // it must never render a made-up token or install command.
    const response = await page.goto("/en/welcome", { waitUntil: "domcontentloaded" });
    expect(response?.status() ?? 0).toBeLessThan(400);
    await expect(page).toHaveURL(/\/sign-in/);
    await expect(page.getByTestId("install-one-liner-cmd")).toHaveCount(0);
    await expect(page.locator("body")).not.toContainText(/--token=|corelink_pat_/i);
  });
});
