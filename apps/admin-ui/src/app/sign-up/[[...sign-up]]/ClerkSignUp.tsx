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

export default function ClerkSignUp(): React.ReactElement {
  return (
    <ClerkProvider
      publishableKey={process.env["NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"]}
      localization={clerkLocalization}
    >
      {/*
       * forceRedirectUrl: always land on /en/welcome after Clerk completes
       * sign-up, regardless of the Clerk dashboard "redirect URL" setting.
       * fallbackRedirectUrl: safety net if Clerk ignores forceRedirectUrl
       * (e.g. email-verification flows that redirect independently).
       * /en/welcome is used because next-intl requires the locale prefix;
       * the default locale is "en" (src/i18n/request.ts).
       */}
      <SignUp forceRedirectUrl="/en/welcome" fallbackRedirectUrl="/en/welcome" />
    </ClerkProvider>
  );
}
