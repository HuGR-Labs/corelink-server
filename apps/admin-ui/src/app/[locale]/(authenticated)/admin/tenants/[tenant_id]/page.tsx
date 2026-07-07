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
  let tenant;
  try {
    tenant = await adminClient.getTenant(tenant_id);
  } catch {
    tenant = null;
  }

  return (
    <RbacGuard>
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
    </RbacGuard>
  );
}
