// Shared hook that yields a lifetime-STABLE `CustomerClient` for a customer
// screen, immune to Clerk `getToken` identity churn.
//
// WHY THIS EXISTS (render-loop fix):
//   Every customer client component followed the pattern
//
//     const { getToken } = useAuth();
//     const client = useMemo(() => new CustomerClient({ getToken }), [getToken]);
//     const load  = useCallback(..., [client]);
//     useEffect(() => load(), [load]);
//
//   Clerk's `useAuth().getToken` is NOT referentially stable: for an
//   unprovisioned / thrashing session it churns a new function identity on
//   almost every render. That churn re-created the memoized `CustomerClient`
//   → re-created the `load` callback → re-fired the fetch effect → re-rendered
//   → churned `getToken` again → an infinite fetch/render loop (the "cursor
//   blinking madly" repro).
//
//   The fix keeps the LATEST `getToken` in a ref and hands the client a stable
//   thunk that reads `getTokenRef.current` at call time. The client is memoized
//   with an EMPTY dependency list, so its identity is fixed for the component's
//   lifetime. The fetch effect (keyed on the client) therefore runs exactly
//   once per mount and cannot loop on session thrash — while every request
//   still mints its token from the freshest `getToken` Clerk has handed us.
//
// Reading `getTokenRef.current` at call time (not memo time) is the officially
// endorsed "latest ref" pattern; mutating the ref during render is safe because
// we never read it during render.

import React from "react";
import { useAuth } from "@clerk/nextjs";
import { CustomerClient } from "@/lib/customer-client";

/**
 * Returns a `CustomerClient` whose identity is stable for the lifetime of the
 * calling component, regardless of how often Clerk's `getToken` identity
 * changes. Use this everywhere instead of the
 * `useMemo(() => new CustomerClient({ getToken }), [getToken])` pattern.
 */
export function useCustomerClient(): CustomerClient {
  const { getToken } = useAuth();

  // Keep the freshest getToken available to the (stable) client without
  // re-creating the client. Ref mutation during render is safe: the ref is
  // only ever read later, inside an async request.
  const getTokenRef = React.useRef(getToken);
  getTokenRef.current = getToken;

  // Empty deps → one client per mount. The client reads the ref at request
  // time, so it always uses the latest token supplier without churning.
  return React.useMemo(
    () => new CustomerClient({ getToken: () => getTokenRef.current() }),
    [],
  );
}
