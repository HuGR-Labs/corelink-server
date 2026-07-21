import { test, expect } from "../fixtures/auth.js";

/**
 * REAL PAT-mint via the console — drives the create form end-to-end and captures
 * a live `corelink_…` token, then proves it authenticates. This is the honest
 * version of the keys journey (05 only asserted the page renders) and the
 * definitive answer to the runners-TL's "does create actually work + what gates
 * tenant-readiness" question.
 *
 * Key insight for the tenant-provision race (their 401): DON'T POST immediately
 * after sign-in — let the keys PAGE load first. Its list GET
 * (`GET /v1/customer/keys`) is the first authed hit and auto-provisions the
 * tenant; only then is create safe.
 */
test("console mints a real PAT that authenticates", async ({ authedPage: page }) => {
  const posts: string[] = [];
  page.on("response", (r) => {
    if (r.url().includes("/v1/customer/keys") && r.request().method() === "POST") {
      posts.push(`POST /v1/customer/keys → ${r.status()}`);
      // eslint-disable-next-line no-console
      console.log(`[keys-mint][net] POST /v1/customer/keys → ${r.status()}`);
    }
  });

  await page.goto("https://humangr.com/corelink/en/customer/keys");
  await page.waitForLoadState("networkidle").catch(() => {});
  // Wait for the list to settle — this GET auto-provisions the tenant.
  await expect(page.locator('[data-testid="keys-create"]')).toBeVisible({ timeout: 30_000 });
  await page.locator('[data-testid="keys-loading"]').waitFor({ state: "detached", timeout: 30_000 }).catch(() => {});

  await page.locator('[data-testid="keys-create-name"]').fill(`e2e-runner-${Date.now()}`);
  // Check a cache read scope (id form is `cache:r` etc.).
  const scope = page.locator('[data-testid^="keys-scope-cache:r"]').first();
  await scope.check().catch(async () => {
    // fallback: first available scope checkbox
    await page.locator('[data-testid^="keys-scope-"]').first().check();
  });

  // Create — retry through the tenant-provision race / a transient container
  // cold-start (container_start_threw is a per-DO wedge that clears on retry/roll).
  let token = "";
  for (let attempt = 1; attempt <= 4 && !token; attempt++) {
    await page.locator('[data-testid="keys-create-submit"]').click();
    // success → the plaintext reveal renders a `corelink_…` token somewhere.
    const revealed = await page
      .waitForFunction(() => /corelink_[a-z0-9]/.test(document.body.innerText), { timeout: 15_000 })
      .then(() => true)
      .catch(() => false);
    if (revealed) {
      const m = (await page.evaluate(() => document.body.innerText)).match(/corelink_[A-Za-z0-9_.]+/);
      token = m?.[0] ?? "";
    } else {
      // eslint-disable-next-line no-console
      console.log(`[keys-mint] attempt ${attempt}: no token yet (net=${posts.join("; ") || "none"}); retrying`);
      await page.waitForTimeout(3000);
    }
  }
  // eslint-disable-next-line no-console
  console.log(`[keys-mint] net=${posts.join("; ") || "none"} tokenLen=${token.length}`);
  expect(token, `console must mint a corelink_ token (net: ${posts.join("; ")})`).toMatch(/^corelink_/);

  // Prove the minted token AUTHENTICATES against prod (introspect via the app is
  // out; use a direct data-plane call — a valid token gets 403 scope, never 401).
  const authStatus = await page.evaluate(async (t: string) => {
    const h = await crypto.subtle.digest("SHA-256", new TextEncoder().encode("probe"));
    const hex = [...new Uint8Array(h)].map((b) => b.toString(16).padStart(2, "0")).join("");
    const r = await fetch(`https://corelink-api.humangr.com/v1/cas/${hex}`, {
      headers: { Authorization: `Bearer ${t}` },
    }).catch(() => null);
    return r ? r.status : -1;
  }, token);
  // eslint-disable-next-line no-console
  console.log(`[keys-mint] minted token → /v1/cas → ${authStatus} (403/404=authenticated ok, 401=bad)`);
  expect(authStatus, "the minted token must authenticate (not 401)").not.toBe(401);
});
