// Product name on the Clerk auth screens.
//
// The Clerk *application* name (Dashboard → application settings) is unset, so
// every prebuilt widget falls back to Clerk's placeholder "My Application" —
// e.g. the sign-in card renders "Sign in to My Application". That name lives on
// the application object above the instance and is NOT settable through the
// Backend API (`PATCH /v1/instance` accepts the request but ignores it), so we
// pin the product name here instead: `localization` is a first-class Clerk prop
// that deep-merges over the default English resource, and keeping it in source
// means the name is version-controlled and ships through CI rather than living
// as an out-of-band Dashboard toggle that can silently drift.
//
// Only the app-name-bearing titles are overridden; everything else keeps Clerk's
// defaults. If the Dashboard application name is ever set, these still win for
// the widgets (harmless) — the Dashboard field additionally governs surfaces we
// can't reach here (e.g. transactional email sender name).
//
// Intentionally left un-annotated (no `LocalizationResource` import): that type
// lives in `@clerk/types`, a TRANSITIVE dep that pnpm's strict node_modules
// won't let us import directly. The shape is instead validated at the call site
// — `<ClerkProvider localization={clerkLocalization}>` type-checks this object
// against the real prop type from `@clerk/nextjs` (a direct dependency).
export const clerkLocalization = {
    signIn: { start: { title: "Sign in to CoreLink" } },
    signUp: { start: { title: "Create your CoreLink account" } },
};
