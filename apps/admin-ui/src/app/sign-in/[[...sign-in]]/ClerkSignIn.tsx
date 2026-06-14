/**
 * Client-only Clerk sign-in widget WITH its required `<ClerkProvider>`.
 *
 * Why this file exists (the fix): `<SignIn>` (like every Clerk widget) calls
 * `useSession`/`useClerk` internally, which throw
 *   "useSession can only be used within the <ClerkProvider /> component"
 * unless a `<ClerkProvider>` is mounted above them. The /sign-in route lives
 * OUTSIDE the `[locale]/(authenticated)` group (which has its own provider),
 * so without this wrapper the widget had no provider and the page crashed
 * with a client-side exception.
 *
 * Why it's a SEPARATE module dynamic-imported with `ssr: false` from page.tsx:
 * `@clerk/nextjs` runs initialisation in module-scope code that the Cloudflare
 * Workers edge sandbox rejects ("Disallowed operation called within global
 * scope"). Keeping the entire `@clerk/nextjs` import (provider + widget) behind
 * an `ssr: false` boundary means it only ever evaluates in the browser.
 */
"use client";

import { ClerkProvider, SignIn } from "@clerk/nextjs";

export default function ClerkSignIn(): React.ReactElement {
  return (
    <ClerkProvider
      publishableKey={process.env["NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"]}
    >
      <SignIn />
    </ClerkProvider>
  );
}
