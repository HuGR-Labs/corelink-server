/**
 * Clerk test-mode authentication helper — WI-S16-007.
 *
 * STRATEGY DECISION (recorded here per WI-S16-007 §6.1 “Document the
 * testing-strategy choice”):
 *
 * Option A — `@clerk/testing` package with `clerkSetup()` / `setupClerkTestingToken()`.
 *   Pros: official, supports real Clerk dev tenant + ephemeral test users.
 *   Cons: requires CLERK_TEST_API_KEY + network call against Clerk; brittle
 *   when running offline-CI; ties the e2e suite to a real third-party tenant.
 *
 * Option B — Stub session via cookies + a test-mode env flag the app honors.
 *   Pros: fully deterministic, offline-CI compatible, zero coupling to Clerk
 *   uptime; matches the FE-only mock posture of this suite.
 *   Cons: does not exercise the real Clerk redirect flow (acceptable because
 *   Clerk integration is unit-tested in WI-S16-001 + WI-S16-002 specs, and
 *   the production Clerk redirect path is exercised by Clerk's own test
 *   coverage of the SDK).
 *
 * We CHOSE Option B for the ship gate. Rationale:
 *   - Suite must run with no network egress (gate runs in PR CI).
 *   - Real Clerk integration is already covered by 244 vitest tests of the
 *     middleware + route guard layers (apps/admin-ui/src/lib + middleware.ts).
 *   - The deferred Clerk-production-keys waiver (WI-S16-007 §6 PRR) covers
 *     real-Clerk e2e for post-GA.
 *
 * Mechanics:
 *   - The app reads `process.env.NEXT_PUBLIC_E2E_TEST_MODE` (set in
 *     playwright.config.ts → webServer.env). When "1", middleware/route
 *     guards accept a `__corelink_e2e_session` cookie as a synthetic Clerk
 *     session, encoded as a JSON blob of `{ sub, email, role, tenant_id, mfaAt }`.
 *   - This file installs that cookie before navigation so protected routes
 *     don't bounce to /sign-in.
 *
 * Important: production builds MUST ignore this cookie. The check is gated
 * on `NEXT_PUBLIC_E2E_TEST_MODE === "1"` AND `process.env.NODE_ENV !== "production"`.
 */

import type { BrowserContext } from "@playwright/test";

export interface E2EUser {
  sub: string;
  email: string;
  role: "user" | "admin" | "approver";
  tenant_id: string;
  /** epoch seconds when MFA was last refreshed (CTRL-AUTH-010 ≤ 30 min). */
  mfaAt?: number;
}

export const FIXTURE_USERS = {
  newDev: {
    sub: "user_e2e_newdev",
    email: "newdev@example.com",
    role: "user",
    tenant_id: "",
    mfaAt: Math.floor(Date.now() / 1000),
  } satisfies E2EUser,
  existingTenant: {
    sub: "user_e2e_tenant",
    email: "user@acme.example",
    role: "user",
    tenant_id: "tenant_acme",
    mfaAt: Math.floor(Date.now() / 1000),
  } satisfies E2EUser,
  admin: {
    sub: "user_e2e_admin",
    email: "admin@acme.example",
    role: "admin",
    tenant_id: "tenant_acme",
    mfaAt: Math.floor(Date.now() / 1000),
  } satisfies E2EUser,
  approver: {
    sub: "user_e2e_approver",
    email: "approver@acme.example",
    role: "approver",
    tenant_id: "tenant_acme",
    mfaAt: Math.floor(Date.now() / 1000),
  } satisfies E2EUser,
} as const;

/**
 * Install a synthetic Clerk session into the BrowserContext.
 *
 * Call once per test (typically in `test.beforeEach`). The cookie is
 * domain-scoped to the baseURL host so it applies to all routes.
 */
export async function signInAs(
  context: BrowserContext,
  user: E2EUser,
  baseURL: string,
): Promise<void> {
  const url = new URL(baseURL);
  await context.addCookies([
    {
      name: "__corelink_e2e_session",
      value: encodeURIComponent(JSON.stringify(user)),
      domain: url.hostname,
      path: "/",
      httpOnly: false,
      secure: url.protocol === "https:",
      sameSite: "Lax",
    },
  ]);
}

/**
 * Clear the synthetic session (for sign-out / 403 RBAC tests).
 */
export async function signOut(context: BrowserContext): Promise<void> {
  await context.clearCookies({ name: "__corelink_e2e_session" });
}
