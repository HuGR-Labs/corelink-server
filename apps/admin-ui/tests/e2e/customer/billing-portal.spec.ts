/**
 * E2E — Stripe Customer Portal redirect flow (Linear BillingClient).
 *
 * Strategy: the real Stripe API is exercised by `corelink-stripe-real`'s
 * `--features live-integration` suite. Here we verify the **boundary contract**
 * of the live billing page — that clicking "Manage subscription"
 * (`billing-portal-btn`) POSTs to `/v1/customer/billing/portal`, treats the
 * response as a one-shot redirect (`window.location.assign`), and is NEVER
 * cached: a second click MUST trigger a second POST + redirect.
 *
 * We intercept the portal URL via `page.route()` so the browser doesn't
 * actually navigate offsite.
 *
 * Reconciliation notes (pre-Linear → current):
 *   - The page control is `billing-portal-btn` ("Manage subscription"), not the
 *     removed `PortalLauncher`'s `portal-open-button`.
 *   - The client (`startBillingPortal`) POSTs to `/v1/customer/billing/portal`
 *     with NO body — the old `{ return_url, tenant_id }` body contract belonged
 *     to the removed PortalLauncher/`/portal-session` path, so those assertions
 *     are dropped.
 *   - The E2E mock returns a FIXED portal URL, so per-click URL *uniqueness*
 *     cannot be asserted; we assert the no-cache invariant via the second POST
 *     firing (a fresh session is minted server-side on each real call).
 */

import { test, expect } from "../fixtures/test";
import { type Request as PWRequest } from "@playwright/test";
import { LoginPage } from "../pages/LoginPage";

// The E2E mock's portal URL host (fixed). Intercept so we don't navigate offsite.
const PORTAL_URL_RE = /^https:\/\/billing\.example\.invalid\/portal\//;

test.describe("customer billing portal redirect", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("admin");
    // Intercept the eventual portal redirect so we don't actually navigate
    // offsite (the mock host is non-routable by design).
    await page.route(PORTAL_URL_RE, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "text/html",
        body: "<html><body data-testid='stripe-portal-stub'>stripe portal stub</body></html>",
      });
    });
  });

  test("clicking Manage subscription POSTs to the portal endpoint then redirects — re-POSTs on every click (no cache)", async ({
    page,
  }) => {
    // Collect the POSTs we send to the portal endpoint.
    const sessionPosts: PWRequest[] = [];
    page.on("request", (req) => {
      if (
        req.method() === "POST" &&
        req.url().endsWith("/v1/customer/billing/portal")
      ) {
        sessionPosts.push(req);
      }
    });

    await page.goto("/en/customer/billing");
    await expect(page.locator("h1#customer-billing-heading")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId("billing-portal-btn")).toBeEnabled();

    // --- First click: triggers POST + redirect to the portal stub. ---------
    await page.getByTestId("billing-portal-btn").click();
    await expect(page.getByTestId("stripe-portal-stub")).toBeVisible();
    expect(sessionPosts.length).toBe(1);

    const firstPortalUrl = page.url();
    expect(firstPortalUrl).toMatch(PORTAL_URL_RE);

    // --- Second click: navigate back, click again — MUST re-fetch. ---------
    await page.goto("/en/customer/billing");
    await expect(page.getByTestId("billing-portal-btn")).toBeEnabled();
    await page.getByTestId("billing-portal-btn").click();
    await expect(page.getByTestId("stripe-portal-stub")).toBeVisible();

    // No-cache invariant: the second click issued a fresh POST (the server mints
    // a new single-use session on each call; the E2E mock's URL is fixed, so we
    // assert the re-POST rather than URL uniqueness here).
    expect(sessionPosts.length).toBe(2);
    expect(page.url()).toMatch(PORTAL_URL_RE);
  });
});
