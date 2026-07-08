// WI-S16-005 — /[locale]/admin/tenants — tenant search.

import React from "react";
import RbacGuard from "@/components/admin/RbacGuard";
import TenantSearchClient from "@/components/admin/TenantSearchClient";

export default function AdminTenantsPage(): React.ReactElement {
  return (
    <RbacGuard>
      <div className="cx-shell lin">
        <div className="cx-main">
          <main aria-labelledby="tenants-heading">
            <h1 id="tenants-heading">Tenants</h1>
            <p>
              Look up any tenant in operator scope to inspect its plan, region, and BYOK
              status, then open the deep-dive. Filter the list by plan, region, or BYOK
              state. Sensitive actions (suspend, offboard, entitlement changes) are queued
              for dual approval in <code>/admin/ops</code>.
            </p>
            <div className="lin-mt-lg">
              <TenantSearchClient />
            </div>
          </main>
        </div>
      </div>
    </RbacGuard>
  );
}
