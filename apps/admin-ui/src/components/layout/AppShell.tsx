"use client";

import * as React from "react";
import { cn } from "@/lib/cn";

export interface NavItem {
  label: string;
  href: string;
  /** Optional list of required roles; if empty the item is shown to all. */
  requiredRoles?: string[];
}

export interface AppShellProps {
  logo?: React.ReactNode;
  userMenu?: React.ReactNode;
  navItems: NavItem[];
  /** Current user roles, used for RBAC filtering of nav items. */
  userRoles?: string[];
  footer?: React.ReactNode;
  skipToContentLabel?: string;
  children: React.ReactNode;
}

export function AppShell({
  logo,
  userMenu,
  navItems,
  userRoles = [],
  footer,
  skipToContentLabel = "Skip to main content",
  children,
}: AppShellProps) {
  const visibleNav = navItems.filter(
    (n) => !n.requiredRoles?.length || n.requiredRoles.some((r) => userRoles.includes(r))
  );

  return (
    <div className="flex min-h-screen flex-col bg-slate-50 text-slate-900">
      {/* Skip link — first focusable element. */}
      <a
        href="#main-content"
        className={cn(
          "sr-only focus:not-sr-only",
          "focus-visible:fixed focus-visible:left-2 focus-visible:top-2 focus-visible:z-50",
          "focus-visible:rounded focus-visible:bg-slate-900 focus-visible:px-3 focus-visible:py-2 focus-visible:text-white"
        )}
      >
        {skipToContentLabel}
      </a>

      <header className="flex items-center justify-between border-b border-slate-200 bg-white px-4 py-2">
        <div className="flex items-center gap-2">{logo}</div>
        <div>{userMenu}</div>
      </header>

      <div className="flex flex-1">
        <aside aria-label="Primary" className="w-56 border-r border-slate-200 bg-white p-3">
          <nav>
            <ul className="flex flex-col gap-1">
              {visibleNav.map((n) => (
                <li key={n.href}>
                  <a
                    href={n.href}
                    className={cn(
                      "block min-h-[40px] rounded px-3 py-2 text-sm font-medium text-slate-700",
                      "hover:bg-slate-100",
                      "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700"
                    )}
                  >
                    {n.label}
                  </a>
                </li>
              ))}
            </ul>
          </nav>
        </aside>

        <main id="main-content" tabIndex={-1} className="flex-1 p-6">
          {children}
        </main>
      </div>

      <footer className="border-t border-slate-200 bg-white px-4 py-3 text-sm text-slate-600">
        {footer}
      </footer>
    </div>
  );
}
