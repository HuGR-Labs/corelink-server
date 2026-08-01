/**
 * Client-only Clerk sign-up widget WITH its required `<ClerkProvider>`.
 *
 * See the sibling sign-in `ClerkSignIn.tsx` for the full rationale: `<SignUp>`
 * calls `useSession`/`useClerk` internally and throws without a `<ClerkProvider>`
 * ancestor; the /sign-up route is outside the `(authenticated)` provider group,
 * so it must supply its own. The whole `@clerk/nextjs` import stays behind the
 * page's `ssr: false` dynamic boundary to avoid the edge-runtime module-scope
 * sandbox error.
 */
"use client";

import { ClerkProvider, SignUp } from "@clerk/nextjs";

import { clerkLocalization } from "@/lib/clerk-localization";
import { APP_BASE_PATH, withAppBasePath } from "@/lib/route-matcher";

export default function ClerkSignUp(): React.ReactElement {
  return (
    <ClerkProvider
      publishableKey={process.env["NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"]}
      localization={clerkLocalization}
      signInUrl={`${APP_BASE_PATH}/sign-in`}
      signUpUrl={`${APP_BASE_PATH}/sign-up`}
    >
      {/*
       * forceRedirectUrl: always land on /en/welcome after Clerk completes
       * sign-up, regardless of the Clerk dashboard "redirect URL" setting.
       * fallbackRedirectUrl: safety net if Clerk ignores forceRedirectUrl
       * (e.g. email-verification flows that redirect independently).
       * /en/welcome is used because next-intl requires the locale prefix;
       * the default locale is "en" (src/i18n/request.ts).
       *
       * BOTH must carry the basePath (`withAppBasePath`). PROVEN against live
       * prod with a throwaway Clerk user: the after-auth navigation landed on
       * `humangr.com/en/customer` — basePath LOST — i.e. the hugr-site
       * marketing landing, not the app. The identical shape here meant every
       * NEW SIGN-UP was dropped on the marketing page instead of /en/welcome,
       * silently skipping the one-time PAT reveal + DPA onboarding. Clerk's
       * after-auth navigation does not go through Next's router (the only
       * thing that auto-applies basePath); prefixing is safe under both
       * mechanisms (Clerk's `removeBasePath` strips it before a router.push,
       * and a hard `window.location` gets the correct absolute path).
       */}
      <SignUp
        path={`${APP_BASE_PATH}/sign-up`}
        routing="path"
        forceRedirectUrl={withAppBasePath("/en/welcome")}
        fallbackRedirectUrl={withAppBasePath("/en/welcome")}
      />
    </ClerkProvider>
  );
}
