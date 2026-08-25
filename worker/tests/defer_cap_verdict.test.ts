import { describe, expect, it } from "vitest";
import { deferCapVerdictFor, quotaExceededResponse } from "../src/index.js";

// Deferring the DO meter hop is only sound for a READ: a mutation that commits
// and is THEN refused would be a correctness bug, not a latency win. These cases
// are the boundary of that soundness argument.
describe("deferCapVerdictFor", () => {
  const base = {
    method: "GET",
    isStorageMutating: false,
    asyncMeterMode: "on",
    meter: true,
    tierD1Error: false,
  } as const;

  it("defers a warm read", () => {
    expect(deferCapVerdictFor(base)).toBe(true);
    expect(deferCapVerdictFor({ ...base, method: "HEAD" })).toBe(true);
  });

  it("never defers a mutation, by verb or by storage effect", () => {
    for (const method of ["PUT", "POST", "DELETE", "PATCH"]) {
      expect(deferCapVerdictFor({ ...base, method })).toBe(false);
    }
    expect(deferCapVerdictFor({ ...base, isStorageMutating: true })).toBe(false);
  });

  it("never defers when the storage verdict would still cost a D1 round trip", () => {
    for (const mode of [undefined, "off", "shadow", "1"]) {
      expect(deferCapVerdictFor({ ...base, asyncMeterMode: mode })).toBe(false);
    }
  });

  it("never defers an unmetered (fan-out) request or an unconfirmed tier", () => {
    expect(deferCapVerdictFor({ ...base, meter: false })).toBe(false);
    expect(deferCapVerdictFor({ ...base, tierD1Error: true })).toBe(false);
  });
});

describe("quotaExceededResponse", () => {
  it("is the 429 shape both the inline and the deferred gate return", async () => {
    const res = quotaExceededResponse("Monthly request quota exceeded: limit is 5 (tier: solo)", 42, "req-1");
    expect(res.status).toBe(429);
    expect(res.headers.get("Retry-After")).toBe("42");
    expect(res.headers.get("X-Request-Id")).toBe("req-1");
    expect(res.headers.get("Content-Type")).toBe("application/json");
    await expect(res.json()).resolves.toEqual({
      error: "QUOTA_EXCEEDED",
      message: "Monthly request quota exceeded: limit is 5 (tier: solo)",
      request_id: "req-1",
    });
  });
});
