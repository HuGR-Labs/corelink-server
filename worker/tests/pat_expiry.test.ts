import { describe, expect, it } from "vitest";
import { isPatExpiryLive } from "../src/lib/pat_expiry.js";

describe("PAT expiry contract", () => {
  const now = 1_750_000_000_000;

  it("treats zero as the canonical never-expiring sentinel", () => {
    expect(isPatExpiryLive(0, now)).toBe(true);
  });

  it("requires a positive expiry to be strictly in the future", () => {
    expect(isPatExpiryLive(now + 1, now)).toBe(true);
    expect(isPatExpiryLive(now, now)).toBe(false);
    expect(isPatExpiryLive(now - 1, now)).toBe(false);
  });
});
