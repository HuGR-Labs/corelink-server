/**
 * E2E — customer team invite (r-prep).
 *
 * Existing members render. Submit an invite → the new row appears with
 * status `invited`.
 */
import { test, expect } from "../fixtures/test";
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
    // WIRE-SHAPE: the container replies `201 { "member": { … } }` — the row is
    // ENVELOPED (`crates/corelink-container/src/routes/customer.rs:960-968`,
    // asserted server-side at :2062). Reading `user_id` off the top level yields
    // `undefined`, so the row locator became `team-row-undefined` and could never
    // match. Unwrap the envelope, exactly as `CustomerClient.inviteTeam` does.
    const invitedBody = (await invited.json()) as { member: { user_id: string } };
    const invitedUserId = invitedBody.member.user_id;
    expect(invitedUserId, "invite response must carry member.user_id").toBeTruthy();

    await expect(page.getByTestId("team-invite-success")).toContainText("newbie@acme.example");
    // An invited-but-not-yet-joined seat is `status: "invited"`, so TeamClient
    // renders it in the "Pending invitations" table — same `team-row-<user_id>`
    // testid, different card. Assert the row AND that it is the pending one.
    const invitedRow = page.getByTestId(`team-row-${invitedUserId}`);
    await expect(invitedRow).toBeVisible();
    await expect(page.getByTestId("team-pending-list")).toContainText("newbie@acme.example");
    await expect(invitedRow).toContainText("Invited");
  });
});
