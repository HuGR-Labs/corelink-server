// Customer self-serve dashboard layout (W0.e).
//
// Distinct from the operator surface: no admin chrome, no global tenant
// switcher, no operator nav. Anyone authenticated in the org (admin / viewer /
// member) sees only their own tenant's data through `/v1/customer/*`
// endpoints. See `specs/_audits/sealed/2026-05-15-customer-dashboard-spec.md`.
//
// The interactive chrome (topbar, ⌘K palette, account menu, theme toggle) lives
// in the client `CustomerShell`; this server layout wires the sidebar + content.

import * as React from "react";
import CustomerNav from "@/components/customer/CustomerNav";
import CustomerShell from "@/components/customer/CustomerShell";
import type { Locale } from "@/i18n/LocaleContext";

interface LayoutProps {
  children: React.ReactNode;
  params: Promise<{ locale: Locale }>;
}

export default async function CustomerLayout({
  children,
  params,
}: LayoutProps): Promise<React.ReactElement> {
  const { locale } = await params;
  return (
    <CustomerShell locale={locale}>
      <div className="cx-body">
        <aside className="cx-sidebar">
          <CustomerNav locale={locale} />
        </aside>
        <div className="cx-main">{children}</div>
      </div>
    </CustomerShell>
  );
}
