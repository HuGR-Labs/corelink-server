// WI-S16-005 — Clerk org-role binding.
//
// In production this is backed by `@clerk/nextjs/server` (`auth()` — see
// `clerkProvider` below). The E2E path (double-gated, never active in
// production builds) synthesizes the context from the playwright fixture
// cookie instead. Tests can still swap the whole provider via
// `setAuthProvider`.

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
    // NOT in E2E mode → resolve the real Clerk session server-side.
    return clerkProvider(empty);
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
    // access too in real Clerk org config); a plain `user` maps to
    // `corelink-member` — sufficient for the customer self-serve dashboard
    // (Viewer-minimum) but not for the operator surface (RbacGuard rejects).
    const role: ClerkOrgRole =
      decoded.role === "admin" || decoded.role === "approver"
        ? "corelink-admin"
        : decoded.role === "user"
          ? "corelink-member"
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

// ─── Real Clerk session resolution (production path) ─────────────────────────
//
// Mirrors the role mapping the E2E path synthesizes from the fixture cookie:
// an explicit Clerk org role keyed `corelink-admin` / `corelink-viewer` /
// `corelink-member` maps 1:1 (Clerk v6 prefixes custom org-role keys with
// `org:`); Clerk's legacy built-in roles map `admin` → `corelink-admin` and
// `basic_member`/`member` → `corelink-member`. A signed-in user with NO
// active organization is the self-serve solo-tenant case (signup-worker
// provisions one tenant per `clerk_user_id`) and gets `corelink-member` —
// Viewer-minimum for CustomerGuard, NEVER operator scope: `corelink-admin`
// only ever comes from an explicit org role, so RbacGuard stays fail-closed.

/** Map a raw Clerk `orgRole` claim onto {@link ClerkOrgRole}. */
function mapClerkOrgRole(orgRole: string | null | undefined): ClerkOrgRole {
  if (!orgRole) return null;
  const key = orgRole.startsWith("org:") ? orgRole.slice(4) : orgRole;
  if (
    key === "corelink-admin" ||
    key === "corelink-viewer" ||
    key === "corelink-member"
  ) {
    return key;
  }
  if (key === "admin") return "corelink-admin";
  if (key === "basic_member" || key === "member") return "corelink-member";
  // Unknown role key → null; the caller downgrades an *authenticated*
  // session to Viewer-minimum `corelink-member`. Operator scope
  // (`corelink-admin`) is never inferred — only explicit keys above.
  return null;
}

interface ClerkSessionShape {
  userId?: string | null;
  orgId?: string | null;
  orgRole?: string | null;
  sessionClaims?: {
    /** Session-token v2 factor-verification-age: [firstFactorAgeMin, secondFactorAgeMin]; -1 = never. */
    fva?: [number, number];
  } | null;
}

/**
 * Resolve the real Clerk session via `@clerk/nextjs/server` `auth()`.
 * Fail-closed: any error (Clerk not installed, no middleware context,
 * unit-test environment, malformed claims) returns the empty context.
 */
async function clerkProvider(empty: AuthContext): Promise<AuthContext> {
  try {
    // Lazy import so unit tests / non-Clerk environments never crash.
    const mod = (await import("@clerk/nextjs/server").catch(() => null)) as
      | { auth?: () => Promise<ClerkSessionShape> }
      | null;
    if (!mod?.auth) return empty;
    const session = await mod.auth();
    if (!session?.userId) return empty;
    const role: ClerkOrgRole =
      mapClerkOrgRole(session.orgRole) ?? "corelink-member";
    // mfa_verified_at from the v2 session-token `fva` claim (minutes since
    // the second factor was verified; -1 = never verified). Absent claim or
    // -1 → null, which mfaFresh() treats as stale (fail closed).
    let mfaVerifiedAt: string | null = null;
    const fva = session.sessionClaims?.fva;
    if (Array.isArray(fva) && typeof fva[1] === "number" && fva[1] >= 0) {
      mfaVerifiedAt = new Date(Date.now() - fva[1] * 60_000).toISOString();
    }
    return {
      user_id: session.userId,
      org_id: session.orgId ?? null,
      role,
      mfa_verified_at: mfaVerifiedAt,
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

/**
 * Customer self-serve dashboard requires Viewer-minimum: any authenticated
 * org member (admin/viewer/member) sees their tenant dashboard. Anonymous
 * sessions are rejected. See specs/_audits/sealed/2026-05-15-customer-dashboard-spec.md
 * §RBAC for the role mapping rationale.
 */
export function hasCustomerAccess(ctx: AuthContext): boolean {
  return (
    ctx.role === "corelink-admin" ||
    ctx.role === "corelink-viewer" ||
    ctx.role === "corelink-member"
  );
}

/** Returns true if MFA was verified within `maxAgeMinutes` (CTRL-AUTH-010). */
export function mfaFresh(ctx: AuthContext, maxAgeMinutes = 30): boolean {
  if (!ctx.mfa_verified_at) return false;
  const verifiedMs = Date.parse(ctx.mfa_verified_at);
  if (Number.isNaN(verifiedMs)) return false;
  const ageMs = Date.now() - verifiedMs;
  return ageMs <= maxAgeMinutes * 60 * 1000;
}
