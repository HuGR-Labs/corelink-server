/**
 * E2E — customer PAT lifecycle (r-prep).
 *
 * Create a PAT, verify the once-shown token surfaces, then revoke it and
 * confirm the row flips to `revoked`.
 */
import { test, expect } from "@playwright/test";
import { LoginPage } from "../pages/LoginPage";

test.describe("customer keys", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("admin");
  });

  test("create + revoke PAT round-trip", async ({ page }) => {
    await page.goto("/en/customer/keys");
    await expect(page.locator("h1#customer-keys-heading")).toBeVisible({ timeout: 30_000 });

    // Existing PATs render.
    await expect(page.getByTestId("keys-row-pat_001")).toBeVisible();

    // Create a new PAT.
    await page.getByTestId("keys-create-name").fill("e2e-pat");
    await page.getByTestId("keys-scope-cache:r").check();
    const createResp = page.waitForResponse(
      (r) => r.url().endsWith("/v1/customer/keys") && r.request().method() === "POST",
      { timeout: 10_000 },
    );
    await page.getByTestId("keys-create-submit").click();
    const created = await createResp;
    const createdBody = (await created.json()) as { pat_id: string };
    const newId = createdBody.pat_id;

    // Token shown once.
    await expect(page.getByTestId("keys-new-token")).toContainText("crl_pat_");

    // The newly created row appears.
    await expect(page.getByTestId(`keys-row-${newId}`)).toBeVisible();
    await expect(page.getByTestId(`keys-status-${newId}`)).toContainText("active");

    // Revoke it. The revoke button opens a ConfirmDialog (destructive-action
    // guard); the actual revoke fires from the dialog's confirm button.
    await page.getByTestId(`keys-revoke-${newId}`).click();
    const revokeResp = page.waitForResponse(
      (r) => r.url().endsWith(`/v1/customer/keys/${newId}/revoke`),
      { timeout: 10_000 },
    );
    await page.getByRole("button", { name: "Revoke token" }).click();
    await revokeResp;

    await expect(page.getByTestId(`keys-status-${newId}`)).toContainText("revoked");
  });
});
