// Customer self-serve dashboard guard — Viewer-minimum.
//
// Different from operator `RbacGuard` (which requires `corelink-admin`):
// the customer side is the tenant member's own dashboard, so any authenticated
// org role — admin / viewer / member — is allowed. Anonymous sessions hit the
// in-page 401 panel and are pointed at `/sign-in`.
//
// See `specs/_audits/2026-05-15-customer-dashboard-spec.md` §RBAC and
// `apps/docs/docs/explanation/rbac/role-catalog.mdx` for the canonical role
// matrix.

import React from "react";
import Link from "next/link";
import type { AuthContext, AuthProvider } from "@/lib/auth";
import { getAuthContext, hasCustomerAccess } from "@/lib/auth";

export interface CustomerGuardProps {
  children: React.ReactNode;
  authProvider?: AuthProvider;
}

export function Unauthenticated(): React.ReactElement {
  return (
    <div role="alert" data-testid="customer-unauth" className="customer-unauth">
      <h1>Sign in required</h1>
      <p>
        Your tenant dashboard is only available to signed-in members. Visit{" "}
        <Link href="/sign-in">/sign-in</Link> to authenticate.
      </p>
    </div>
  );
}

export async function CustomerGuard({
  children,
  authProvider,
}: CustomerGuardProps): Promise<React.ReactElement | null> {
  const ctx: AuthContext = authProvider ? await authProvider() : await getAuthContext();
  if (!hasCustomerAccess(ctx)) {
    return <Unauthenticated />;
  }
  return <>{children}</>;
}

export default CustomerGuard;
