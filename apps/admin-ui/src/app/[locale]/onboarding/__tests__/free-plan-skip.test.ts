import { describe, it, expect } from "vitest";
import {
  nextStepFor,
  ALL_STEPS,
  type OnboardingStep,
} from "@/lib/onboarding-state";
import { isFreeplan } from "@/lib/validators";

/**
 * Phase 0.C (PLG defer-billing) — wizard collapse.
 *
 * Before: the wizard had 6 steps (tenant → dpa → region-plan → billing
 * → pat → done) and the `free` plan skipped the in-wizard `billing`
 * step. Paid plans collected a raw `payment_method_id` text input — the
 * scaffold flagged in launch-readiness §2.
 *
 * After: the in-wizard `billing` step is REMOVED for every plan. The
 * money question is deferred to post-signup via Stripe-hosted Checkout
 * Session (`<UpgradeButton />` → `/api/checkout/session` → Stripe). The
 * wizard is now uniformly 5 steps regardless of plan choice. Free vs
 * paid divergence happens AFTER signup on the upgrade surface, not in
 * the wizard.
 */
function walkPlan(plan: "free" | "starter" | "pro" | "enterprise"): OnboardingStep[] {
  let cur: OnboardingStep = "tenant";
  const visited: OnboardingStep[] = [cur];
  while (cur !== "done") {
    const next = nextStepFor(cur, { plan });
    if (next === cur) break;
    cur = next;
    visited.push(cur);
  }
  return visited;
}

describe("defer-billing wizard walk", () => {
  it("no plan visits an in-wizard billing step", () => {
    for (const plan of ["free", "starter", "pro", "enterprise"] as const) {
      const visited = walkPlan(plan);
      // The "billing" step is gone from the type entirely; assert via
      // string compare so the test still meaningfully guards if the
      // step is ever re-added.
      expect(visited.map(String)).not.toContain("billing");
      expect(visited).toContain("pat");
      expect(visited).toContain("done");
    }
  });

  it("every plan walks the same 5-step sequence", () => {
    const expected: OnboardingStep[] = [
      "tenant",
      "dpa",
      "region-plan",
      "pat",
      "done",
    ];
    for (const plan of ["free", "starter", "pro", "enterprise"] as const) {
      expect(walkPlan(plan)).toEqual(expected);
    }
  });

  it("isFreeplan helper is consistent (kept for upgrade-surface gating)", () => {
    // The helper remains in `validators.ts` because the post-signup
    // `<UpgradeButton />` surface still distinguishes free vs paid
    // tiers (Checkout Session only created for paid tiers).
    expect(isFreeplan("free")).toBe(true);
    expect(isFreeplan("starter")).toBe(false);
    expect(isFreeplan("pro")).toBe(false);
    expect(isFreeplan("enterprise")).toBe(false);
  });

  it("ALL_STEPS exposes 5 distinct steps", () => {
    expect(new Set(ALL_STEPS).size).toBe(ALL_STEPS.length);
    expect(ALL_STEPS.length).toBe(5);
  });
});
