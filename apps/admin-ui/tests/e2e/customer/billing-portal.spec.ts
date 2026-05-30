/**
 * E2E — Stripe Customer Portal redirect flow (wt/r-prep-stripe-portal).
 *
 * Strategy: the real Stripe API is exercised by `corelink-stripe-real`'s
 * `--features live-integration` suite. Here we verify the **boundary
 * contract** — that the customer billing page POSTs to the right
 * endpoint with `{ return_url, tenant_id }`, that the server response
 * is treated as a one-shot redirect, and that the portal URL is
 * NEVER cached (a second click MUST trigger a second POST).
 *
 * We intercept the Stripe-portal URL itself via `page.route()` so the
 * browser doesn't actually navigate to billing.stripe.com — that would
 * be flaky AND would burn real Stripe quota on test traffic.
 */

import { test, expect, type Request as PWRequest } from "@playwright/test";
import { LoginPage } from "../pages/LoginPage";

const PORTAL_URL_RE = /^https:\/\/billing\.stripe\.com\/p\/session\//;

test.describe("customer billing portal redirect", () => {
  test.beforeEach(async ({ page, context, baseURL }) => {
    const login = new LoginPage(page, context, baseURL!);
    await login.signInAs("admin");
    // Intercept the eventual Stripe redirect so we don't actually
    // navigate offsite. We mark the request as captured for assertions.
    await page.route(PORTAL_URL_RE, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "text/html",
        body: "<html><body data-testid='stripe-portal-stub'>stripe portal stub</body></html>",
      });
    });
  });

  test("clicking Open Stripe Portal POSTs return_url+tenant_id then redirects to a fresh portal URL each time", async ({
    page,
  }) => {
    // Collect the POSTs we send to the session endpoint.
    const sessionPosts: PWRequest[] = [];
    page.on("request", (req) => {
      if (
        req.method() === "POST" &&
        req.url().endsWith("/api/v1/customer/billing/portal-session")
      ) {
        sessionPosts.push(req);
      }
    });

    await page.goto("/en/customer/billing");
    await expect(page.locator("h1#customer-billing-heading")).toBeVisible();
    await expect(page.getByTestId("portal-open-button")).toBeEnabled();

    // --- First click: triggers POST + redirect to Stripe-stub. -------
    await page.getByTestId("portal-open-button").click();
    await expect(page.getByTestId("stripe-portal-stub")).toBeVisible();
    expect(sessionPosts.length).toBe(1);

    // Validate the POST body contract.
    const firstBody = JSON.parse(sessionPosts[0]!.postData() ?? "{}") as {
      return_url?: string;
      tenant_id?: string;
    };
    expect(firstBody.return_url).toMatch(/^https?:\/\/.+\/en\/customer\/billing$/);
    expect(firstBody.tenant_id).toBeTruthy();

    // The browser is now on the stub URL — capture it for uniqueness check.
    const firstStripeUrl = page.url();
    expect(firstStripeUrl).toMatch(PORTAL_URL_RE);

    // --- Second click: navigate back, click again — MUST re-fetch. ---
    await page.goto("/en/customer/billing");
    await expect(page.getByTestId("portal-open-button")).toBeEnabled();
    await page.getByTestId("portal-open-button").click();
    await expect(page.getByTestId("stripe-portal-stub")).toBeVisible();

    expect(sessionPosts.length).toBe(2);
    const secondStripeUrl = page.url();
    expect(secondStripeUrl).toMatch(PORTAL_URL_RE);
    expect(secondStripeUrl).not.toEqual(firstStripeUrl);
  });
});
