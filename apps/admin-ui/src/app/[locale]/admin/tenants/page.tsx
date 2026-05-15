// WI-S16-005 — /[locale]/admin/tenants — tenant search.

import React from "react";
import RbacGuard from "@/components/admin/RbacGuard";
import TenantSearchClient from "@/components/admin/TenantSearchClient";

export default function AdminTenantsPage(): React.ReactElement {
  return (
    <RbacGuard>
      <main aria-labelledby="tenants-heading">
        <h1 id="tenants-heading">Tenants</h1>
        <p>Search, filter, and inspect any tenant in operator scope.</p>
        <TenantSearchClient />
      </main>
    </RbacGuard>
  );
}
