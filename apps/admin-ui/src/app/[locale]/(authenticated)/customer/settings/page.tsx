// /[locale]/customer/settings — spend controls, region, danger zone (W8).

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import SettingsClient from "@/components/customer/SettingsClient";

export default function CustomerSettingsPage(): React.ReactElement {
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-settings-heading">
        <h1 id="customer-settings-heading">Settings</h1>
        <p>Spend limit, region, notifications, and account controls.</p>
        <SettingsClient />
      </main>
    </CustomerGuard>
  );
}
