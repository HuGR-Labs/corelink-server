// /[locale]/customer/team — tenant members + invite flow.

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import TeamClient from "@/components/customer/TeamClient";

export default function CustomerTeamPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-team-heading">
        <h1 id="customer-team-heading">Team</h1>
        <p>
          Invite teammates, see who has access, and understand what each role can do. Removing a
          member also revokes their tokens. Roles are enforced server-side (RBAC).
        </p>
        <TeamClient />
      </main>
    </CustomerGuard>
  );
}
