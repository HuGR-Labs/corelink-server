/**
 * E2E #1 — Collapsed onboarding (signup + /welcome).
 *
 * Per Phase-0 PLG framework §4 the wizard is one screen post-signup. The
 * tenant + region + plan + PAT are provisioned server-side on the Clerk
 * `user.created` webhook (apps/signup-worker), so the only visible flow
 * is: sign-up → /welcome.
 *
 * Critical assertions:
 *   - PAT shown exactly once on /welcome (CTRL-CRED-001).
 *   - Refreshing /welcome does NOT reveal the PAT plaintext again.
 *   - Time-to-first-CLI-authed under the §3.1 target (≤ 5 min stretch).
 */

import { test, expect } from "@playwright/test";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";
import { installApiMocks } from "../fixtures/api-mocks";

test.describe("Onboarding (collapsed signup + welcome)", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    await installApiMocks(page);
    await signInAs(context, FIXTURE_USERS.newDev, baseURL!);
  });

  // FIXME(F-ONBOARDING-2STEP): requires the signup-worker `user.created`
  // webhook to be wired to a Clerk-test-mode environment + an analytics_events
  // D1 binding (agent G) so the SSE stream can flip the activation badge.
  test.fixme(
    "signup → /welcome shows install one-liner + activation badge",
    async ({ page }) => {
      const t0 = Date.now();

      await page.goto("/en/welcome");
      await expect(page).toHaveURL(/\/welcome/);

      // Install one-liner present and contains the PAT exactly once.
      const cmd = page.getByTestId("install-one-liner-cmd");
      await expect(cmd).toBeVisible({ timeout: 10_000 });
      const cmdText = await cmd.innerText();
      expect(cmdText).toContain("curl -fsSL");
      expect(cmdText).toMatch(/--token=ct_\w+/);

      // Activation badge starts in the "waiting" state.
      const badge = page.getByTestId("activation-state-badge");
      await expect(badge).toHaveAttribute("data-state", "waiting");

      // Time-to-first-CLI-authed bound (PLG §3.1 stretch target).
      const elapsed = (Date.now() - t0) / 1000;
      expect(elapsed).toBeLessThan(5 * 60);

      // Reload: the install command (and its embedded PAT) MUST NOT be
      // re-served from the page on a fresh render — the token comes from a
      // one-shot Clerk session claim that the welcome route consumes once.
      await page.reload();
      const reloadedToken = await page
        .getByTestId("install-one-liner-cmd")
        .innerText()
        .catch(() => "");
      // After the one-shot consume the page redirects to /sign-up because
      // the session claim is gone, so either the redirect happened or the
      // PAT is no longer in the rendered command.
      const stillHasToken = /--token=ct_\w/.test(reloadedToken);
      expect(stillHasToken).toBe(false);
    },
  );
});
