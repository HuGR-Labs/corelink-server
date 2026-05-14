// WI-S16-005 — /[locale]/admin/tenants/[tenant_id] — tenant deep-dive.

import React from "react";
import RbacGuard from "@/components/admin/RbacGuard";
import TenantDeepDive from "@/components/admin/TenantDeepDive";
import { adminClient } from "@/lib/admin-client";

export interface TenantPageProps {
  params: { locale: string; tenant_id: string };
}

export default async function AdminTenantPage({
  params,
}: TenantPageProps): Promise<React.ReactElement> {
  let tenant;
  try {
    tenant = await adminClient.getTenant(params.tenant_id);
  } catch {
    tenant = null;
  }

  return (
    // @ts-expect-error — server component returning Promise<ReactElement>.
    <RbacGuard>
      <main>
        <h1>Tenant {params.tenant_id}</h1>
        {!tenant && <p role="alert">Tenant not found.</p>}
        {tenant && <TenantDeepDive tenant={tenant} />}
      </main>
    </RbacGuard>
  );
}
