#!/usr/bin/env node
/**
 * admin-ui render smoke — the test that would have caught the 2026-06-14 prod
 * login outage.
 *
 * The existing `e2e-clerk-signup` cron exercises the BACKEND signup orchestration
 * (Clerk API -> signup-worker -> D1 tenant -> PAT -> publicMetadata -> API auth)
 * but NEVER loads the actual auth pages in a browser, so it stayed green while
 * /sign-in and /sign-up crashed client-side ("Application error: a client-side
 * exception"). An HTTP-200 check cannot catch a client-side exception — the HTML
 * 200s, the JS throws on hydration.
 *
 * This loads the real pages in headless Chromium and FAILS on:
 *   - any uncaught page error (pageerror) — the exact signal of the outage
 *   - any failed/4xx/5xx JS chunk request (e.g. the rate-limit 429 / err 1015)
 *   - the Clerk widget not rendering on /sign-in and /sign-up
 *
 * Usage:  node scripts/e2e-admin-ui-render-smoke.mjs [BASE_URL]
 *   BASE_URL defaults to https://corelink-app.humangr.com (env BASE_URL overrides).
 * Exit 0 = all pages healthy; exit 1 = at least one failure (page + cause printed).
 *
 * Requires Playwright. In CI install once: `npx playwright install --with-deps chromium`.
 * On the self-hosted Mac the chromium-headless-shell is already cached.
 */
import { chromium } from "playwright";

const BASE = process.env.BASE_URL || process.argv[2] || "https://corelink-app.humangr.com";

/** Each target: path + whether the Clerk auth widget must render. */
const TARGETS = [
  { path: "/", clerk: false },
  { path: "/sign-in", clerk: true },
  { path: "/sign-up", clerk: true },
];

const CLERK_SELECTOR =
  '.cl-rootBox, .cl-card, input[name="identifier"], input[type="email"], iframe[src*="challenges.cloudflare.com"]';

async function checkPage(browser, target) {
  const ctx = await browser.newContext();
  const page = await ctx.newPage();
  const errors = [];
  page.on("pageerror", (e) => errors.push(`pageerror: ${e.message.split("\n")[0]}`));
  page.on("response", (r) => {
    const u = r.url();
    if (r.status() >= 400 && /\/_next\/static\/|\.js(\?|$)/.test(u)) {
      errors.push(`asset ${r.status()}: ${u.replace(BASE, "")}`);
    }
  });

  const url = `${BASE}${target.path}`;
  try {
    // `domcontentloaded` (not `networkidle`): pages with a live analytics beacon
    // or other long-lived connection never reach network-idle, which would flag a
    // perfectly healthy page. A client-side exception still surfaces via the
    // `pageerror` listener + the error-screen text check after the settle wait.
    await page.goto(url, { waitUntil: "domcontentloaded", timeout: 30000 });
  } catch (e) {
    errors.push(`navigation: ${e.message.split("\n")[0]}`);
  }
  await page.waitForTimeout(5000);

  const bodyText = (await page.evaluate(() => document.body?.innerText || "")).slice(0, 200);
  if (/Application error|client-side exception/i.test(bodyText)) {
    errors.push(`Next.js error screen rendered: "${bodyText.replace(/\s+/g, " ").trim()}"`);
  }

  let widgetOk = true;
  if (target.clerk) {
    widgetOk = await page.evaluate(
      (sel) => !!document.querySelector(sel),
      CLERK_SELECTOR,
    );
    if (!widgetOk) errors.push("Clerk auth widget did not render");
  }

  await ctx.close();
  return { url, ok: errors.length === 0, errors };
}

// PW_EXECUTABLE_PATH lets callers pin a specific Chromium build (e.g. the
// self-hosted Mac's cached chrome-headless-shell); CI uses the default after
// `npx playwright install chromium`.
const launchOpts = process.env.PW_EXECUTABLE_PATH
  ? { executablePath: process.env.PW_EXECUTABLE_PATH }
  : {};
const browser = await chromium.launch(launchOpts);
let failed = 0;
for (const target of TARGETS) {
  const r = await checkPage(browser, target);
  if (r.ok) {
    console.log(`[PASS] ${r.url}`);
  } else {
    failed++;
    console.log(`[FAIL] ${r.url}`);
    for (const e of r.errors) console.log(`         - ${e}`);
  }
}
await browser.close();

if (failed > 0) {
  console.log(`\n${failed}/${TARGETS.length} page(s) FAILED render smoke against ${BASE}`);
  process.exit(1);
}
console.log(`\nAll ${TARGETS.length} pages rendered cleanly against ${BASE}`);
