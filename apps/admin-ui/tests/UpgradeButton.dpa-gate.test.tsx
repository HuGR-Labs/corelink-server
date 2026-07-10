/**
 * UpgradeButton — DPA-first gate (INV-ONBOARD-DPA-FIRST).
 *
 * The backend `POST /v1/onboarding/tier-select` returns `403` with a
 * `dpa_required` body until the tenant has accepted the current DPA. This
 * suite verifies the wired UX:
 *
 *   403 dpa_required  → render the shared <DpaStep /> click-through
 *   scroll-to-end     → the accept button enables
 *   accept            → acceptDpaAction records the acceptance (with the
 *                       version + the hash of the notice the user saw)
 *   auto-retry        → the checkout POST re-fires and redirects to Stripe
 *
 * A 403 that is NOT `dpa_required` must still surface inline (no gate), so the
 * detection stays specific.
 */

import * as React from "react";
import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

// Mock the server action DpaStep calls on accept — return an immutable audit id.
const acceptDpaAction = vi.fn(async () => ({
  audit_event_id: "evt_test_dpa",
  dpa_version: "1.0.0",
  dpa_locale: "en",
  dpa_accepted_at: "2026-07-10T00:00:00.000Z",
}));
vi.mock("@/app/[locale]/onboarding/actions", () => ({
  acceptDpaAction: (input: unknown) => acceptDpaAction(input as never),
}));

import { UpgradeButton } from "@/components/UpgradeButton";

function dpaRequired403(): Response {
  return {
    ok: false,
    status: 403,
    text: async () =>
      JSON.stringify({
        error: "tier_select_failed",
        status: 403,
        body: { error: "dpa_required", message: "accept the current DPA first" },
      }),
  } as unknown as Response;
}

function checkoutOk(): Response {
  return {
    ok: true,
    json: async () => ({
      checkout_url: "https://checkout.stripe.com/c/pay/cs_test_after_dpa",
      session_id: "cs_test_after_dpa",
    }),
  } as unknown as Response;
}

describe("<UpgradeButton /> DPA-first gate", () => {
  afterEach(() => {
    vi.clearAllMocks();
  });

  it("403 dpa_required → shows the DPA gate, accepts, and auto-retries to Stripe", async () => {
    const fetchImpl = vi
      .fn()
      .mockResolvedValueOnce(dpaRequired403()) // first checkout POST
      .mockResolvedValueOnce(checkoutOk()); // auto-retry after accept
    const redirectImpl = vi.fn();

    render(
      <UpgradeButton
        tier="pro"
        locale="en"
        fetchImpl={fetchImpl as unknown as typeof fetch}
        redirectImpl={redirectImpl}
      />,
    );

    // 1. Click upgrade → 403 dpa_required → gate appears (no dead-end error).
    fireEvent.click(screen.getByTestId("upgrade-open-button"));
    await screen.findByTestId("upgrade-dpa-gate");
    expect(screen.queryByTestId("upgrade-error")).toBeNull();

    // 2. Accept is gated on scroll-to-end.
    const acceptBtn = screen.getByTestId("dpa-accept") as HTMLButtonElement;
    expect(acceptBtn.disabled).toBe(true);
    fireEvent.scroll(screen.getByTestId("dpa-scroller"));
    await waitFor(() => expect(acceptBtn.disabled).toBe(false));

    // 3. Accept → records acceptance → auto-retries checkout → Stripe redirect.
    fireEvent.click(acceptBtn);
    await waitFor(() =>
      expect(redirectImpl).toHaveBeenCalledWith(
        "https://checkout.stripe.com/c/pay/cs_test_after_dpa",
      ),
    );

    // The acceptance carried the canonical version + the hash of the notice the
    // user actually saw (64-hex SHA-256), for the en locale.
    expect(acceptDpaAction).toHaveBeenCalledTimes(1);
    const arg = acceptDpaAction.mock.calls[0]![0] as {
      dpaVersion: string;
      dpaLocale: string;
      noticeTextHash: string;
    };
    expect(arg.dpaVersion).toBe("1.0.0");
    expect(arg.dpaLocale).toBe("en");
    expect(arg.noticeTextHash).toMatch(/^[0-9a-f]{64}$/);

    // Two POSTs: the initial one and the post-accept retry.
    expect(fetchImpl).toHaveBeenCalledTimes(2);
  });

  it("a non-dpa 403 surfaces inline (no DPA gate)", async () => {
    const fetchImpl = vi.fn().mockResolvedValue({
      ok: false,
      status: 403,
      text: async () => "forbidden: some other reason",
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
    expect(screen.queryByTestId("upgrade-dpa-gate")).toBeNull();
    expect(redirectImpl).not.toHaveBeenCalled();
  });
});
