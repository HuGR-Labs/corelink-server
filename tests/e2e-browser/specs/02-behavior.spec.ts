import { test, expect } from "../fixtures/auth.js";
import { test as base } from "@playwright/test";

/**
 * FRONTEND BEHAVIOR — the app's auth-gating + authed render behave correctly
 * against PROD.
 */

// Unauthenticated access to an authed surface must bounce to sign-in.
base("unauthenticated → /customer bounces to /sign-in", async ({ page }) => {
  await page.goto("https://humangr.com/corelink/en/customer");
  await page.waitForLoadState("domcontentloaded");
  expect(page.url(), "an unauthenticated user must be sent to sign-in").toContain("/sign-in");
});

// A signed-in user reaches an authed surface (onboarding OR customer), never
// bounced back to /sign-in.
test("authenticated → reaches an authed surface (not sign-in)", async ({ authedPage: page }) => {
  await page.goto("https://humangr.com/corelink/en/customer");
  await page.waitForLoadState("networkidle").catch(() => {});
  const url = page.url();
  // eslint-disable-next-line no-console
  console.log(`[behavior] authed landing = ${url.replace("https://humangr.com", "")}`);
  expect(url, "a signed-in user must NOT be bounced to sign-in").not.toContain("/sign-in");
  // The page rendered *something* authed (a real heading, not a blank/error).
  const hasHeading = await page.locator("h1, h2").first().isVisible().catch(() => false);
  expect(hasHeading, "an authed page should render a heading").toBeTruthy();
});
