// Customer self-serve dashboard layout.
//
// Distinct from the operator surface: no admin chrome, no global tenant
// switcher, no operator nav. Anyone authenticated in the org (admin / viewer /
// member) sees only their own tenant's data through `/v1/customer/*`
// endpoints. See `specs/_audits/sealed/2026-05-15-customer-dashboard-spec.md`.

import * as React from "react";
import CustomerNav from "@/components/customer/CustomerNav";
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
    <div data-testid="customer-shell" className="cx-shell">
      <header data-testid="customer-header" className="cx-topbar">
        <div className="cx-brand">
          <b>CoreLink</b>
          <span className="sep">/</span>
          <span className="muted">Dashboard</span>
        </div>
        <div className="cx-topbar-right">
          <a href="https://humangr.com/">humangr.com</a>
        </div>
      </header>
      <div className="cx-body">
        <aside className="cx-sidebar">
          <CustomerNav locale={locale} />
        </aside>
        <div className="cx-main">{children}</div>
      </div>
    </div>
  );
}
