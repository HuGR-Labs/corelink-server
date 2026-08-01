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

import { clerkLocalization } from "@/lib/clerk-localization";
import { APP_BASE_PATH } from "@/lib/route-matcher";

export default function ClerkSignIn(): React.ReactElement {
  return (
    <ClerkProvider
      publishableKey={process.env["NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"]}
      localization={clerkLocalization}
      signInUrl={`${APP_BASE_PATH}/sign-in`}
      signUpUrl={`${APP_BASE_PATH}/sign-up`}
    >
      {/*
       * fallbackRedirectUrl (NOT force): after sign-in, land on the
       * authenticated dashboard — NOT the marketing home. The Clerk instance
       * "paths" are all null, so the default after-sign-in was "/" (the public
       * landing, which has no ClerkProvider and shows no signed-in state) →
       * users perceived "can't sign in / it bounces to home" (#270). An
       * already-signed-in user hitting /sign-in is also redirected straight
       * here instead of bouncing to "/".
       *
       * It MUST be `fallbackRedirectUrl`, never `forceRedirectUrl`: force
       * unconditionally overrides the `?redirect_url=` query param, which
       * broke the signed-out buyer funnel — `/upgrade?plan=<tier>` round-trips
       * through `/sign-in?redirect_url=/<locale>/upgrade?plan=<tier>`
       * ([locale]/upgrade/page.tsx) and the visitor must land BACK on the
       * upgrade page to auto-fire checkout, not on the dashboard with the
       * plan intent dropped. Fallback preserves both behaviours: honours
       * `redirect_url` when present, dashboard otherwise. Regression-locked
       * in tests/clerk-basepath.test.tsx.
       *
       * /en/customer is the (authenticated)-group dashboard (default locale en).
       */}
      <SignIn
        path={`${APP_BASE_PATH}/sign-in`}
        routing="path"
        fallbackRedirectUrl="/en/customer"
      />
    </ClerkProvider>
  );
}
