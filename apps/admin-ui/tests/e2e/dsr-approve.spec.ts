/**
 * E2E 5/5 — DSR approve via dual-approval queue (wt-r3-7).
 *
 * S-11 DSR access/erasure requests are routed through `tenant_data_export`
 * dual-approval ops (see e2e-mock-fixtures.ts). The flow is symmetric to
 * byok-rotate but on op_dsr_001.
 */
import { test, expect } from "@playwright/test";
import { LoginPage } from "./pages/LoginPage";
import { AuditPage } from "./pages/AuditPage";

test.describe("dsr approve", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("approver");
  });

  test("approver reviews and approves the DSR → status flips + audit row", async ({ page }) => {
    await page.goto("/en/admin/ops/op_dsr_001");
    await expect(page.locator("h1#op-heading")).toContainText("op_dsr_001", { timeout: 30_000 });

    // Review the request payload (request_id + tenant_id).
    const detail = page.getByTestId("op-detail-view");
    await expect(detail).toContainText("dsr_001");
    await expect(detail).toContainText("tenant_001");

    // Initial state is "pending".
    await expect(page.getByTestId("approval-state")).toHaveText("pending");

    // Enter the approval reason, then wait for the card to settle before
    // approving: the op detail hydrates async, and DualApprovalCard keeps the
    // reason in local state — click too eagerly and a late re-render drops the
    // value, leaving `approve-btn` disabled (reason.trim() empty). Mirror the
    // byok-rotate flow: confirm the value stuck AND the button is enabled first.
    const reason = page.getByTestId("approval-reason");
    await reason.fill("DSR-30d SLA review per S-11 R-S11-7");
    await expect(reason).toHaveValue("DSR-30d SLA review per S-11 R-S11-7");
    const approveBtn = page.getByTestId("approve-btn");
    await expect(approveBtn).toBeEnabled();
    await approveBtn.click();

    // Status flips: state advances out of "pending".
    await expect(page.getByTestId("approval-state")).toHaveText("one-of-two");

    // Audit trail records the approval.
    const audit = new AuditPage(page);
    await audit.visit("en");
    await audit.expectAuditRowFor(/approved tenant_data_export op_dsr_001/);
  });
});
