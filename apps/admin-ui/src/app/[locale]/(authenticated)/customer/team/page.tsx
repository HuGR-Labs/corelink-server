// /[locale]/customer/team — tenant members + invite flow.

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import TeamClient from "@/components/customer/TeamClient";

export default function CustomerTeamPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-team-heading">
        <h1 id="customer-team-heading">Team</h1>
        <p>Members of this tenant and their role assignments. RBAC enforced server-side.</p>
        <TeamClient />
      </main>
    </CustomerGuard>
  );
}
