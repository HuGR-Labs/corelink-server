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

let provider: AuthProvider = () => ({
  user_id: null,
  org_id: null,
  role: null,
  mfa_verified_at: null,
});

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
