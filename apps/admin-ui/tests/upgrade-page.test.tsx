/**
 * /upgrade — public pricing CTA → Stripe Checkout bridge tests (#49).
 *
 * Covers the three layers added for the docs CTAs
 * (`humangr.com/corelink/upgrade?plan=<tier>`):
 *   1. `normalizeCheckoutTier` — `?plan=` validation/defaulting
 *      (solo|starter|team|pro|max; invalid/missing → "pro").
 *   2. `GET /upgrade` (locale-less) — 307 forwarder to
 *      `/en/upgrade?plan=<normalized>`.
 *   3. `/[locale]/upgrade` page — signed-out → `/sign-in?redirect_url=…`
 *      round-trip; signed-in → tier card + auto-fired checkout POST
 *      (mock fetch + Clerk, mirroring UpgradeButton/checkout-session
 *      route tests).
 *   4. `<UpgradeButton autoStart />` — fires the POST once on mount
 *      (StrictMode-safe), never without the flag.
 */

import * as React from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";

// Hoisted module mocks — registered BEFORE the page module is imported so
// the page's lazy `import("@clerk/nextjs/server")` and `redirect` resolve
// to these stubs (same pattern as welcome.test.tsx +
// checkout-session-route.test.ts).
vi.mock("next/navigation", () => ({
  redirect: vi.fn(),
}));

vi.mock("@clerk/nextjs/server", () => ({
  auth: vi.fn(),
}));

import {
  CHECKOUT_TIER_IDS,
  DEFAULT_CHECKOUT_TIER,
  normalizeCheckoutTier,
} from "@/lib/pricing";
import { UpgradeButton } from "@/components/UpgradeButton";

// ---------------------------------------------------------------------------
// 1. ?plan= validation / defaulting
// ---------------------------------------------------------------------------

describe("normalizeCheckoutTier (?plan= validation)", () => {
  it("accepts every checkout-able tier id verbatim", () => {
    // The frozen checkout contract: the 5 cache SKUs (incl. legacy `team`)
    // + the 5 self-serve runner SKUs (byte-identical to the backend).
    expect(CHECKOUT_TIER_IDS).toEqual([
      "solo",
      "starter",
      "team",
      "pro",
      "max",
      "runner_starter",
      "runner_pro",
      "runner_team",
      "runner_scale",
      "runner_max",
    ]);
    for (const id of CHECKOUT_TIER_IDS) {
      expect(normalizeCheckoutTier(id)).toBe(id);
    }
  });

  it("is case-insensitive and trims whitespace", () => {
    expect(normalizeCheckoutTier("PRO")).toBe("pro");
    expect(normalizeCheckoutTier("  Starter ")).toBe("starter");
  });

  it("falls back to the anchor SKU pro on invalid/missing values", () => {
    expect(DEFAULT_CHECKOUT_TIER).toBe("pro");
    expect(normalizeCheckoutTier("enterprise")).toBe("pro"); // not checkout-able
    expect(normalizeCheckoutTier("free")).toBe("pro"); // not checkout-able
    expect(normalizeCheckoutTier("bogus")).toBe("pro");
    expect(normalizeCheckoutTier("")).toBe("pro");
    expect(normalizeCheckoutTier(undefined)).toBe("pro");
    expect(normalizeCheckoutTier(null)).toBe("pro");
    expect(normalizeCheckoutTier([])).toBe("pro");
  });

  it("uses the first occurrence for repeated ?plan= params", () => {
    expect(normalizeCheckoutTier(["max", "solo"])).toBe("max");
    expect(normalizeCheckoutTier(["bogus", "solo"])).toBe("pro");
  });
});

// ---------------------------------------------------------------------------
// 2. GET /upgrade — locale-less forwarder (the docs-CTA URL shape)
// ---------------------------------------------------------------------------

function makeGetRequest(url: string): import("next/server").NextRequest {
  return new Request(url) as unknown as import("next/server").NextRequest;
}

describe("GET /upgrade (locale-less docs-CTA forwarder)", () => {
  it("307s to /en/upgrade preserving a valid plan", async () => {
    const { GET } = await import("@/app/upgrade/route");
    const res = GET(
      makeGetRequest("https://humangr.com/corelink/upgrade?plan=starter"),
    );
    expect(res.status).toBe(307);
    expect(res.headers.get("location")).toBe(
      "https://humangr.com/corelink/en/upgrade?plan=starter",
    );
  });

  it("defaults to plan=pro when ?plan= is missing", async () => {
    const { GET } = await import("@/app/upgrade/route");
    const res = GET(makeGetRequest("https://humangr.com/corelink/upgrade"));
    expect(res.status).toBe(307);
    expect(res.headers.get("location")).toBe(
      "https://humangr.com/corelink/en/upgrade?plan=pro",
    );
  });

  it("normalizes invalid plans to pro (never forwards junk)", async () => {
    const { GET } = await import("@/app/upgrade/route");
    const res = GET(
      makeGetRequest(
        "https://humangr.com/corelink/upgrade?plan=%3Cscript%3E",
      ),
    );
    expect(res.status).toBe(307);
    expect(res.headers.get("location")).toBe(
      "https://humangr.com/corelink/en/upgrade?plan=pro",
    );
  });
});

// ---------------------------------------------------------------------------
// 3. /[locale]/upgrade page — auth branches + tier card + auto checkout
// ---------------------------------------------------------------------------

async function mockClerkToken(token: string | null): Promise<void> {
  const clerk = await import("@clerk/nextjs/server");
  vi.mocked(clerk.auth).mockResolvedValue({
    getToken: async () => token,
  } as never);
}

async function renderUpgradePage(opts: {
  locale?: string;
  plan?: string | string[];
}): Promise<ReturnType<typeof render>> {
  const pageModule = await import("@/app/[locale]/upgrade/page");
  const UpgradePage = pageModule.default;
  const element = await UpgradePage({
    params: Promise.resolve({ locale: (opts.locale ?? "en") as never }),
    searchParams: Promise.resolve({ plan: opts.plan }),
  });
  return render(element as React.ReactElement);
}

describe("/[locale]/upgrade page", () => {
  beforeEach(async () => {
    const nav = await import("next/navigation");
    vi.mocked(nav.redirect).mockImplementation((path: string) => {
      throw new Error(`REDIRECT:${path}`);
    });
    // Default: keep the auto-started POST pending so render-shape tests
    // never race the redirect handoff.
    vi.stubGlobal(
      "fetch",
      vi.fn(() => new Promise<never>(() => {})),
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("signed-out: redirects to /sign-in with redirect_url back to the upgrade page (plan preserved)", async () => {
    await mockClerkToken(null);
    await expect(renderUpgradePage({ locale: "pt", plan: "max" })).rejects.toThrow(
      `REDIRECT:/sign-in?redirect_url=${encodeURIComponent("/pt/upgrade?plan=max")}`,
    );
  });

  it("signed-out: the normalized (defaulted) plan rides the sign-in round-trip", async () => {
    await mockClerkToken(null);
    await expect(renderUpgradePage({ locale: "en" })).rejects.toThrow(
      `REDIRECT:/sign-in?redirect_url=${encodeURIComponent("/en/upgrade?plan=pro")}`,
    );
  });

  it("signed-in: renders the tier card (name + rate-card price) with the auto-start button + live status", async () => {
    await mockClerkToken("clerk_test_jwt_xxx");
    const { container } = await renderUpgradePage({ plan: "starter" });

    expect(container.querySelector("[data-testid='upgrade-root']")).toBeTruthy();
    expect(screen.getByTestId("upgrade-heading")).toHaveTextContent(
      "Upgrade to Starter",
    );
    expect(screen.getByTestId("upgrade-price")).toHaveTextContent("$35");
    expect(screen.getByTestId("upgrade-price")).toHaveTextContent("/mo");
    // Accessible while-redirecting status (role=status → aria-live).
    expect(screen.getByRole("status")).toHaveTextContent(/Stripe Checkout/);
    // Manual fallback button is present (busy while the POST is in flight).
    expect(screen.getByTestId("upgrade-open-button")).toBeInTheDocument();
    expect(screen.getByTestId("upgrade-pricing-link")).toHaveAttribute(
      "href",
      "/en/pricing",
    );
  });

  it("signed-in: invalid ?plan= defaults to the Pro card", async () => {
    await mockClerkToken("clerk_test_jwt_xxx");
    await renderUpgradePage({ plan: "bogus" });
    expect(screen.getByTestId("upgrade-heading")).toHaveTextContent(
      "Upgrade to Pro",
    );
    expect(screen.getByTestId("upgrade-price")).toHaveTextContent("$50");
  });

  it("signed-in: legacy team plan renders a name-only card (no invented price)", async () => {
    await mockClerkToken("clerk_test_jwt_xxx");
    const { container } = await renderUpgradePage({ plan: "team" });
    expect(screen.getByTestId("upgrade-heading")).toHaveTextContent(
      "Upgrade to Team",
    );
    // `team` is off the public 6-tier rate card — no price line.
    expect(container.querySelector("[data-testid='upgrade-price']")).toBeNull();
  });

  it("signed-in: auto-fires the checkout POST and hands off to the Checkout URL", async () => {
    await mockClerkToken("clerk_test_jwt_xxx");

    const checkoutUrl = "https://checkout.stripe.com/c/pay/cs_test_auto";
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ checkout_url: checkoutUrl, session_id: "cs_test_auto" }),
    } as unknown as Response);
    vi.stubGlobal("fetch", fetchMock);

    const assignSpy = vi.fn();
    Object.defineProperty(window, "location", {
      value: { ...window.location, assign: assignSpy },
      configurable: true,
      writable: true,
    });

    await renderUpgradePage({ locale: "es", plan: "solo" });

    // No click — the page drives checkout immediately.
    await waitFor(() => expect(assignSpy).toHaveBeenCalledWith(checkoutUrl));
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0]!;
    // basePath-ABSOLUTE — a bare path posts to the apex marketing app (405).
    expect(url).toBe("/corelink/api/checkout/session");
    expect((init as RequestInit).method).toBe("POST");
    const headers = (init as RequestInit).headers as Record<string, string>;
    expect(headers["Accept"]).toBe("application/json");
    expect(JSON.parse((init as RequestInit).body as string)).toEqual({
      tier: "solo",
      locale: "es",
    });
  });
});

// ---------------------------------------------------------------------------
// 4. <UpgradeButton autoStart /> — auto-submit contract
// ---------------------------------------------------------------------------

describe("<UpgradeButton autoStart />", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("fires the checkout POST exactly once on mount, even under StrictMode double-effects", async () => {
    const fetchImpl = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        checkout_url: "https://checkout.stripe.com/c/pay/cs_test_strict",
        session_id: "cs_test_strict",
      }),
    } as unknown as Response);
    const redirectImpl = vi.fn();

    render(
      <React.StrictMode>
        <UpgradeButton
          tier="max"
          locale="de"
          autoStart
          fetchImpl={fetchImpl as unknown as typeof fetch}
          redirectImpl={redirectImpl}
        />
      </React.StrictMode>,
    );

    await waitFor(() =>
      expect(redirectImpl).toHaveBeenCalledWith(
        "https://checkout.stripe.com/c/pay/cs_test_strict",
      ),
    );
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(JSON.parse((fetchImpl.mock.calls[0]![1] as RequestInit).body as string)).toEqual({
      tier: "max",
      locale: "de",
    });
  });

  it("keeps the manual retry affordance when the auto-fired POST fails", async () => {
    const fetchImpl = vi.fn().mockResolvedValue({
      ok: false,
      status: 403,
      text: async () => "dpa_first_lock: accept current DPA before tier-select",
    } as unknown as Response);
    const redirectImpl = vi.fn();

    render(
      <UpgradeButton
        autoStart
        fetchImpl={fetchImpl as unknown as typeof fetch}
        redirectImpl={redirectImpl}
      />,
    );

    const err = await screen.findByTestId("upgrade-error");
    expect(err.textContent).toMatch(/403/);
    expect(redirectImpl).not.toHaveBeenCalled();
    // Button re-enabled so the user can retry manually.
    expect(screen.getByTestId("upgrade-open-button")).toBeEnabled();
  });

  it("does NOT auto-fire without the flag (existing click contract untouched)", async () => {
    const fetchImpl = vi.fn();
    render(
      <UpgradeButton
        fetchImpl={fetchImpl as unknown as typeof fetch}
        redirectImpl={vi.fn()}
      />,
    );
    // Effect would have run synchronously after mount — give it a beat.
    await new Promise((r) => setTimeout(r, 10));
    expect(fetchImpl).not.toHaveBeenCalled();
  });
});
