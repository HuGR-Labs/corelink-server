/**
 * E2E 4/5 — BYOK CMK rotate via dual-approval queue (wt-r3-7).
 *
 * The S-16 UI routes sensitive ops through `/admin/ops/[op_id]` rather than a
 * standalone "rotate CMK button → confirm modal → success toast" sequence on
 * the tenant detail page. The flow we test is equivalent:
 *
 *   1. Approver opens the BYOK rotation op pending in the queue.
 *   2. Enters an approval reason (the audit-trail-required confirmation step).
 *   3. Clicks Approve → server records the approval → DualApprovalCard
 *      transitions to "one-of-two" (the visible "success" indication; there
 *      is no toast component in this UI surface).
 *   4. The audit log records the approval as `admin.op_approved`.
 */
import { test, expect } from "./fixtures/test";
import { LoginPage } from "./pages/LoginPage";
import { AuditPage } from "./pages/AuditPage";

test.describe("byok cmk rotate", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    // Approver (NOT requestor) — DualApprovalCard greys-out the approve
    // button for the requestor (CTRL-DUAL-APPROVAL).
    await login.signInAs("approver");
  });

  test("approver signs the BYOK rotation → audit row appears", async ({ page }) => {
    await page.goto("/en/admin/ops/op_byok_001");
    await expect(page.locator("h1#op-heading")).toContainText("op_byok_001", { timeout: 30_000 });
    await expect(page.getByTestId("dual-approval-card")).toHaveAttribute(
      "data-state",
      "pending",
      { timeout: 30_000 },
    );

    // 2. Enter the approval reason (confirmation step).
    await page.getByTestId("approval-reason").fill("Q2 key rotation per CTRL-CRYPTO-001");

    // 3. Approve → expect the card to transition to "one-of-two".
    const approveBtn = page.getByTestId("approve-btn");
    await expect(approveBtn).toBeEnabled();
    await approveBtn.click();
    await expect(page.getByTestId("approval-state")).toHaveText("one-of-two");

    // 4. Navigate to the audit viewer → the approval is recorded.
    const audit = new AuditPage(page);
    await audit.visit("en");
    await audit.expectAuditRowFor(/approved byok_cmk_rotation op_byok_001/);
  });
});
