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
 * Pages inside this group:
 *   - /[locale]/welcome  — post-signup PAT reveal (WI-PLG-004)
 *   - /[locale]/customer — dashboard (to be added)
 */

import * as React from "react";
import { ClerkProvider } from "@clerk/nextjs";

export default function AuthenticatedLayout({
  children,
}: {
  children: React.ReactNode;
}): React.ReactElement {
  return <ClerkProvider>{children}</ClerkProvider>;
}
