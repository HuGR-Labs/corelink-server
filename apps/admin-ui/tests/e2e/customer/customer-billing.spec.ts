/**
 * E2E — customer billing surface (Linear BillingClient).
 *
 * View subscription + invoice list, then click "Manage subscription" and verify
 * the Stripe portal redirect fires.
 *
 * Notes on the current page vs. the pre-Linear one:
 *   - Human labels are shown ("Team" / "Active"), never raw enums.
 *   - The payment method is a [stub]: it renders a "managed in the Stripe portal"
 *     teaching state, NOT a fabricated brand/last4 card. The old
 *     `billing-brand`/`billing-last4` testids are gone by design.
 *   - The portal button (`billing-portal-btn`) calls `startBillingPortal`
 *     (POST /v1/customer/billing/portal) and then `window.location.assign`s to
 *     the returned URL — it no longer prints the raw URL in a `billing-portal-url`
 *     node. We intercept the offsite redirect to assert it happened.
 */
import { test, expect } from "../fixtures/test";
import { LoginPage } from "../pages/LoginPage";

// The E2E mock returns this fixed portal URL; intercept it so the browser
// doesn't actually navigate to the (invalid) offsite host.
const PORTAL_URL_RE = /^https:\/\/billing\.example\.invalid\/portal\//;

test.describe("customer billing", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("admin");
    await page.route(PORTAL_URL_RE, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "text/html",
        body: "<html><body data-testid='stripe-portal-stub'>stripe portal stub</body></html>",
      });
    });
  });

  test("subscription state renders and Manage subscription redirects to the portal", async ({
    page,
  }) => {
    await page.goto("/en/customer/billing");
    await expect(page.locator("h1#customer-billing-heading")).toBeVisible({ timeout: 30_000 });

    await expect(page.getByTestId("billing-plan")).toContainText("Team");
    await expect(page.getByTestId("billing-status")).toContainText("Active");
    await expect(page.getByTestId("billing-amount-due")).toContainText("$199.00");

    // Payment method is a teaching stub — managed in Stripe, never a fake card.
    await expect(page.getByTestId("billing-pm-managed")).toBeVisible();

    // Invoices render from the mock.
    await expect(page.getByTestId("billing-invoice-inv_2026_04")).toBeVisible();

    // Portal click → POST /v1/customer/billing/portal → redirect to the portal URL.
    const portalResp = page.waitForResponse(
      (r) =>
        r.url().endsWith("/v1/customer/billing/portal") && r.request().method() === "POST",
      { timeout: 10_000 },
    );
    await page.getByTestId("billing-portal-btn").click();
    await portalResp;
    await expect(page.getByTestId("stripe-portal-stub")).toBeVisible();
  });
});
