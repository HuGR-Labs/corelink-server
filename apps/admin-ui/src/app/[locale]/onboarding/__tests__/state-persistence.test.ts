import { describe, it, expect, beforeEach } from "vitest";
import {
  loadState,
  saveState,
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

describe("onboarding state persistence (collapsed wizard)", () => {
  beforeEach(() => {
    clearPlaintextPat();
  });

  it("returns null tenant_id when nothing is stored", () => {
    const s = makeStorage();
    expect(loadState(s)).toEqual({ tenantId: null });
  });

  it("round-trips tenantId across loads", () => {
    const s = makeStorage();
    saveState({ tenantId: "ten_abc" }, s);
    expect(loadState(s)).toEqual({ tenantId: "ten_abc" });
  });

  it("recovers gracefully from corrupt JSON", () => {
    const s = makeStorage();
    s.setItem("corelink.onboarding.v2", "{not json");
    expect(loadState(s)).toEqual({ tenantId: null });
  });

  it("refuses to persist PAT-shaped strings (CTRL-CRED-001)", () => {
    const s = makeStorage();
    expect(() => saveState({ tenantId: "corelink_pat_LEAK" }, s)).toThrowError(
      /credential-shaped/,
    );
  });

  it("never writes the plaintext PAT to storage", () => {
    const s = makeStorage();
    setPlaintextPat("corelink_pat_secret_xyz");
    saveState({ tenantId: "ten_123" }, s);
    const dumped = JSON.stringify(s.dump());
    expect(dumped).not.toContain("corelink_pat_secret_xyz");
    expect(getPlaintextPat()).toBe("corelink_pat_secret_xyz");
    clearPlaintextPat();
    expect(getPlaintextPat()).toBeNull();
  });
});
