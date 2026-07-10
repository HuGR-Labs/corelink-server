/**
 * useCustomerClient — render-stability regression guard.
 *
 * The customer screens loop ("cursor blinking madly") when Clerk's
 * `useAuth().getToken` reference churns: the old
 * `useMemo(() => new CustomerClient({ getToken }), [getToken])` re-created the
 * client on every churn → re-fired the fetch effect → re-rendered → churned
 * again. `useCustomerClient()` keeps `getToken` in a ref and memoizes the
 * client with an EMPTY dep list, so the client identity is stable for the
 * component lifetime regardless of getToken churn — the fetch effect (keyed on
 * the client) runs once and cannot loop.
 *
 * This test simulates the churn: the mocked `useAuth` hands back a BRAND-NEW
 * getToken function on every call. A stable client identity across re-renders
 * proves the loop can't happen.
 */

import { describe, it, expect, vi } from "vitest";
import { renderHook } from "@testing-library/react";

// Every call to useAuth() returns a fresh getToken identity — the exact churn
// that used to re-create the client and drive the render loop.
vi.mock("@clerk/nextjs", () => ({
  useAuth: () => ({ getToken: () => Promise.resolve("tok") }),
}));

import { useCustomerClient } from "@/lib/use-customer-client";
import { CustomerClient } from "@/lib/customer-client";

describe("useCustomerClient", () => {
  it("returns a client whose identity is stable across getToken churn", () => {
    const { result, rerender } = renderHook(() => useCustomerClient());
    const first = result.current;
    expect(first).toBeInstanceOf(CustomerClient);

    // Re-render several times — each re-render churns getToken identity.
    rerender();
    rerender();
    rerender();

    // The client MUST be the same instance — otherwise a client-keyed fetch
    // effect would re-fire on every render and loop.
    expect(result.current).toBe(first);
  });
});
