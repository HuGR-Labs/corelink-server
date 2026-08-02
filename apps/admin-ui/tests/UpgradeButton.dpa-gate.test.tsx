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
import { describe, it, expect, vi, afterEach, beforeAll } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";

// The exact shape DpaStep passes to acceptDpaAction on accept.
interface DpaAcceptInput {
  tenantId: string;
  dpaVersion: string;
  dpaLocale: string;
  noticeTextHash: string;
  uiCaptureTs: number;
}

// Mock the server action DpaStep calls on accept — return an immutable audit id.
const acceptDpaAction = vi.fn(async (_input: DpaAcceptInput) => ({
  audit_event_id: "evt_test_dpa",
  dpa_version: "1.0.0",
  dpa_locale: "en",
  dpa_accepted_at: "2026-07-10T00:00:00.000Z",
}));
vi.mock("@/app/[locale]/onboarding/actions", () => ({
  acceptDpaAction: (input: DpaAcceptInput) => acceptDpaAction(input),
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

/**
 * Take the LAZY-MODULE TRANSFORM out of every timed window in this file.
 *
 * On the `dpa_required` branch `UpgradeButton` does `await import(
 * "@/lib/dpa-notice")` (`src/components/UpgradeButton.tsx`) — correct in
 * production, since that specifier drags in the whole `@/content/load` map
 * (12 markdown modules + sub-processors) which the rare 403 path is the only
 * consumer of. In vitest, though, the FIRST evaluation of that specifier makes
 * vite resolve + transform that chain ON DEMAND, and that cost lands inside the
 * `findByTestId` window below, whose default budget is 1000 ms.
 *
 * Measured on the shared 12-core Mac, 2026-08-01 (see the fix commit):
 *
 *     cold `await import("@/lib/dpa-notice")`   55.6 ms   <- 98% of the window
 *     `loadDpaNotice()` (map read + hash)        0.8 ms
 *     `sha256Hex()` alone                        0.1 ms   <- NOT the cost
 *     same import once warm                      0.0 ms
 *
 *     click -> gate rendered, module cold:
 *       idle box            68 / 75 / 78 / 88 / 98 ms   (n=5)
 *       inside a full-suite run, load-avg 11->42
 *                          159 / 168 / 184 ms           (n=3)
 *
 * So the window is TRANSFORM-bound, not crypto-bound, and it inflates with CPU
 * contention. The observed failure was a full-suite run at a 15-min load
 * average of 103.5 on 12 cores (~8.6x oversubscribed); the test died at
 * 1061 ms, i.e. the 1000 ms `findBy` budget plus teardown — not a hang.
 *
 * Pre-resolving here collapses that 55.6 ms to 0 and leaves the window
 * measuring only what is actually under test: the 403 -> gate wiring.
 */
beforeAll(async () => {
  await import("@/lib/dpa-notice");
});

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
    // Explicit budget, and it is a NET — not the mechanism. The `beforeAll`
    // pre-resolve above is what makes this window fast, but it is coupled to
    // the component's import specifier by hand: if `UpgradeButton` ever lazily
    // imports something else, the pre-resolve silently becomes a no-op and the
    // cold transform comes back into this window. This budget is what survives
    // that rot.
    //
    // 4000 ms is derived, not round: ~22x the worst value measured under a
    // saturated full-suite run (184 ms) and ~51x the idle median (78 ms), so
    // CPU contention alone can never trip it — while still firing INSIDE
    // vitest's 5000 ms default `testTimeout`, so a genuine hang fails here
    // with "gate never appeared" rather than as an opaque test timeout.
    await screen.findByTestId("upgrade-dpa-gate", undefined, { timeout: 4000 });
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
    const arg = acceptDpaAction.mock.calls[0]![0];
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
