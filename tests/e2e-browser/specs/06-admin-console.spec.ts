import { test, expect } from "../fixtures/auth.js";

/**
 * ADMIN / operator surface — coverage of its SECURITY POSTURE, which is the
 * testable part as a real user.
 *
 * Two findings established live (2026-07-19):
 *   1. The operator CONSOLE (`/corelink/en/admin/*`) is RBAC-gated on the Clerk
 *      org role `corelink-admin` (apps/admin-ui/src/lib/auth.ts). But **Clerk
 *      Organizations are DISABLED on the CoreLink instance** (Backend API
 *      `POST /organizations` → 403 `organization_not_enabled_in_instance`), so
 *      no real user can hold `corelink-admin` → every signed-in user resolves to
 *      `corelink-member` and the console is unreachable via the browser. Admin is
 *      OPERATOR-ONLY by design.
 *   2. The operator API (`/v1/admin/read|mutate|approve`, `/v1/admin/audit/events`)
 *      is gated by the internal keys `CORELINK_ADMIN_AUTH_KEY` (mutate) +
 *      a DISTINCT `CORELINK_ADMIN_APPROVER_AUTH_KEY` (H5 two-person control), NOT
 *      a customer PAT. So a real customer credential is rejected, a client cannot
 *      forge its way in with an `x-*-scope` header (the Worker strips them), and
 *      anon is rejected. Driving the read/mutate/dual-approve flow itself needs
 *      the operator keys (owner-held, OOB).
 *
 * This spec proves the RBAC lockdown a real user hits. (When Clerk Orgs are
 * enabled + the operator keys are ferried OOB, add the positive admin-drive.)
 */

test("RBAC: a customer is denied the operator CONSOLE (org role required, orgs disabled)", async ({
  authedPage: page,
}) => {
  await page.goto("https://humangr.com/corelink/en/admin/tenants", { waitUntil: "domcontentloaded" });
  await page.waitForLoadState("networkidle").catch(() => {});
  const body = await page.evaluate(() => document.body.innerText.slice(0, 400));
  // Correct deny: the operator page renders an explicit 403 naming the required
  // role (`corelink-admin`), NOT the tenants operator UI. This proves RbacGuard
  // fails closed for a `corelink-member`.
  expect(
    /forbidden|lacks operator|corelink-admin is required|operator role/i.test(body),
    "customer must get the explicit operator-role 403, not the console",
  ).toBeTruthy();
});

const ADMIN_API = ["/v1/admin/read/tenants", "/v1/admin/audit/events"];

/**
 * The operator API is internal-key-gated, so NO user-held credential works. This
 * exercises the three real client-controlled shapes against it:
 *   - a REAL, freshly-minted customer `cas:rw` PAT (a valid credential — the
 *     strongest a customer can self-serve; admin/owner scopes are never grantable),
 *   - that same PAT PLUS forged `x-corelink-scope` / `x-admin-scope` headers (a
 *     scope-escalation attempt — the Worker strips client-supplied scope headers),
 *   - anon.
 * All must be denied (401). The PAT is minted live via the session and the mint
 * is asserted 201 FIRST — without that guard an un-minted token silently
 * collapses every arm to anon, so the test would "pass" while proving nothing
 * (the prior shape read two never-set env vars → all three arms were anon).
 */
test("RBAC: the operator API rejects a real customer PAT, a forged-admin-scope PAT, and anon", async ({
  authedPage: page,
}) => {
  const out = await page.evaluate(async (paths: string[]) => {
    const API = "https://corelink-api.humangr.com";
    const clerk = (window as unknown as { Clerk: { session: { getToken(): Promise<string> } } }).Clerk;
    const session = await clerk.session.getToken();
    const sessAuth = { Authorization: `Bearer ${session}`, "content-type": "application/json" };

    // Mint a REAL customer cas:rw PAT via the authed session.
    const mintR = await fetch(`${API}/v1/customer/keys`, {
      method: "POST",
      headers: sessAuth,
      body: JSON.stringify({ name: `rbac-probe-${Date.now()}`, scopes: ["cas:rw"] }),
    });
    const mintBody = (await mintR.json().catch(() => ({}))) as { token?: string };
    const pat = mintBody?.token ?? "";

    const results: Record<string, number> = {};
    for (const path of paths) {
      // 1. a real, valid customer credential
      results[`customer ${path}`] = (
        await fetch(`${API}${path}`, { headers: { Authorization: `Bearer ${pat}` } })
      ).status;
      // 2. real customer PAT + FORGED operator-scope headers (must be stripped/ignored)
      results[`forged-admin ${path}`] = (
        await fetch(`${API}${path}`, {
          headers: {
            Authorization: `Bearer ${pat}`,
            "x-corelink-scope": "corelink:admin:pilots",
            "x-admin-scope": "corelink:admin:pilots",
          },
        })
      ).status;
      // 3. anon
      results[`anon ${path}`] = (await fetch(`${API}${path}`)).status;
    }
    return { mintStatus: mintR.status, hasPat: pat.length > 0, results };
  }, ADMIN_API);
  // eslint-disable-next-line no-console
  console.log("[admin-api] " + JSON.stringify(out, null, 2));

  // Guard the fix: the credentialed arms are only meaningful if a REAL PAT exists.
  expect(out.mintStatus, "customer PAT mint must 201 (else the credentialed arms collapse to anon)").toBe(201);
  expect(out.hasPat, "a real customer cas:rw PAT must be minted").toBe(true);
  // Internal-auth-gated operator plane → every user-controlled shape is denied 401.
  for (const [label, status] of Object.entries(out.results)) {
    expect(status, `${label} must be denied (401)`).toBe(401);
  }
});
