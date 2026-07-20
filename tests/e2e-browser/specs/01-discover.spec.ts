import { test, expect } from "../fixtures/auth.js";

/**
 * DISCOVERY (not a gate) — dumps the live app's structure for an authed user so
 * the behavior/money/DSR specs can use real selectors. Prints nav links,
 * headings, buttons per route, and which candidate routes resolve authed.
 */

const ROUTES = [
  "/corelink/dashboard",
  "/corelink/billing",
  "/corelink/settings",
  "/corelink/settings/billing",
  "/corelink/privacy",
  "/corelink/settings/privacy",
  "/corelink/pricing",
  "/corelink/upgrade",
  "/corelink/keys",
  "/corelink/usage",
];

test("discover app structure (authed)", async ({ authedPage: page }) => {
  for (const route of ROUTES) {
    let status = "?";
    try {
      const resp = await page.goto(route, { waitUntil: "domcontentloaded", timeout: 30_000 });
      status = String(resp?.status() ?? "?");
    } catch (e) {
      // eslint-disable-next-line no-console
      console.log(`\n### ${route} → NAV ERROR ${(e as Error).message.slice(0, 80)}`);
      continue;
    }
    await page.waitForLoadState("networkidle").catch(() => {});
    const finalUrl = page.url().replace("https://humangr.com", "");
    const bounced = finalUrl.includes("/sign-in");
    const info = await page.evaluate(() => {
      const txt = (el: Element) => (el.textContent || "").trim().replace(/\s+/g, " ").slice(0, 60);
      const headings = [...document.querySelectorAll("h1,h2,h3")].map(txt).filter(Boolean).slice(0, 8);
      const navLinks = [...document.querySelectorAll("nav a, aside a, a[href^='/corelink']")]
        .map((a) => `${txt(a)}→${(a as HTMLAnchorElement).getAttribute("href")}`)
        .filter((s) => !s.startsWith("→"))
        .slice(0, 25);
      const buttons = [...document.querySelectorAll("button, a[role='button'], [type='submit']")]
        .map(txt)
        .filter(Boolean)
        .slice(0, 20);
      const testids = [...document.querySelectorAll("[data-testid]")]
        .map((e) => e.getAttribute("data-testid"))
        .filter(Boolean)
        .slice(0, 25);
      return { headings, navLinks: [...new Set(navLinks)], buttons: [...new Set(buttons)], testids: [...new Set(testids)] };
    });
    // eslint-disable-next-line no-console
    console.log(`\n### ${route} → HTTP ${status} final=${finalUrl} bounced=${bounced}`);
    // eslint-disable-next-line no-console
    console.log(`  H: ${JSON.stringify(info.headings)}`);
    // eslint-disable-next-line no-console
    console.log(`  NAV: ${JSON.stringify(info.navLinks)}`);
    // eslint-disable-next-line no-console
    console.log(`  BTN: ${JSON.stringify(info.buttons)}`);
    // eslint-disable-next-line no-console
    console.log(`  TESTID: ${JSON.stringify(info.testids)}`);
  }
  expect(true).toBeTruthy();
});
