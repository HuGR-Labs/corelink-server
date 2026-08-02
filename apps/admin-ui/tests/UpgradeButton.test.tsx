/**
 * UpgradeButton — PLG defer-billing upgrade-trigger tests
 * (Phase 0.C — see `specs/_audits/2026-05-27-phase-0-execution-plan.md`
 * §2.C).
 *
 * Verifies the contract with `/api/checkout/session`:
 *   - POSTs with Accept: application/json + tier/locale body.
 *   - Hands off to the Checkout URL (test-injected redirectImpl —
 *     production uses window.location.assign).
 *   - Surfaces backend errors inline (non-silent).
 *   - Rejects non-HTTPS Checkout URLs (defense-in-depth against
 *     mis-configured backends).
 */

import * as React from "react";
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { UpgradeButton } from "@/components/UpgradeButton";

describe("<UpgradeButton />", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("POSTs tier+locale and redirects to the returned Checkout URL", async () => {
    const fetchImpl = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        checkout_url: "https://checkout.stripe.com/c/pay/cs_test_abc",
        session_id: "cs_test_abc",
      }),
    } as unknown as Response);
    const redirectImpl = vi.fn();

    render(
      <UpgradeButton
        tier="pro"
        locale="pt"
        fetchImpl={fetchImpl as unknown as typeof fetch}
        redirectImpl={redirectImpl}
      />,
    );

    fireEvent.click(screen.getByTestId("upgrade-open-button"));

    await waitFor(() => {
      expect(redirectImpl).toHaveBeenCalledWith(
        "https://checkout.stripe.com/c/pay/cs_test_abc",
      );
    });

    expect(fetchImpl).toHaveBeenCalledTimes(1);
    const [url, init] = fetchImpl.mock.calls[0]!;
    // basePath-ABSOLUTE. `fetch()` never gets Next's automatic basePath, and a
    // bare `/api/checkout/session` posts to the apex marketing app (405 live).
    // See `src/components/UpgradeButton.test.tsx` for the dedicated guard.
    expect(url).toBe("/corelink/api/checkout/session");
    expect((init as RequestInit).method).toBe("POST");
    const headers = (init as RequestInit).headers as Record<string, string>;
    expect(headers["Accept"]).toBe("application/json");
    expect(headers["Content-Type"]).toBe("application/json");
    expect(JSON.parse((init as RequestInit).body as string)).toEqual({
      tier: "pro",
      locale: "pt",
    });
  });

  it("surfaces backend errors inline (does not redirect on non-2xx)", async () => {
    const fetchImpl = vi.fn().mockResolvedValue({
      ok: false,
      status: 403,
      text: async () => "dpa_first_lock: accept current DPA before tier-select",
    } as unknown as Response);
    const redirectImpl = vi.fn();

    render(
      <UpgradeButton
        fetchImpl={fetchImpl as unknown as typeof fetch}
        redirectImpl={redirectImpl}
      />,
    );

    fireEvent.click(screen.getByTestId("upgrade-open-button"));

    const err = await screen.findByTestId("upgrade-error");
    expect(err.textContent).toMatch(/403/);
    expect(err.textContent).toMatch(/dpa_first_lock/);
    expect(redirectImpl).not.toHaveBeenCalled();
  });

  it("rejects non-HTTPS Checkout URLs (defense-in-depth)", async () => {
    const fetchImpl = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        checkout_url: "http://attacker.example/phish",
        session_id: "cs_test_evil",
      }),
    } as unknown as Response);
    const redirectImpl = vi.fn();

    render(
      <UpgradeButton
        fetchImpl={fetchImpl as unknown as typeof fetch}
        redirectImpl={redirectImpl}
      />,
    );

    fireEvent.click(screen.getByTestId("upgrade-open-button"));

    const err = await screen.findByTestId("upgrade-error");
    expect(err.textContent).toMatch(/non-HTTPS/);
    expect(redirectImpl).not.toHaveBeenCalled();
  });

  it("defaults to tier=pro and locale=en when props omitted", async () => {
    const fetchImpl = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        checkout_url: "https://checkout.stripe.com/c/pay/cs_test_default",
        session_id: "cs_test_default",
      }),
    } as unknown as Response);
    const redirectImpl = vi.fn();

    render(
      <UpgradeButton
        fetchImpl={fetchImpl as unknown as typeof fetch}
        redirectImpl={redirectImpl}
      />,
    );

    fireEvent.click(screen.getByTestId("upgrade-open-button"));

    await waitFor(() => {
      expect(redirectImpl).toHaveBeenCalled();
    });
    const [, init] = fetchImpl.mock.calls[0]!;
    expect(JSON.parse((init as RequestInit).body as string)).toEqual({
      tier: "pro",
      locale: "en",
    });
  });
});
