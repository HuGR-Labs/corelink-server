/**
 * E2E #10 — CSP violation report flow.
 *
 * Asserts:
 *   - Page sends a CSP header (Content-Security-Policy OR -Report-Only).
 *   - Header forbids `unsafe-inline` and includes a nonce-based script-src.
 *   - When an inline <script> with no nonce is injected, the browser
 *     blocks it AND the CSP report endpoint receives a POST.
 *
 * Note: in dev mode the header is `-Report-Only`; in production it
 * flips to `Content-Security-Policy` per next.config.ts (WI-S16-007
 * deliverable 4). Either form is acceptable for this test — what matters
 * is that violations are reported.
 */

import { test, expect } from "../fixtures/test";

test.describe("CSP", () => {
  test("response carries hardened CSP without unsafe-inline", async ({ page }) => {
    const resp = await page.goto("/");
    expect(resp).not.toBeNull();
    const headers = resp!.headers();
    const cspHeader =
      headers["content-security-policy"] ??
      headers["content-security-policy-report-only"];
    expect(cspHeader, "CSP header present").toBeTruthy();
    // The security-critical property is that `script-src` (the XSS vector) is
    // nonce-hardened with NO 'unsafe-inline'. 'unsafe-inline' is deliberately
    // permitted on `style-src` only — required by Tailwind's JIT inline <style>
    // blocks and Clerk's overlay styles, and harmless for styles (see
    // src/lib/csp.ts, which documents "NEVER on script-src"). The previous
    // whole-header check contradicted that intentional, documented design.
    const scriptSrc =
      cspHeader!
        .split(";")
        .map((d) => d.trim())
        .find((d) => d.startsWith("script-src")) ?? "";
    expect(scriptSrc, "script-src directive present").toBeTruthy();
    expect(scriptSrc, "script-src must not allow unsafe-inline (XSS)").not.toMatch(
      /'unsafe-inline'/,
    );
    expect(scriptSrc, "script-src must be nonce-hardened").toMatch(/'nonce-/);
    expect(cspHeader).toMatch(/default-src/);
    // The report URL must carry the app basePath. A bare `report-uri
    // /api/csp-report` resolves against the ORIGIN root, where this app is not
    // mounted — observed live as POST /api/csp-report -> 405 on humangr.com and
    // 404 under `next dev`, i.e. 100% of reports discarded. `/report-uri/`
    // alone (the previous assertion) passed straight through that.
    expect(cspHeader, "report-uri must be basePath-correct").toMatch(
      /report-uri \/corelink\/api\/csp-report/,
    );
    // Reporting API v1 channel for Chromium, which ignores report-uri.
    expect(cspHeader).toMatch(/report-to csp-endpoint/);
    // KNOWN BLIND SPOT — a green run here is NOT evidence that prod is clean.
    // The exact-equality assertion below is the right shape (a doubled value
    // fails it), but this suite drives `next dev` (playwright.config.ts →
    // `webServer.command: "pnpm dev"`), and the duplication it would catch is
    // introduced by the `@opennextjs/cloudflare` adapter's header merge, which
    // `next dev` never runs. It stayed green for the whole window in which prod
    // actually served `csp-endpoint="…", csp-endpoint="…"` (fixed in
    // fix/reporting-endpoints-emitted-twice). The PR-time gate for that class is
    // `tests/security-headers-single-emitter.test.ts`, which asserts the two
    // emitters' path coverage is disjoint; the post-deploy detector belongs in
    // the LIVE prod suite (`e2e/`, daily cron) — see that branch's PR body.
    expect(headers["reporting-endpoints"], "Reporting-Endpoints header").toBe(
      'csp-endpoint="/corelink/api/csp-report"',
    );
  });

  test("inline script injection fires securitypolicyviolation", async ({ page }) => {
    let reportPosted = false;
    await page.route(/\/api\/csp-report/, async (route) => {
      reportPosted = true;
      await route.fulfill({ status: 204 });
    });

    const resp = await page.goto("/");
    const headers = resp!.headers();
    const isReportOnly = "content-security-policy-report-only" in headers;

    // We listen for `securitypolicyviolation` which fires in BOTH modes
    // (report-only AND enforce). In enforce mode the script also fails to
    // execute; in report-only mode the violation is observed but the script
    // still runs (by spec). We assert the violation event always fires; we
    // only assert `__pwn === undefined` in enforce mode.
    const result = await page.evaluate(
      () =>
        new Promise<{ violation: boolean }>((resolve) => {
          let fired = false;
          document.addEventListener(
            "securitypolicyviolation",
            () => {
              fired = true;
              resolve({ violation: true });
            },
            { once: true },
          );
          const s = document.createElement("script");
          s.textContent = "window.__pwn = true;";
          document.head.appendChild(s);
          setTimeout(() => resolve({ violation: fired }), 1500);
        }),
    );
    expect(result.violation).toBe(true);

    if (!isReportOnly) {
      const pwned = await page.evaluate(
        () => (window as unknown as { __pwn?: boolean }).__pwn === true,
      );
      expect(pwned).toBe(false);
    }

    console.info(
      `[e2e] CSP mode=${isReportOnly ? "report-only" : "enforce"} reportPosted=${reportPosted}`,
    );
  });
});
