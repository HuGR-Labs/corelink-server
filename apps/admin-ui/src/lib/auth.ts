// WI-S16-005 — minimal Clerk org-role binding.
//
// In production this is backed by `@clerk/nextjs` (`auth()` server helper + the
// `useUser()` client hook). To keep WI-005 testable in isolation and decoupled
// from the parallel scaffolds (WI-S16-001..004) we expose a single
// `getCurrentRole()` provider that production wiring overrides.

import type { ClerkOrgRole } from "./types";

export interface AuthContext {
  user_id: string | null;
  org_id: string | null;
  role: ClerkOrgRole;
  mfa_verified_at: string | null; // ISO timestamp for fresh-MFA gate
}

export type AuthProvider = () => AuthContext | Promise<AuthContext>;

// E2E test-mode auth provider (wt-r3-7).
//
// When NEXT_PUBLIC_E2E_TEST_MODE === "1" AND NODE_ENV !== "production", the
// default provider reads the `__corelink_e2e_session` cookie and synthesizes
// an AuthContext from it. This wires the contract that
// playwright/fixtures/clerk.ts promises but never delivered.
//
// Production builds (NODE_ENV === "production") ALWAYS ignore the cookie and
// the env flag — the check is double-gated. See security notes in
// playwright/fixtures/clerk.ts.
async function defaultProvider(): Promise<AuthContext> {
  const empty: AuthContext = {
    user_id: null,
    org_id: null,
    role: null,
    mfa_verified_at: null,
  };
  const e2e = process.env["NEXT_PUBLIC_E2E_TEST_MODE"];
  if (e2e !== "1" || process.env["NODE_ENV"] === "production") {
    return empty;
  }
  try {
    // Lazy-load `next/headers` so unit tests (which exercise this module
    // outside the Next.js request context) keep working.
    const mod = (await import("next/headers").catch(() => null)) as
      | { cookies?: () => Promise<{ get: (n: string) => { value: string } | undefined }> }
      | null;
    if (!mod?.cookies) return empty;
    const store = await mod.cookies();
    const raw = store.get("__corelink_e2e_session")?.value;
    if (!raw) return empty;
    const decoded = JSON.parse(decodeURIComponent(raw)) as {
      sub?: string;
      tenant_id?: string;
      role?: "user" | "admin" | "approver";
      mfaAt?: number;
    };
    // Map fixture role → Clerk org role. Admin + approver both map to
    // `corelink-admin` for RbacGuard purposes (approver gets viewer-style
    // access too in real Clerk org config); a plain `user` maps to null.
    const role: ClerkOrgRole =
      decoded.role === "admin" || decoded.role === "approver"
        ? "corelink-admin"
        : null;
    return {
      user_id: decoded.sub ?? null,
      org_id: decoded.tenant_id ?? null,
      role,
      mfa_verified_at: decoded.mfaAt
        ? new Date(decoded.mfaAt * 1000).toISOString()
        : new Date().toISOString(),
    };
  } catch {
    return empty;
  }
}

let provider: AuthProvider = defaultProvider;

export function setAuthProvider(p: AuthProvider): void {
  provider = p;
}

export async function getAuthContext(): Promise<AuthContext> {
  return provider();
}

export function hasAdminRole(ctx: AuthContext): boolean {
  return ctx.role === "corelink-admin";
}

/** Returns true if MFA was verified within `maxAgeMinutes` (CTRL-AUTH-010). */
export function mfaFresh(ctx: AuthContext, maxAgeMinutes = 30): boolean {
  if (!ctx.mfa_verified_at) return false;
  const verifiedMs = Date.parse(ctx.mfa_verified_at);
  if (Number.isNaN(verifiedMs)) return false;
  const ageMs = Date.now() - verifiedMs;
  return ageMs <= maxAgeMinutes * 60 * 1000;
}
