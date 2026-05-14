import { describe, it, expect, beforeEach } from "vitest";
import {
  loadState,
  saveState,
  clearState,
  nextStep,
  prevStep,
  nextStepFor,
  setPlaintextPat,
  getPlaintextPat,
  clearPlaintextPat,
  type StorageLike,
} from "@/lib/onboarding-state";

function makeStorage(): StorageLike & { dump: () => Record<string, string> } {
  const data: Record<string, string> = {};
  return {
    getItem: (k) => (k in data ? (data[k] ?? null) : null),
    setItem: (k, v) => {
      data[k] = v;
    },
    removeItem: (k) => {
      delete data[k];
    },
    dump: () => ({ ...data }),
  };
}

describe("onboarding state persistence", () => {
  beforeEach(() => {
    clearPlaintextPat();
  });

  it("starts at tenant when nothing is stored", () => {
    const s = makeStorage();
    expect(loadState(s)).toEqual({ step: "tenant", tenantId: null });
  });

  it("round-trips step and tenantId", () => {
    const s = makeStorage();
    saveState({ step: "pat", tenantId: "ten_abc" }, s);
    expect(loadState(s)).toEqual({ step: "pat", tenantId: "ten_abc" });
  });

  it("recovers gracefully from corrupt JSON", () => {
    const s = makeStorage();
    s.setItem("corelink.onboarding.v1", "{not json");
    expect(loadState(s)).toEqual({ step: "tenant", tenantId: null });
  });

  it("refuses to persist PAT-shaped strings", () => {
    const s = makeStorage();
    expect(() =>
      saveState({ step: "pat", tenantId: "corelink_prod_LEAK" }, s),
    ).toThrowError(/credential-shaped/);
  });

  it("never writes the plaintext PAT to storage", () => {
    const s = makeStorage();
    setPlaintextPat("corelink_prod_secret_xyz");
    saveState({ step: "pat", tenantId: "ten_123" }, s);
    const dumped = JSON.stringify(s.dump());
    expect(dumped).not.toContain("corelink_prod_secret_xyz");
    expect(getPlaintextPat()).toBe("corelink_prod_secret_xyz");
    clearPlaintextPat();
    expect(getPlaintextPat()).toBeNull();
  });

  it("nextStep / prevStep traverse the wizard linearly", () => {
    expect(nextStep("tenant")).toBe("dpa");
    expect(nextStep("dpa")).toBe("region-plan");
    expect(nextStep("done")).toBe("done"); // saturates
    expect(prevStep("tenant")).toBe("tenant"); // saturates
    expect(prevStep("billing")).toBe("region-plan");
  });

  it("free plan skips the billing step in the wizard sequence", () => {
    expect(nextStepFor("region-plan", { plan: "free" })).toBe("pat");
    expect(nextStepFor("region-plan", { plan: "starter" })).toBe("billing");
    expect(nextStepFor("region-plan", { plan: "pro" })).toBe("billing");
    expect(nextStepFor("region-plan", { plan: "enterprise" })).toBe("billing");
  });
});
