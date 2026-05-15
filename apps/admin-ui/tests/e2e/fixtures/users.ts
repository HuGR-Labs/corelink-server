/**
 * Fixture users for the 5 critical-flow e2e suite (wt-r3-7).
 *
 * Mirrors playwright/fixtures/clerk.ts but kept locally under tests/e2e/ per
 * the task brief. The encoded JSON blob is read by src/lib/auth.ts in
 * NEXT_PUBLIC_E2E_TEST_MODE=1 to synthesize an AuthContext.
 */
import type { BrowserContext } from "@playwright/test";

export interface E2EUser {
  sub: string;
  email: string;
  role: "user" | "admin" | "approver";
  tenant_id: string;
  mfaAt?: number;
}

const now = () => Math.floor(Date.now() / 1000);

export const USERS = {
  admin: {
    sub: "user_e2e_admin",
    email: "admin@acme.example",
    role: "admin",
    tenant_id: "tenant_acme",
    mfaAt: now(),
  } satisfies E2EUser,
  approver: {
    sub: "user_e2e_approver",
    email: "approver@acme.example",
    role: "approver",
    tenant_id: "tenant_acme",
    mfaAt: now(),
  } satisfies E2EUser,
  member: {
    sub: "user_e2e_member",
    email: "member@acme.example",
    role: "user",
    tenant_id: "tenant_acme",
    mfaAt: now(),
  } satisfies E2EUser,
} as const;

const COOKIE_NAME = "__corelink_e2e_session";

export async function signInAs(
  context: BrowserContext,
  user: E2EUser,
  baseURL: string,
): Promise<void> {
  const url = new URL(baseURL);
  await context.addCookies([
    {
      name: COOKIE_NAME,
      value: encodeURIComponent(JSON.stringify(user)),
      domain: url.hostname,
      path: "/",
      httpOnly: false,
      secure: url.protocol === "https:",
      sameSite: "Lax",
    },
  ]);
}

export async function signOut(context: BrowserContext): Promise<void> {
  await context.clearCookies({ name: COOKIE_NAME });
}
