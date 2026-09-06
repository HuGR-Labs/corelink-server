// WI-S16-005 — /[locale]/admin/tenants/[tenant_id] — tenant deep-dive.

import React from "react";
import RbacGuard from "@/components/admin/RbacGuard";
import TenantDeepDive from "@/components/admin/TenantDeepDive";
import { adminClient } from "@/lib/admin-client";
import { Callout } from "@/components/ui/linear";

export interface TenantPageProps {
  params: Promise<{ locale: string; tenant_id: string }>;
}

export default async function AdminTenantPage({
  params,
}: TenantPageProps): Promise<React.ReactElement> {
  const { tenant_id } = await params;

  async function GuardedTenantContent(): Promise<React.ReactElement> {
    // Do not fetch privileged tenant data until RbacGuard has admitted the
    // request. This ordering is security-relevant even if the result is later
    // rendered only inside the guard: the old code queried first and guarded
    // second, leaking existence/timing to unauthorized callers.
    let tenant;
    try {
      tenant = await adminClient.getTenant(tenant_id);
    } catch {
      tenant = null;
    }

    return (
      <div className="cx-shell lin">
        <div className="cx-main">
          <main>
            <h1>Tenant {tenant_id}</h1>
            {!tenant && (
              <div role="alert" className="lin-mt">
                <Callout tone="danger">Tenant not found.</Callout>
              </div>
            )}
            {tenant && (
              <div className="lin-mt-lg">
                <TenantDeepDive tenant={tenant} />
              </div>
            )}
          </main>
        </div>
      </div>
    );
  }

  return (
    <RbacGuard>
      <GuardedTenantContent />
    </RbacGuard>
  );
}
