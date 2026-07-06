// /[locale]/customer — Home: onboarding checklist + live snapshot + ROI hero (W1).

import React from "react";
import CustomerGuard from "@/components/customer/CustomerGuard";
import HomeClient from "@/components/customer/HomeClient";
import type { Locale } from "@/i18n/LocaleContext";

interface PageProps {
  params: Promise<{ locale: Locale }>;
}

export default async function CustomerHomePage({
  params,
}: PageProps): Promise<React.ReactElement> {
  const { locale } = await params;
  return (
    <CustomerGuard>
      <main aria-labelledby="customer-overview-heading">
        <h1 id="customer-overview-heading">Overview</h1>
        <p>Snapshot of your tenant: usage, billing, BYOK status, and recent activity.</p>
        <HomeClient locale={locale} />
      </main>
    </CustomerGuard>
  );
}
