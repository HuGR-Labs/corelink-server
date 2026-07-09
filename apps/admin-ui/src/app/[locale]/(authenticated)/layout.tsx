"use client";

/**
 * (authenticated) route-group layout.
 *
 * Wraps authenticated pages (welcome, dashboard, …) with Clerk's client-side
 * provider. This is deliberately a CLIENT component so it renders at the
 * browser layer, not inside the CF Workers edge sandbox where Clerk's
 * module-scope initialisation would crash (see root layout.tsx comment).
 *
 * The root `app/layout.tsx` provides <html>, <body>, and NextIntlClientProvider.
 * This layout adds only the ClerkProvider layer so SSR server-actions and
 * `auth()` calls inside child Server Components receive the Clerk request
 * context without a full-edge ClerkProvider in the global scope.
 *
 * Pages inside this group (route groups don't change URLs):
 *   - /[locale]/welcome  — post-signup PAT reveal (WI-PLG-004)
 *   - /[locale]/customer — tenant self-serve dashboard (client components
 *     use useAuth/useUser → ClerkProvider must be mounted above them)
 *   - /[locale]/admin    — operator surface (same requirement)
 */

import * as React from "react";
import { ClerkProvider } from "@clerk/nextjs";

// E2E test mode has no reachable Clerk backend (auth is mocked at the data
// layer via the `__corelink_e2e_session` cookie — see src/lib/auth.ts). With
// NO publishable key, `<ClerkProvider>` falls into dev *keyless* mode, which
// polls Clerk's API to provision a throwaway dev instance and, while retrying
// against the unreachable FAPI, REMOUNTS this whole authenticated subtree in a
// loop — destroying in-flight React state (the minted-PAT reveal modal, a
// mid-approval op view, …) and making the E2E specs flaky. Passing a
// syntactically-valid dummy key pins the provider into a stable "has-key" mode
// (no keyless polling, no remount loop). Gated on NEXT_PUBLIC_E2E_TEST_MODE
// (never set in production) and scoped to THIS layout only, so the /sign-in
// route still sees no key and renders its keyless fallback. The key merely
// encodes the dummy FAPI host `example.clerk.accounts.dev`; it never talks to a
// real Clerk backend.
const E2E_DUMMY_CLERK_PK = "pk_test_ZXhhbXBsZS5jbGVyay5hY2NvdW50cy5kZXYk";

export default function AuthenticatedLayout({
  children,
}: {
  children: React.ReactNode;
}): React.ReactElement {
  const isE2E = process.env["NEXT_PUBLIC_E2E_TEST_MODE"] === "1";
  const clerkProps = isE2E ? { publishableKey: E2E_DUMMY_CLERK_PK } : {};
  return <ClerkProvider {...clerkProps}>{children}</ClerkProvider>;
}
