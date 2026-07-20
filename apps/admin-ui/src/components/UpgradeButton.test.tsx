/**
 * Regression: the checkout POST must carry the surface's `/corelink` basePath.
 *
 * Next auto-prefixes basePath onto framework links but NEVER onto a raw
 * `fetch()`; a bare `/api/checkout/session` posts to the apex on the path
 * surface (humangr.com/corelink migration, #804) → 405 → checkout dead. Caught
 * live by the Playwright money journey (tests/e2e-browser/03-money-checkout).
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

  it("posts to a bare /api/checkout/session on the legacy no-prefix surface", async () => {
    window.history.pushState({}, "", "/en/upgrade");
    const fetchImpl = vi.fn().mockResolvedValue(okResponse());
    render(<UpgradeButton autoStart tier="pro" fetchImpl={fetchImpl} redirectImpl={() => {}} />);
    await waitFor(() => expect(fetchImpl).toHaveBeenCalled());
    expect(fetchImpl.mock.calls[0]?.[0]).toBe("/api/checkout/session");
  });
});
