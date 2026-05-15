/**
 * E2E 1/5 — auth: SSO login + session cookie + logout (wt-r3-7).
 *
 * Strategy: real Clerk SSO is exercised by Clerk's own SDK tests; here we
 * verify the *boundary contract* — that protected admin routes reject the
 * unauthenticated browser, that a synthetic Clerk session cookie unlocks
 * them, and that clearing the cookie restores the 403 panel.
 */
import { test, expect } from "@playwright/test";
import { LoginPage } from "./pages/LoginPage";

test.describe("auth: SSO + session cookie + logout", () => {
  test("unauthenticated → 403 panel; signed-in → admin shell; signed-out → 403 again", async ({
    page,
    context,
    baseURL,
  }) => {
    const login = new LoginPage(page, context, baseURL!);

    // 1. Unauthenticated direct access to a protected admin route renders the
    //    in-page 403 Forbidden panel (RbacGuard fallback).
    await page.goto("/en/admin/tenants");
    await expect(page.getByTestId("rbac-forbidden")).toBeVisible();
    await login.expectNoSessionCookie();

    // 2. The /sign-in route renders without leaking session.
    await login.visit();
    await expect(page.locator("h1")).toContainText(/sign[- ]?in/i);

    // 3. Install the synthetic Clerk session → admin shell renders.
    await login.signInAs("admin");
    await login.expectSessionCookie();
    await page.goto("/en/admin/tenants");
    await expect(page.locator("h1#tenants-heading")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByTestId("rbac-forbidden")).toHaveCount(0);

    // 4. Logout clears the cookie → admin route bounces back to 403.
    await login.signOut();
    await login.expectNoSessionCookie();
    await page.goto("/en/admin/tenants");
    await expect(page.getByTestId("rbac-forbidden")).toBeVisible();
  });
});
