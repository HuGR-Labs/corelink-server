import { test, expect } from "../fixtures/auth.js";

/**
 * Verifies the self-serve PAT-mint console EXISTS + renders authed (answers the
 * runners-TL ask: they probed /corelink/keys → marketing SPA; the real surface
 * is /corelink/en/customer/keys under the (authenticated) route group).
 */
test("customer keys console renders a real PAT surface (not marketing)", async ({
  authedPage: page,
}) => {
  await page.goto("https://humangr.com/corelink/en/customer/keys");
  await page.waitForLoadState("networkidle").catch(() => {});
  const url = page.url().replace("https://humangr.com", "");
  const body = await page.evaluate(() => document.body.innerText.slice(0, 400));
  // eslint-disable-next-line no-console
  console.log(`[keys] url=${url}\n[keys] body="${body.replace(/\s+/g, " ").slice(0, 200)}"`);
  expect(url, "keys console must not bounce to sign-in").not.toContain("/sign-in");
  // Real PAT surface, NOT the marketing SPA ("Play the journey" / "01 Cache").
  const isMarketing = /Play the journey|01 Cache|pure function of its inputs/i.test(body);
  expect(isMarketing, "must render the PAT console, not the marketing SPA").toBeFalsy();
  const mentionsPat = /personal access token|\bPAT\b|api key|create.*key|token/i.test(body);
  expect(mentionsPat, "keys console should mention PAT/tokens").toBeTruthy();
});
