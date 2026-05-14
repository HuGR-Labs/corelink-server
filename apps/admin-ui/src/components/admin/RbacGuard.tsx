// WI-S16-005 — RbacGuard wraps every /admin/** route.
// Renders children only if the current Clerk org role matches `requiredRole`
// (default `corelink-admin`); otherwise renders an in-page 403 panel that links
// to the canonical 403 route.

import React from "react";
import type { AuthContext, AuthProvider } from "@/lib/auth";
import { getAuthContext, hasAdminRole } from "@/lib/auth";
import type { ClerkOrgRole } from "@/lib/types";

export interface RbacGuardProps {
  children: React.ReactNode;
  requiredRole?: ClerkOrgRole;
  authProvider?: AuthProvider;
  /**
   * If set, render this fallback instead of the default 403 panel. Useful for
   * server components that want a redirect instead of an inline panel.
   */
  fallback?: (reason: string) => React.ReactElement | null;
}

export function Forbidden({ reason }: { reason: string }): React.ReactElement {
  return (
    <div role="alert" data-testid="rbac-forbidden" className="rbac-forbidden">
      <h1>403 — forbidden</h1>
      <p>{reason}</p>
      <p>
        Operator role <code>corelink-admin</code> is required to access this
        page. Contact your security lead if you believe this is wrong.
      </p>
    </div>
  );
}

export async function RbacGuard({
  children,
  requiredRole = "corelink-admin",
  authProvider,
  fallback,
}: RbacGuardProps): Promise<React.ReactElement | null> {
  const ctx: AuthContext = authProvider ? await authProvider() : await getAuthContext();
  if (requiredRole === "corelink-admin" && !hasAdminRole(ctx)) {
    const reason = ctx.role
      ? `role "${ctx.role}" lacks operator scope`
      : "no authenticated operator session";
    if (fallback) return fallback(reason);
    return <Forbidden reason={reason} />;
  }
  if (requiredRole !== "corelink-admin" && ctx.role !== requiredRole) {
    const reason = `required role "${requiredRole}" not present`;
    if (fallback) return fallback(reason);
    return <Forbidden reason={reason} />;
  }
  return <>{children}</>;
}

export default RbacGuard;
