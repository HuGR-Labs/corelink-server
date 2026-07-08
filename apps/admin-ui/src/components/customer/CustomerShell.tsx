// Customer dashboard chrome (W0.e). Client component: the topbar (brand · Docs ·
// ⌘K · theme · account menu) + the global ⌘K command palette wired to the nav.
// The sidebar + page content are passed as children by the server layout.

"use client";

import React from "react";
import { useRouter } from "next/navigation";
import { useClerk } from "@clerk/nextjs";
import {
  CommandPalette,
  Menu,
  MenuItem,
  MenuSep,
  ThemeToggle,
} from "@/components/ui/linear";
import { customerNavLinks } from "@/components/customer/CustomerNav";

const DOCS_URL = "https://humangr.com/";

export interface CustomerShellProps {
  locale: string;
  children: React.ReactNode;
}

export function CustomerShell({ locale, children }: CustomerShellProps): React.ReactElement {
  const router = useRouter();
  const { signOut } = useClerk();

  const paletteItems = React.useMemo(
    () =>
      customerNavLinks(locale).map((l) => ({
        id: l.href,
        label: l.label,
        hint: "Go to",
        onSelect: () => router.push(l.href),
      })),
    [locale, router],
  );

  const openPalette = React.useCallback(() => {
    // The palette self-manages via a global ⌘K listener; surface a clickable
    // affordance by dispatching the same shortcut.
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "k", metaKey: true }));
  }, []);

  return (
    <div data-testid="customer-shell" className="cx-shell lin">
      <header data-testid="customer-header" className="cx-topbar">
        <div className="cx-brand">
          <b>CoreLink</b>
          <span className="sep">/</span>
          <span className="muted">Dashboard</span>
        </div>
        <div className="cx-topbar-right">
          <button type="button" className="cx-kbd" onClick={openPalette} aria-label="Open command palette">
            <span>Search</span>
            <kbd>⌘K</kbd>
          </button>
          <a href={DOCS_URL} target="_blank" rel="noreferrer">Docs</a>
          <ThemeToggle />
          <Menu
            trigger={
              <button type="button" className="cx-acct" aria-label="Account menu" data-testid="account-menu">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" aria-hidden="true">
                  <circle cx="12" cy="8" r="4" /><path d="M4 21v-1a6 6 0 0 1 6-6h4a6 6 0 0 1 6 6v1" />
                </svg>
              </button>
            }
          >
            <MenuItem onClick={() => router.push(`/${locale}/customer/billing`)}>Plan & billing</MenuItem>
            <MenuItem onClick={() => router.push(`/${locale}/customer/settings`)}>Settings</MenuItem>
            <MenuSep />
            <MenuItem onClick={() => void signOut(() => router.push(`/${locale}`))}>Sign out</MenuItem>
          </Menu>
        </div>
      </header>
      {children}
      <CommandPalette items={paletteItems} />
    </div>
  );
}

export default CustomerShell;
