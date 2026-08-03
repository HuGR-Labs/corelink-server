/**
 * Regression lock for the CONSENT-SURFACE TAKEDOWN.
 *
 * The four screens under `src/app/[locale]/consent/*` call five endpoints that
 * nothing in this repo serves (`/v1/consent/active`, `/v1/consent/grant`,
 * `/v1/consent/{id}/withdraw`, `/v1/consent/history`, `/v1/subprocessors`) —
 * see `src/lib/consent-api.ts` for the measurement. The severe case was
 * `/[locale]/consent/new`, which was on the middleware PUBLIC allowlist: an
 * ANONYMOUS visitor got a 200 "Grant consent — CoreLink" page that collected
 * legal basis, data categories, retention period, sub-processors and
 * withdrawal method, stamped an ISO capture timestamp, and persisted nothing —
 * against hardcoded STUB notice text.
 *
 * These tests assert the takedown BEHAVIOURALLY: each page component must call
 * Next's `notFound()` and must NOT render its flow.
 *
 * MUTATION PROOF — flip `CONSENT_UI_RETIRED` to `false` in
 * `src/app/[locale]/consent/retired.ts` and this file goes red (every case
 * fails: `notFound` is not called and the flow renders).
 *
 * Note the page modules are imported through the real `retired.ts`, so the
 * kill-switch itself is under test — not a re-declared copy of it. The consent
 * COMPONENTS are stubbed so that, on a reverted run, the pages render cleanly
 * and the missing `notFound()` is the only thing that fails; that keeps the
 * mutation signal about the takedown rather than about a provider throw.
 */

import * as React from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";

// Next's real `notFound()` throws a control-flow error. Mirror that (the page
// bodies must not continue past the guard) while recording the call.
// `vi.hoisted` because `vi.mock` factories are hoisted above the module body;
// a plain `const` here would be in the TDZ when the factory runs.
const { notFound, flowRendered } = vi.hoisted(() => ({
  notFound: vi.fn(() => {
    throw new Error("NEXT_HTTP_ERROR_FALLBACK;404");
  }),
  // Stub marker for the four client islands: each records that it was reached
  // — on a reverted run these render and `flowRendered` flips, which is the
  // second, independent half of the assertion.
  flowRendered: vi.fn(),
}));
vi.mock("next/navigation", () => ({ notFound }));
vi.mock("@/components/consent/ConsentCaptureFlow", () => ({
  ConsentCaptureFlow: () => {
    flowRendered("ConsentCaptureFlow");
    return React.createElement("div");
  },
}));
vi.mock("@/components/consent/ConsentDashboard", () => ({
  ConsentDashboard: () => {
    flowRendered("ConsentDashboard");
    return React.createElement("div");
  },
}));
vi.mock("@/components/consent/ConsentHistory", () => ({
  ConsentHistory: () => {
    flowRendered("ConsentHistory");
    return React.createElement("div");
  },
}));
vi.mock("@/components/consent/WithdrawForm", () => ({
  WithdrawForm: () => {
    flowRendered("WithdrawForm");
    return React.createElement("div");
  },
}));

import { CONSENT_UI_RETIRED } from "@/app/[locale]/consent/retired";
import ConsentNewPage from "@/app/[locale]/consent/new/page";
import ConsentDashboardPage from "@/app/[locale]/consent/page";
import ConsentHistoryPage from "@/app/[locale]/consent/history/page";
import ConsentWithdrawPage from "@/app/[locale]/consent/withdraw/[id]/page";

/** Invoke a page component (server components here are plain functions). */
async function invoke(page: () => unknown): Promise<void> {
  await page();
}

describe("consent surface is retired until a backend exists", () => {
  beforeEach(() => {
    notFound.mockClear();
    flowRendered.mockClear();
  });

  it("keeps the kill-switch engaged", () => {
    // The switch itself. Flipping it is the documented ONE-LINE reversal, and
    // it is intended to be flipped only together with a landed backend.
    expect(CONSENT_UI_RETIRED).toBe(true);
  });

  it("PUBLIC ROUTE: /[locale]/consent/new 404s for an anonymous visitor", async () => {
    // This is the load-bearing case: `/consent/new` is still on the middleware
    // public allowlist (`lib/route-matcher.ts` — deliberately, so the 404 is
    // served directly rather than after a pointless Clerk round-trip), which
    // means the PAGE is the only thing standing between an anonymous visitor
    // and a consent form that stores nothing.
    await expect(
      invoke(() => ConsentNewPage({ params: Promise.resolve({ locale: "en" }) })),
    ).rejects.toThrow(/404/);
    expect(notFound).toHaveBeenCalledTimes(1);
    expect(flowRendered).not.toHaveBeenCalled();
  });

  it("/[locale]/consent 404s", async () => {
    await expect(
      invoke(() => ConsentDashboardPage({ params: Promise.resolve({ locale: "en" }) })),
    ).rejects.toThrow(/404/);
    expect(notFound).toHaveBeenCalledTimes(1);
    expect(flowRendered).not.toHaveBeenCalled();
  });

  it("/[locale]/consent/history 404s", async () => {
    await expect(invoke(() => ConsentHistoryPage())).rejects.toThrow(/404/);
    expect(notFound).toHaveBeenCalledTimes(1);
    expect(flowRendered).not.toHaveBeenCalled();
  });

  it("/[locale]/consent/withdraw/[id] 404s", async () => {
    await expect(
      invoke(() =>
        ConsentWithdrawPage({ params: Promise.resolve({ locale: "en", id: "csn_42" }) }),
      ),
    ).rejects.toThrow(/404/);
    expect(notFound).toHaveBeenCalledTimes(1);
    expect(flowRendered).not.toHaveBeenCalled();
  });
});
