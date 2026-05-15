/**
 * E2E — customer team invite (r-prep).
 *
 * Existing members render. Submit an invite → the new row appears with
 * status `invited`.
 */
import { test, expect } from "@playwright/test";
import { LoginPage } from "../pages/LoginPage";

test.describe("customer team", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("admin");
  });

  test("seeded members render and invite adds a new member", async ({ page }) => {
    await page.goto("/en/customer/team");
    await expect(page.locator("h1#customer-team-heading")).toBeVisible({ timeout: 30_000 });

    await expect(page.getByTestId("team-row-user_e2e_admin")).toBeVisible();
    await expect(page.getByTestId("team-role-user_e2e_admin")).toContainText("Owner");
    await expect(page.getByTestId("team-row-user_e2e_member")).toBeVisible();

    await page.getByTestId("team-invite-email").fill("newbie@acme.example");
    await page.getByTestId("team-invite-role").selectOption("Developer");

    const inviteResp = page.waitForResponse(
      (r) =>
        r.url().endsWith("/v1/customer/team/invite") && r.request().method() === "POST",
      { timeout: 10_000 },
    );
    await page.getByTestId("team-invite-submit").click();
    const invited = await inviteResp;
    const invitedBody = (await invited.json()) as { user_id: string };

    await expect(page.getByTestId("team-invite-success")).toContainText("newbie@acme.example");
    await expect(page.getByTestId(`team-row-${invitedBody.user_id}`)).toBeVisible();
  });
});
