import { describe, it, expect } from "vitest";
import {
  nextStepFor,
  ALL_STEPS,
  type OnboardingStep,
} from "@/lib/onboarding-state";
import { isFreeplan } from "@/lib/validators";

/**
 * Walks the entire wizard with each plan and asserts that:
 *  - the free plan never visits the "billing" step
 *  - paid plans always pass through "billing"
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

describe("plan-driven step skipping", () => {
  it("free plan skips billing", () => {
    const visited = walkPlan("free");
    expect(visited).not.toContain("billing");
    expect(visited).toContain("pat");
    expect(visited).toContain("done");
  });

  it("paid plans include billing", () => {
    for (const plan of ["starter", "pro", "enterprise"] as const) {
      const visited = walkPlan(plan);
      expect(visited).toContain("billing");
    }
  });

  it("isFreeplan helper is consistent", () => {
    expect(isFreeplan("free")).toBe(true);
    expect(isFreeplan("starter")).toBe(false);
    expect(isFreeplan("pro")).toBe(false);
    expect(isFreeplan("enterprise")).toBe(false);
  });

  it("ALL_STEPS exposes 6 distinct steps", () => {
    expect(new Set(ALL_STEPS).size).toBe(ALL_STEPS.length);
    expect(ALL_STEPS.length).toBe(6);
  });
});
