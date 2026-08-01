/**
 * Regression: the checkout POST must carry the `/corelink` basePath —
 * UNCONDITIONALLY, whatever the current pathname looks like.
 *
 * Next auto-prefixes basePath onto framework links but NEVER onto a raw
 * `fetch()`; a bare `/api/checkout/session` posts to the apex, which is a
 * DIFFERENT app (the hugr-site Pages project) — measured live 2026-08-01:
 *
 *     POST https://humangr.com/api/checkout/session          -> 405
 *     POST https://humangr.com/corelink/api/checkout/session -> 307 (sign-in)
 *
 * i.e. a bare POST is checkout dead. Caught live by the Playwright money
 * journey (tests/e2e-browser/03-money-checkout).
 *
 * This file previously asserted the OPPOSITE for a prefix-less pathname — a
 * bare POST was correct on the `corelink-app.humangr.com` subdomain, which the
 * app was served on during the subdomain→path migration. That surface is
 * RETIRED: both `corelink-app` and `corelink-admin` had their `custom_domain`
 * bindings removed and neither resolves (`dig` returns nothing — NXDOMAIN,
 * re-verified 2026-08-01). There is no longer any pathname shape for which a
 * bare POST is right, so the conditional is gone from `requestBasePath` and
 * the case below asserts the invariant that replaced it.
 */
import { render, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, afterEach } from "vitest";
import { UpgradeButton } from "./UpgradeButton";

function okResponse(): Response {
  return new Response(JSON.stringify({ checkout_url: "https://checkout.stripe.com/x", session_id: "cs_1" }), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

afterEach(() => {
  window.history.pushState({}, "", "/");
});

describe("UpgradeButton checkout POST — basePath re-attachment", () => {
  it("posts to /corelink/api/checkout/session on the path surface", async () => {
    window.history.pushState({}, "", "/corelink/en/upgrade");
    const fetchImpl = vi.fn().mockResolvedValue(okResponse());
    render(<UpgradeButton autoStart tier="pro" fetchImpl={fetchImpl} redirectImpl={() => {}} />);
    await waitFor(() => expect(fetchImpl).toHaveBeenCalled());
    expect(fetchImpl.mock.calls[0]?.[0]).toBe("/corelink/api/checkout/session");
  });

  it("still posts to /corelink/… from a prefix-less pathname — the retired subdomain must not resurrect a bare POST", async () => {
    // A pathname with no `/corelink` is what the RETIRED subdomain surface
    // used to produce. It no longer occurs in production, but if a rewrite,
    // a proxy, or a future basePath change ever reintroduces it, the POST
    // must NOT silently fall back to the apex (405, checkout dead).
    window.history.pushState({}, "", "/en/upgrade");
    const fetchImpl = vi.fn().mockResolvedValue(okResponse());
    render(<UpgradeButton autoStart tier="pro" fetchImpl={fetchImpl} redirectImpl={() => {}} />);
    await waitFor(() => expect(fetchImpl).toHaveBeenCalled());
    expect(fetchImpl.mock.calls[0]?.[0]).toBe("/corelink/api/checkout/session");
  });

  it("never emits a bare /api/checkout/session for ANY pathname shape", async () => {
    // The guard the old suite lacked: it asserted one prefixed case and one
    // bare case as both-correct, so the defect was green from both sides.
    for (const p of ["/", "/en/upgrade", "/corelink", "/corelink/en/upgrade", "/corelink/"]) {
      window.history.pushState({}, "", p);
      const fetchImpl = vi.fn().mockResolvedValue(okResponse());
      const { unmount } = render(
        <UpgradeButton autoStart tier="pro" fetchImpl={fetchImpl} redirectImpl={() => {}} />,
      );
      await waitFor(() => expect(fetchImpl).toHaveBeenCalled());
      expect(fetchImpl.mock.calls[0]?.[0]).toBe("/corelink/api/checkout/session");
      unmount();
    }
  });
});
