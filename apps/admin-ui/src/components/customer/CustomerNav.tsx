// Customer-side navigation. Operator routes (`/admin/*`) are deliberately
// absent — this nav only surfaces the caller's own tenant. The active link is
// resolved by the consumer via `data-active` (rendered by the page).

"use client";

import React from "react";

export interface CustomerNavLink {
  href: string;
  label: string;
  testId: string;
}

export function customerNavLinks(locale: string): CustomerNavLink[] {
  return [
    { href: `/${locale}/customer`, label: "Overview", testId: "nav-overview" },
    { href: `/${locale}/customer/usage`, label: "Usage", testId: "nav-usage" },
    { href: `/${locale}/customer/audit`, label: "Audit", testId: "nav-audit" },
    { href: `/${locale}/customer/billing`, label: "Billing", testId: "nav-billing" },
    { href: `/${locale}/customer/keys`, label: "Keys", testId: "nav-keys" },
    { href: `/${locale}/customer/team`, label: "Team", testId: "nav-team" },
  ];
}

export interface CustomerNavProps {
  locale: string;
  activeHref?: string;
}

export function CustomerNav({ locale, activeHref }: CustomerNavProps): React.ReactElement {
  const links = customerNavLinks(locale);
  return (
    <nav aria-label="Customer dashboard" data-testid="customer-nav">
      <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
        {links.map((l) => (
          <li key={l.href}>
            <a
              href={l.href}
              data-testid={l.testId}
              data-active={activeHref === l.href ? "true" : "false"}
            >
              {l.label}
            </a>
          </li>
        ))}
      </ul>
    </nav>
  );
}

export default CustomerNav;
