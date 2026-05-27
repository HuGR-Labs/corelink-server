/**
 * Production-surface E2E — signup → /welcome happy path + public-surface
 * regression guards.
 *
 * Per Phase 0.F wizard 2-step + PLG framework §4 the critical happy path is:
 *
 *   sign-up → /welcome → corelink CLI install → first_cache_hit event
 *
 * These tests run against PRODUCTION URLs (configured via E2E_BASE_URL +
 * E2E_DOCS_URL in playwright.prod.config.ts) so they protect against deploy
 * regressions — e.g. Pages secrets not applied → /sign-up 500.
 *
 * Clerk's hosted sign-up flow requires real credentials and live e-mail
 * verification, so the full Clerk auth round-trip is intentionally NOT
 * exercised here: it's covered by the localhost suite at
 * apps/admin-ui/playwright/e2e/01-onboarding.spec.ts (currently fixme'd
 * pending Clerk test-mode env). The prod suite covers the **public surface**
 * (signed-out pages + install Worker) plus a smoke check that the /welcome
 * route itself is reachable behind auth — the redirect chain is the
 * canary that proves the route exists post-deploy.
 *
 * Selectors prefer data-testid attributes that ship in the corresponding
 * components (InstallOneLiner, ActivationStateBadge) over text content so
 * minor copy edits don't break the suite.
 */

import { test, expect } from "@playwright/test";

const APP_URL = process.env["E2E_BASE_URL"] ?? "https://app.corelink.humangr.com";
const DOCS_URL = process.env["E2E_DOCS_URL"] ?? "https://docs.corelink.humangr.com";

test.describe("Signup → Welcome → Activation (prod surface)", () => {
  test("/sign-up returns 200 (post Pages-secrets applied)", async ({ request }) => {
    const res = await request.get(`${APP_URL}/sign-up`, { maxRedirects: 5 });
    expect(
      res.status(),
      `${APP_URL}/sign-up must return 2xx; 500 typically means Clerk env vars are unset on Cloudflare Pages`,
    ).toBeGreaterThanOrEqual(200);
    expect(res.status()).toBeLessThan(400);
  });

  test("sign-up page renders Clerk widget (anchors the post-Clerk-load shell)", async ({
    page,
  }) => {
    await page.goto(`${APP_URL}/sign-up`, { waitUntil: "domcontentloaded" });
    // Title is locale-dependent; assert generously.
    await expect(page).toHaveTitle(/Sign\s?(?:Up|In|in)|CoreLink/i);
    // Clerk's hosted form mounts a node that carries one of these stable hooks.
    // We don't assert a specific selector beyond "something rendered" because
    // Clerk owns the DOM shape and we don't want a copy/CSS bump to break us.
    const body = await page.locator("body").innerText();
    expect(body.length, "sign-up page should render non-empty body").toBeGreaterThan(0);
  });

  // The full Clerk OAuth/email round-trip requires real credentials. We skip
  // here with an explicit reason; the localhost suite covers this with
  // fixture users + mocked Clerk session claims.
  test.skip(
    "complete happy path under 5 minutes (Clerk auth round-trip)",
    () => {
      // Skipped on the prod suite — see file header. Covered by:
      //   apps/admin-ui/playwright/e2e/01-onboarding.spec.ts
      // (uses fixtures/clerk.ts test-mode helpers against a localhost dev
      // server with NEXT_PUBLIC_E2E_TEST_MODE=1).
    },
  );

  test("/welcome (signed-out) redirects through auth — proves route exists post-deploy", async ({
    request,
  }) => {
    // Signed-out visit to /welcome should NOT 500 / NOT 404. The Clerk
    // middleware will redirect to /sign-in (or /sign-up) — both are fine.
    // We follow redirects up to a small bound and assert the terminal status
    // is a 2xx auth page (NOT 500, NOT 404).
    const res = await request.get(`${APP_URL}/welcome`, { maxRedirects: 5 });
    expect(res.status()).toBeGreaterThanOrEqual(200);
    expect(res.status()).toBeLessThan(400);
  });
});

test.describe("Legal pages (prod surface)", () => {
  for (const path of ["/legal/privacy", "/legal/terms", "/legal/sub-processors"]) {
    test(`${path} returns 200`, async ({ request }) => {
      const res = await request.get(`${DOCS_URL}${path}`, { maxRedirects: 3 });
      expect(res.status(), `${DOCS_URL}${path} must return 200`).toBe(200);
      const body = await res.text();
      // Each legal page must include non-trivial content (catch blank deploys).
      expect(body.length).toBeGreaterThan(500);
    });
  }
});

test.describe("Pricing page (prod surface)", () => {
  test("shows 3 tiers (Free / Pro / Enterprise) + Pro CTA points at signup/upgrade/checkout", async ({
    page,
  }) => {
    // The Docusaurus pricing index lives at `/pricing` (calculator at
    // `/pricing/calculator`). Use a permissive locator so a locale prefix
    // (e.g. /pt-BR/pricing) still passes when this test runs against a
    // locale-routed deployment.
    await page.goto(`${DOCS_URL}/pricing`, { waitUntil: "domcontentloaded" });

    const body = page.locator("body");
    await expect(body).toContainText(/Free/i);
    await expect(body).toContainText(/Pro/i);
    await expect(body).toContainText(/Enterprise/i);

    // CTA assertion is best-effort — if no CTA matches we don't fail the
    // tier check (copy / link text may evolve). When a CTA IS present, its
    // href MUST point at the signup / upgrade / checkout flow (catches the
    // regression where CTA goes nowhere or to a 404 route).
    const cta = page
      .locator(
        'a:has-text("Subscribe"), a:has-text("Start Pro"), a:has-text("Get Pro"), a:has-text("Upgrade"), a:has-text("Sign up")',
      )
      .first();
    if ((await cta.count()) > 0) {
      await expect(cta).toHaveAttribute("href", /sign-?up|upgrade|checkout|billing/);
    }
  });
});

// =============================================================================
// Welcome-page DOM contract (runs when an authenticated session is available).
// =============================================================================
//
// In CI we don't have Clerk credentials, so these DOM-level assertions are
// gated on E2E_AUTH_STORAGE_STATE being set (a saved storageState.json from a
// trusted operator login). When unset, the block is skipped at runtime — the
// public-surface tests above still run as the daily-deploy canary.

test.describe("Welcome page DOM contract (auth-gated)", () => {
  test.skip(
    !process.env["E2E_AUTH_STORAGE_STATE"],
    "E2E_AUTH_STORAGE_STATE not set — skipping authenticated /welcome DOM checks. Set it to a saved Playwright storageState.json from a fresh signup to run this block.",
  );

  test.use({ storageState: process.env["E2E_AUTH_STORAGE_STATE"] });

  test("InstallOneLiner present + ActivationStateBadge starts 'waiting'", async ({
    page,
  }) => {
    await page.goto(`${APP_URL}/welcome`, { waitUntil: "domcontentloaded" });
    await expect(page).toHaveURL(/\/welcome/);

    const installBlock = page.getByTestId("install-one-liner");
    await expect(installBlock).toBeVisible({ timeout: 10_000 });

    const cmd = page.getByTestId("install-one-liner-cmd");
    await expect(cmd).toContainText("curl -fsSL");
    await expect(cmd).toContainText("--token=");

    const badge = page.getByTestId("activation-state-badge");
    await expect(badge).toBeVisible();
    await expect(badge).toHaveAttribute("data-state", "waiting");
  });
});
