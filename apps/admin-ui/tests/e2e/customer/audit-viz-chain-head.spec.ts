/**
 * wt/r-prep-audit-chain-viz — chain head card populates from the mock backend.
 *
 * Verifies Component A of the customer audit-chain visualization spec
 * (`specs/_audits/sealed/2026-05-15-audit-viz-spec.md`):
 *   - page loads under `/[locale]/customer/audit/visualization`
 *   - chain head card renders total_events, head digest, algorithm,
 *     last-updated
 *   - "Verify head" button performs a client-side verification round-trip
 *     and surfaces the OK status (mocked single-leaf proof verifies
 *     trivially without requiring the WASM bundle in CI).
 */
import { test, expect } from "@playwright/test";
import { LoginPage } from "../pages/LoginPage";

test.describe("customer audit-chain visualization — chain head", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("admin");
  });

  test("chain head card populates and verify-head succeeds", async ({ page }) => {
    await page.goto("/en/customer/audit/visualization");

    await expect(page.locator("h1#page-heading")).toBeVisible({ timeout: 30_000 });

    const card = page.getByTestId("chain-head-card");
    await expect(card).toBeVisible();

    // Total events from mock fixture = 3.
    await expect(page.getByTestId("chain-head-total")).toHaveText("3");

    // Algorithm pinned to blake3 per the customer audit chain spec.
    await expect(page.getByTestId("chain-head-algorithm")).toHaveText("blake3");

    // Digest renders truncated (mock = "ab" repeated 32 times = 64 hex chars).
    const digest = page.getByTestId("chain-head-digest");
    await expect(digest).toBeVisible();
    await expect(digest).toHaveAttribute("title", "ab".repeat(32));

    // Last-updated renders as a relative time string.
    await expect(page.getByTestId("chain-head-last-updated")).toBeVisible();

    // Leaf table populates so verify-head has a leaf to verify against.
    await expect(page.getByTestId("audit-leaf-table")).toBeVisible();
    await expect(page.getByTestId("leaf-row-cevt_001")).toBeVisible();

    // Click "Verify head" — the mock proof is single-leaf trivial so the
    // verifier returns ok without needing WASM.
    const verifyBtn = page.getByTestId("verify-head-btn");
    await expect(verifyBtn).toBeEnabled();
    await verifyBtn.click();

    const status = page.getByTestId("verify-head-status");
    await expect
      .poll(async () => status.getAttribute("data-state"), { timeout: 10_000 })
      .toBe("ok");
    await expect(status).toContainText("Verified locally");
  });
});
