// Shared hook that yields a lifetime-STABLE `ConsentApi` carrying the caller's
// Clerk session bearer when one is reachable. Direct analogue of
// `useCustomerClient` — see that file for the full latest-ref rationale.
//
// The short version: Clerk's `useAuth().getToken` is NOT referentially stable,
// so memoizing on it re-creates the client on almost every render, which
// re-creates the callers' `load` callbacks and re-fires their fetch effects in
// a loop. The latest-ref pattern keeps the freshest `getToken` reachable while
// the client identity stays fixed for the component's lifetime.
//
// ⚠️ WHY THE PROVIDER LOOKUP IS GUARDED
//   `useCustomerClient` can call `useAuth()` unconditionally because every one
//   of its callers lives under `src/app/[locale]/(authenticated)/layout.tsx`,
//   which is the ONLY place a `<ClerkProvider>` is mounted — the root
//   `src/app/layout.tsx` deliberately does not mount one (see its header).
//   The consent routes are NOT in that group: they sit at
//   `src/app/[locale]/consent/*`, and `/consent/new` is deliberately PUBLIC and
//   pre-auth (`route-matcher.ts`). Calling `useAuth()` bare there throws
//   "useAuth can only be used within the <ClerkProvider /> component" and takes
//   down all four consent screens — a crash strictly worse than the wrong-origin
//   bug this change set out to fix.
//
//   So the lookup degrades to a null token supplier instead. Today that means
//   the consent surface sends no Authorization header — which is honest: it has
//   no session context to draw one from. The day a `<ClerkProvider>` is mounted
//   above these routes (or they move under the authenticated group) the bearer
//   starts flowing with no change here. That structural gap is a REAL blocker
//   for this feature and is called out in the PR, not papered over.
//
//   The guard is safe w.r.t. the rules of hooks: whether a provider is present
//   is fixed for the lifetime of a mounted tree, so the hook sequence is
//   identical on every render of a given component instance.

import React from "react";
import { useAuth } from "@clerk/nextjs";
import { createConsentApi, type ConsentApi } from "@/lib/consent-api";

type TokenSupplier = () => Promise<string | null>;

const NO_TOKEN: TokenSupplier = async () => null;

/** `useAuth().getToken`, or a null supplier when no `<ClerkProvider>` is above us. */
function useOptionalClerkGetToken(): TokenSupplier {
  try {
    return useAuth().getToken as TokenSupplier;
  } catch {
    return NO_TOKEN;
  }
}

/**
 * Returns a `ConsentApi` whose identity is stable for the lifetime of the
 * calling component, regardless of how often Clerk's `getToken` churns.
 */
export function useConsentApi(): ConsentApi {
  const getToken = useOptionalClerkGetToken();

  // Ref mutation during render is safe: only ever read later, inside a request.
  const getTokenRef = React.useRef(getToken);
  getTokenRef.current = getToken;

  // Empty deps → one client per mount; it reads the ref at request time.
  return React.useMemo(
    () => createConsentApi({ getToken: () => getTokenRef.current() }),
    [],
  );
}
