/**
 * wt/r-prep-audit-chain-viz — clicking a leaf opens the proof modal and the
 * in-browser verifier passes.
 *
 * Verifies Component C of the customer audit-chain visualization spec
 * (`specs/_audits/sealed/2026-05-15-audit-viz-spec.md`):
 *   - "Show proof" on a row opens the inclusion-proof modal
 *   - the modal renders leaf hash + expected root + algorithm + sibling
 *     count
 *   - the in-browser verifier completes and surfaces `data-ok="true"`
 *   - the modal closes cleanly on Escape
 */
import { test, expect } from "../fixtures/test";
import { LoginPage } from "../pages/LoginPage";

test.describe("customer audit-chain visualization — proof modal", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("admin");
  });

  test("clicking a leaf opens proof modal and in-browser verification passes", async ({
    page,
  }) => {
    await page.goto("/en/customer/audit/visualization");
    await expect(page.locator("h1#page-heading")).toBeVisible({ timeout: 30_000 });

    // Wait for the leaf table to populate.
    await expect(page.getByTestId("audit-leaf-table")).toBeVisible();
    await expect(page.getByTestId("leaf-row-cevt_002")).toBeVisible();

    // Trigger the proof modal for event cevt_002.
    await page.getByTestId("show-proof-cevt_002").click();

    const modal = page.getByTestId("proof-modal");
    await expect(modal).toBeVisible();

    // Modal renders the event id + chain seq.
    await expect(page.getByTestId("proof-event-id")).toHaveText("cevt_002");
    await expect(page.getByTestId("proof-chain-seq")).toHaveText("1");
    await expect(page.getByTestId("proof-algorithm")).toHaveText("blake3");

    // Mock proof is single-leaf trivial — siblings count == 0, expected_root
    // equals leaf_hash so the verifier short-circuits to OK without needing
    // the WASM bundle in CI.
    await expect(page.getByTestId("proof-sibling-count")).toHaveText("0");

    const result = page.getByTestId("proof-result");
    await expect
      .poll(async () => result.getAttribute("data-ok"), { timeout: 10_000 })
      .toBe("true");
    await expect(result).toHaveAttribute("data-verifier", "trivial");
    await expect(result).toContainText("Proof verified");

    // Close via Escape — restores focus to the page.
    await page.keyboard.press("Escape");
    await expect(modal).toBeHidden();
  });
});
