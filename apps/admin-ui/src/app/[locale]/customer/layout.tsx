// Customer self-serve dashboard layout.
//
// Distinct from the operator surface: no admin chrome, no global tenant
// switcher, no operator nav. Anyone authenticated in the org (admin / viewer /
// member) sees only their own tenant's data through `/v1/customer/*`
// endpoints. See `specs/_audits/2026-05-15-customer-dashboard-spec.md`.

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
    <div data-testid="customer-shell" className="customer-shell">
      <header data-testid="customer-header">
        <strong>CoreLink — Tenant dashboard</strong>
      </header>
      <div style={{ display: "flex", gap: "1.5rem" }}>
        <aside style={{ minWidth: 200 }}>
          <CustomerNav locale={locale} />
        </aside>
        <section style={{ flex: 1 }}>{children}</section>
      </div>
    </div>
  );
}
