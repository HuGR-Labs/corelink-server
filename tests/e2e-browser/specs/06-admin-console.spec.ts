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
 *      a customer PAT. Verified: customer PAT, admin-scoped PAT, and anon ALL get
 *      401 — the operator plane is correctly locked. Driving the read/mutate/
 *      dual-approve flow itself needs the operator keys (owner-held, OOB).
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

// The operator API is locked to the internal key: no user-held credential works.
const ADMIN_API = [
  "/v1/admin/read/tenants",
  "/v1/admin/audit/events",
];

test("RBAC: the operator API is locked — customer PAT / admin PAT / anon all 401", async () => {
  const base = "https://corelink-api.humangr.com";
  const rw = process.env["CORELINK_E2E_PAT_RW"] ?? "";
  const admin = process.env["CORELINK_E2E_PAT_ADMIN"] ?? "";
  for (const path of ADMIN_API) {
    for (const [who, tok] of [["customer", rw], ["admin-pat", admin], ["anon", ""]] as const) {
      const r = await fetch(`${base}${path}`, tok ? { headers: { Authorization: `Bearer ${tok}` } } : {});
      // eslint-disable-next-line no-console
      console.log(`[admin-api] ${who} ${path} → ${r.status}`);
      expect(r.status, `${who} must be denied ${path}`).toBe(401);
    }
  }
});
