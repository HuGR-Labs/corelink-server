/**
 * authenticated-smoke.spec.ts — layer 2 of the public-flip smoke harness.
 *
 * Drives a REAL browser Clerk session against LIVE production
 * (https://corelink-app.humangr.com) with a dedicated low-privilege smoke
 * user. This is the layer every prior smoke lacked: the azp-incident
 * (2026-06-10) — `401 clerk session azp invalid` minted only by tokens from
 * a real Clerk session on the user-facing host — was invisible to curl
 * probes and to localhost e2e runs. This spec asserts SPECIFICALLY on that
 * failure mode (see `azp401s` collector below) in addition to the happy
 * path.
 *
 * STANDALONE on purpose: own package.json/tsconfig/playwright.config in
 * scripts/smoke/ — NOT wired into apps/admin-ui's e2e configs, so it can
 * run from a clean checkout with only `npm install && npx playwright
 * install chromium` and two env vars (no lockfile committed — the repo's
 * gitignore bans package-lock.json repo-wide; pnpm owns the workspaces).
 *
 * Required env (CTRL-CRED-001 — never hardcoded, never logged):
 *   SMOKE_USER_EMAIL     dedicated smoke-user email (provisioned tenant,
 *                        DPA accepted — see README "Test user contract")
 *   SMOKE_USER_PASSWORD  its password
 * Optional env:
 *   SMOKE_BASE_URL       app origin override (default
 *                        https://corelink-app.humangr.com)
 *
 * Stripe note: the checkout test asserts that POST /api/checkout/session
 * returns a Stripe-hosted checkout URL. It NEVER pays — navigation to
 * checkout.stripe.com is aborted via page.route(). Against a Stripe
 * TEST-mode backend the URL is still minted; against LIVE mode no charge
 * can occur because the Stripe page is never even loaded.
 */

import {
  test,
  expect,
  type Browser,
  type BrowserContext,
  type Page,
} from "@playwright/test";

const BASE_URL = process.env["SMOKE_BASE_URL"] ?? "https://corelink-app.humangr.com";

// Fail loud, fail early: a smoke run without credentials must be a hard
// error, never a silent green. Values are intentionally NOT echoed.
function requiredEnv(name: string): string {
  const v = process.env[name];
  if (!v) {
    throw new Error(
      `${name} is required (dedicated smoke user; see scripts/smoke/README.md). ` +
        "Never hardcode it; export it in the shell that runs this spec.",
    );
  }
  return v;
}

/** 401 responses whose body matches the azp-incident signature. */
interface Azp401 {
  url: string;
  body: string;
}

/** All 5xx responses observed (any host) — the dashboard-wave assertions. */
interface Server5xx {
  url: string;
  status: number;
}

test.describe.serial("public-flip authenticated smoke (live prod)", () => {
  let context: BrowserContext;
  let page: Page;
  const azp401s: Azp401[] = [];
  const server5xxs: Server5xx[] = [];

  async function signIn(browser: Browser): Promise<void> {
    context = await browser.newContext();
    page = await context.newPage();

    // Network watcher — runs for the WHOLE session. Captures:
    //  * any 401 whose body carries the azp-incident signature
    //    ("clerk session azp invalid", worker/src/index.ts M1-FIX-1),
    //  * any 5xx from any host.
    page.on("response", (res) => {
      const status = res.status();
      if (status === 401) {
        void res
          .text()
          .then((body) => {
            if (/azp invalid|azp.*not.*allow/i.test(body)) {
              azp401s.push({ url: res.url(), body: body.slice(0, 300) });
            }
          })
          .catch(() => {
            /* body may be unavailable after navigation — ignore */
          });
      }
      if (status >= 500) {
        server5xxs.push({ url: res.url(), status });
      }
    });

    // NEVER let the browser reach Stripe's checkout page — assert-only,
    // no payment surface is ever rendered.
    await page.route("https://checkout.stripe.com/**", (route) => route.abort());

    const email = requiredEnv("SMOKE_USER_EMAIL");
    const password = requiredEnv("SMOKE_USER_PASSWORD");

    await page.goto(`${BASE_URL}/sign-in`, { waitUntil: "domcontentloaded" });

    // Clerk's <SignIn /> widget (dynamic-imported, ssr:false — see
    // apps/admin-ui/src/app/sign-in/[[...sign-in]]/page.tsx). Identifier
    // first; password either on the same screen or after "Continue".
    const identifier = page.locator('input[name="identifier"]');
    await identifier.waitFor({ state: "visible", timeout: 30_000 });
    await identifier.fill(email);

    const passwordField = page.locator('input[name="password"]');
    if (!(await passwordField.isVisible())) {
      await page.getByRole("button", { name: /^continue$/i }).click();
      await passwordField.waitFor({ state: "visible", timeout: 30_000 });
    }
    await passwordField.fill(password);
    await page.getByRole("button", { name: /^continue$/i }).click();

    // Session established ⇔ we leave /sign-in (Clerk redirects to the
    // app; exact landing page is not this assertion's business).
    await page.waitForURL((url) => !url.pathname.includes("/sign-in"), {
      timeout: 60_000,
    });
  }

  test.beforeAll(async ({ browser }) => {
    await signIn(browser);
  });

  test.afterAll(async () => {
    await context?.close();
  });

  test("sign-in establishes a real Clerk session on the public host", async () => {
    // Reaching here means waitForURL left /sign-in. Belt-and-braces: the
    // Clerk session cookie must exist on the app origin.
    const cookies = await context.cookies(BASE_URL);
    expect(
      cookies.some((c) => c.name === "__session" || c.name.startsWith("__session_")),
      "expected a Clerk __session cookie on the app origin after sign-in",
    ).toBe(true);
  });

  test("welcome/provisioning completes — and NO `401 azp invalid` anywhere", async () => {
    await page.goto(`${BASE_URL}/en/welcome`, { waitUntil: "domcontentloaded" });

    // Branch 3 of welcome/page.tsx redirects to /sign-up when the session
    // has no tenant_id — i.e. provisioning failed or the session is not
    // actually honoured by the server. That is a FAIL here.
    expect(
      page.url(),
      "welcome bounced to /sign-up — tenant not provisioned or session not honoured",
    ).not.toMatch(/\/sign-up/);

    // Either welcome branch is a pass: one-time PAT reveal (branch 1) or
    // "already retrieved" (branch 2 — the steady state for the smoke user).
    await expect(
      page
        .getByTestId("welcome-already-retrieved")
        .or(page.getByRole("heading", { name: /welcome to corelink/i })),
    ).toBeVisible({ timeout: 30_000 });

    // Give the SSE stream + metadata fetches a beat to fire, then assert
    // the azp-incident signature never appeared on the wire. This is THE
    // regression assertion for the 2026-06-10 incident.
    await page.waitForTimeout(3_000);
    expect(
      azp401s,
      `azp-incident regression: 401 'azp invalid' observed: ${JSON.stringify(azp401s)}`,
    ).toEqual([]);
  });

  test("/upgrade?plan=solo mints a Stripe checkout session (no payment)", async () => {
    // The page auto-fires POST /api/checkout/session on mount
    // (<UpgradeButton autoStart /> — apps/admin-ui/src/components/UpgradeButton.tsx).
    const checkoutResponse = page.waitForResponse(
      (res) =>
        res.url().includes("/api/checkout/session") &&
        res.request().method() === "POST",
      { timeout: 60_000 },
    );
    await page.goto(`${BASE_URL}/upgrade?plan=solo`, {
      waitUntil: "domcontentloaded",
    });

    // Locale-less forwarder must land us on the canonical page, signed-in
    // (NOT round-tripped to /sign-in — that would mean the session is not
    // honoured on the user-facing host: the azp incident's sibling).
    await expect(page).toHaveURL(/\/en\/upgrade\?plan=solo/);
    await expect(page.getByTestId("upgrade-root")).toBeVisible({ timeout: 30_000 });

    const res = await checkoutResponse;
    expect(
      res.status(),
      "checkout-session creation must succeed (403 = DPA not accepted for the smoke user; fix the test-user contract)",
    ).toBe(200);
    const body = (await res.json()) as { checkout_url?: string; session_id?: string };
    expect(
      body.checkout_url,
      "backend must return a Stripe-hosted checkout URL",
    ).toMatch(/^https:\/\/checkout\.stripe\.com\//);
    expect(body.session_id, "session_id must be recorded for webhook activation").toBeTruthy();
    // Navigation to checkout.stripe.com is aborted by the page.route()
    // installed in signIn() — we never load Stripe, we never pay.

    expect(azp401s, "no `401 azp invalid` during checkout").toEqual([]);
  });

  // ── Dashboard wave — NOT SHIPPED YET ────────────────────────────────────
  // TODO(dashboard-wave): un-skip when the customer dashboard tabs ship.
  // Contract: each tab must render for the smoke user with zero 5xx on the
  // wire (the `server5xxs` collector is already recording — just assert).
  for (const tab of ["usage", "keys", "billing", "audit"]) {
    test(`dashboard tab /en/customer/${tab} loads without 5xx`, async () => {
      test.skip(true, "TODO(dashboard-wave): dashboard tabs not shipped yet");
      await page.goto(`${BASE_URL}/en/customer/${tab}`, {
        waitUntil: "domcontentloaded",
      });
      expect(page.url()).not.toMatch(/\/sign-in/);
      expect(
        server5xxs,
        `5xx responses observed loading /en/customer/${tab}: ${JSON.stringify(server5xxs)}`,
      ).toEqual([]);
    });
  }

  test("session produced zero 5xx across the whole flow", async () => {
    expect(
      server5xxs,
      `5xx responses observed during the smoke session: ${JSON.stringify(server5xxs)}`,
    ).toEqual([]);
  });
});
