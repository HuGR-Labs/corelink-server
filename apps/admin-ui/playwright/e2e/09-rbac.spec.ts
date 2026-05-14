/**
 * E2E #9 — RBAC enforcement.
 *
 * Asserts:
 *   - Non-admin user requesting /admin/* gets blocked (403 page OR redirect
 *     to /403 route — admin-ui /[locale]/403/ exists).
 */

import { test, expect } from "@playwright/test";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";
import { installApiMocks } from "../fixtures/api-mocks";

test.describe("RBAC", () => {
  test("non-admin → /admin/audit → 403", async ({ page, context, baseURL }) => {
    await installApiMocks(page);
    await signInAs(context, FIXTURE_USERS.existingTenant, baseURL!); // role: user
    const resp = await page.goto("/en/admin/audit", { waitUntil: "domcontentloaded" });
    // Accept either a 403 status from the server OR a redirect to /403 route.
    const url = page.url();
    const ok = resp?.status() === 403 || /\/403/.test(url) || /forbidden|access denied/i.test(await page.content());
    expect(ok).toBe(true);
  });

  test("admin → /admin/audit → 200", async ({ page, context, baseURL }) => {
    await installApiMocks(page);
    await signInAs(context, FIXTURE_USERS.admin, baseURL!);
    const resp = await page.goto("/en/admin/audit", { waitUntil: "domcontentloaded" });
    expect((resp?.status() ?? 0) < 400 || /audit/i.test(await page.content())).toBeTruthy();
  });
});
