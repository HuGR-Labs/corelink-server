/**
 * DEV-ONLY screenshot generator — captures every migrated screen (mock auth +
 * mocked API) to /tmp/corelink-screens for visual review. Not a real test.
 * Run: npx playwright test zz-screenshots --project=chromium
 */
import { test } from "../fixtures/test";
import { signInAs, FIXTURE_USERS } from "../fixtures/clerk";
import { installApiMocks } from "../fixtures/api-mocks";

const OUT = "/tmp/corelink-screens";

type Auth = "none" | "newDev" | "existingTenant" | "admin";
const SCREENS: { path: string; auth: Auth; name: string }[] = [
  // Customer dashboard
  { path: "/en/customer", auth: "existingTenant", name: "01-customer-home" },
  { path: "/en/customer/connect", auth: "existingTenant", name: "02-customer-connect" },
  { path: "/en/customer/keys", auth: "existingTenant", name: "03-customer-tokens" },
  { path: "/en/customer/usage", auth: "existingTenant", name: "04-customer-usage" },
  { path: "/en/customer/audit", auth: "existingTenant", name: "05-customer-audit" },
  { path: "/en/customer/runners", auth: "existingTenant", name: "06-customer-runners" },
  { path: "/en/customer/workspaces", auth: "existingTenant", name: "07-customer-workspaces" },
  { path: "/en/customer/trust", auth: "existingTenant", name: "08-customer-trust" },
  { path: "/en/customer/team", auth: "existingTenant", name: "09-customer-team" },
  { path: "/en/customer/billing", auth: "existingTenant", name: "10-customer-billing" },
  { path: "/en/customer/settings", auth: "existingTenant", name: "11-customer-settings" },
  // Admin / operator
  { path: "/en/admin/audit", auth: "admin", name: "20-admin-audit" },
  { path: "/en/admin/ops", auth: "admin", name: "21-admin-ops" },
  { path: "/en/admin/tenants", auth: "admin", name: "22-admin-tenants" },
  // Onboarding / activation
  { path: "/en/team/invite", auth: "existingTenant", name: "32-team-invite" },
  // DSR
  { path: "/en/dsr", auth: "existingTenant", name: "40-dsr-landing" },
  { path: "/en/dsr/status", auth: "existingTenant", name: "41-dsr-status" },
  // `/en/consent/new` (42) + `/en/consent` (43) removed: the consent surface is
  // RETIRED and answers 404 (`src/app/[locale]/consent/retired.ts`), so these
  // captured a 404 page, not a screen. Restore when `CONSENT_UI_RETIRED` flips.
  // Public / legal
  { path: "/en/pricing", auth: "none", name: "50-pricing" },
  { path: "/en/privacy", auth: "none", name: "51-privacy" },
  { path: "/en/privacy/sub-processors", auth: "none", name: "52-sub-processors" },
  { path: "/en/security/policy", auth: "none", name: "53-security-policy" },
  { path: "/en/legal/terms", auth: "none", name: "54-legal-terms" },
  { path: "/en/403", auth: "none", name: "55-403" },
];

test.describe.configure({ mode: "serial" });

for (const s of SCREENS) {
  test(`shot: ${s.name}`, async ({ page, context, baseURL }) => {
    await installApiMocks(page);
    if (s.auth !== "none") {
      await signInAs(context, FIXTURE_USERS[s.auth], baseURL!);
    }
    await page
      .goto(s.path, { waitUntil: "domcontentloaded", timeout: 20_000 })
      .catch(() => {});
    // Let client fetches settle without blocking on networkidle (some screens
    // have long-poll / retrying requests that never idle).
    await page.waitForTimeout(1800);
    await page
      .screenshot({ path: `${OUT}/${s.name}.png`, fullPage: true, timeout: 15_000 })
      .catch(() => {});
  });
}
