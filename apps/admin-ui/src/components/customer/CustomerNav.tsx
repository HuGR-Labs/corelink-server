// Customer-side navigation (W0.e). Grouped by job-to-be-done; operator routes
// (`/admin/*`) are deliberately absent — this nav only surfaces the caller's own
// tenant. Active link resolved via `data-active`. Existing testIds preserved
// (nav-overview/usage/audit/billing/keys/team) so current e2e keeps passing;
// new routes are stubbed in W0 so nothing 404s before the screen wave (W1–W10).

"use client";

import React from "react";
import { usePathname } from "next/navigation";

export interface CustomerNavLink {
  href: string;
  label: string;
  testId: string;
  icon: React.ReactNode;
}

export interface CustomerNavGroup {
  heading?: string;
  links: CustomerNavLink[];
}

// Minimal inline icons (Lucide-style, currentColor, 15px).
const I = (d: string): React.ReactNode => (
  <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d={d} />
  </svg>
);
const ICON = {
  home: I("M3 10.5 12 3l9 7.5M5 9.5V21h14V9.5"),
  connect: I("M8 7 3 12l5 5M16 7l5 5-5 5M14 4l-4 16"),
  key: I("M15 7a4 4 0 1 0-3.9 5H14v2h2v2h3v-3l-1.1-1A4 4 0 0 0 15 7z"),
  usage: I("M3 3v18h18M7 15l3-4 3 3 4-6"),
  audit: I("M9 12l2 2 4-4M12 3l7 3v6c0 4-3 7-7 9-4-2-7-5-7-9V6l7-3z"),
  runner: I("M13 2 3 14h7l-1 8 10-12h-7l1-8z"),
  workspace: I("M3 7h18M3 7v12a1 1 0 0 0 1 1h16a1 1 0 0 0 1-1V7M3 7l2-3h14l2 3"),
  trust: I("M12 3l7 3v6c0 4-3 7-7 9-4-2-7-5-7-9V6l7-3zM9.5 12l1.7 1.7 3.3-3.4"),
  team: I("M16 20v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2M9 10a3 3 0 1 0 0-6 3 3 0 0 0 0 6M22 20v-2a4 4 0 0 0-3-3.9M16 4.1a4 4 0 0 1 0 7.8"),
  billing: I("M3 7h18v10H3zM3 11h18M7 15h2"),
  settings: I("M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-2.7 1.1V21a2 2 0 1 1-4 0v-.1A1.6 1.6 0 0 0 7 19.4a1.6 1.6 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.6 1.6 0 0 0-1.1-2.7H1a2 2 0 1 1 0-4h.1A1.6 1.6 0 0 0 2.6 7"),
};

export function customerNavGroups(locale: string): CustomerNavGroup[] {
  const b = `/${locale}/customer`;
  return [
    { links: [{ href: b, label: "Home", testId: "nav-overview", icon: ICON.home }] },
    {
      heading: "Get started",
      links: [
        { href: `${b}/connect`, label: "Connect a tool", testId: "nav-connect", icon: ICON.connect },
        { href: `${b}/keys`, label: "Tokens", testId: "nav-keys", icon: ICON.key },
      ],
    },
    {
      heading: "Observe",
      links: [
        { href: `${b}/usage`, label: "Usage & savings", testId: "nav-usage", icon: ICON.usage },
        { href: `${b}/audit`, label: "Audit log", testId: "nav-audit", icon: ICON.audit },
      ],
    },
    {
      heading: "Build",
      links: [
        { href: `${b}/runners`, label: "Runners", testId: "nav-runners", icon: ICON.runner },
        { href: `${b}/workspaces`, label: "Workspaces", testId: "nav-workspaces", icon: ICON.workspace },
      ],
    },
    {
      heading: "Govern",
      links: [
        { href: `${b}/trust`, label: "Trust & compliance", testId: "nav-trust", icon: ICON.trust },
        { href: `${b}/team`, label: "Team", testId: "nav-team", icon: ICON.team },
      ],
    },
    {
      heading: "Account",
      links: [
        { href: `${b}/billing`, label: "Plan & billing", testId: "nav-billing", icon: ICON.billing },
        { href: `${b}/settings`, label: "Settings", testId: "nav-settings", icon: ICON.settings },
      ],
    },
  ];
}

/** Flat list retained for callers/tests that expect the old shape. */
export function customerNavLinks(locale: string): CustomerNavLink[] {
  return customerNavGroups(locale).flatMap((g) => g.links);
}

export interface CustomerNavProps {
  locale: string;
  activeHref?: string;
}

export function CustomerNav({ locale, activeHref }: CustomerNavProps): React.ReactElement {
  const pathname = usePathname();
  const active = activeHref ?? pathname ?? "";
  const groups = customerNavGroups(locale);
  return (
    <nav aria-label="Customer dashboard" data-testid="customer-nav" className="cx-nav">
      {groups.map((g, gi) => (
        <div className="cx-nav__group" key={g.heading ?? `g${gi}`}>
          {g.heading ? <div className="cx-nav__heading">{g.heading}</div> : null}
          <ul>
            {g.links.map((l) => (
              <li key={l.href}>
                <a
                  href={l.href}
                  data-testid={l.testId}
                  data-active={active === l.href ? "true" : "false"}
                >
                  <span className="cx-nav__ic" aria-hidden="true">{l.icon}</span>
                  {l.label}
                </a>
              </li>
            ))}
          </ul>
        </div>
      ))}
    </nav>
  );
}

export default CustomerNav;
