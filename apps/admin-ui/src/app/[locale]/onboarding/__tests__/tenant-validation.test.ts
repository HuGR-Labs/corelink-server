import { describe, it, expect } from "vitest";
import { validateTenantName } from "@/lib/validators";

describe("validateTenantName", () => {
  it("accepts a normal name", () => {
    expect(validateTenantName("acme-corp")).toEqual({ ok: true });
  });

  it("rejects empty input", () => {
    expect(validateTenantName("")).toEqual({ ok: false, reason: "empty" });
  });

  it("rejects too-short names", () => {
    expect(validateTenantName("ab")).toEqual({
      ok: false,
      reason: "too_short",
    });
  });

  it("rejects too-long names", () => {
    const long = "a".repeat(65);
    expect(validateTenantName(long)).toEqual({
      ok: false,
      reason: "too_long",
    });
  });

  it("accepts exactly 64 chars", () => {
    expect(validateTenantName("a".repeat(64))).toEqual({ ok: true });
  });

  it("rejects underscores and spaces", () => {
    expect(validateTenantName("bad_name")).toEqual({
      ok: false,
      reason: "invalid_chars",
    });
    expect(validateTenantName("bad name")).toEqual({
      ok: false,
      reason: "invalid_chars",
    });
  });

  it("accepts digits and hyphens", () => {
    expect(validateTenantName("acme-2026")).toEqual({ ok: true });
  });
});
