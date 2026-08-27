// /[locale]/customer/devenv — Cloud DevEnv sandboxes (WP-09)

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import { DevenvClient } from "@/components/customer/DevenvClient";

export default function CustomerDevenvPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-devenv-heading">
        <DevenvClient />
      </main>
    </CustomerGuard>
  );
}
