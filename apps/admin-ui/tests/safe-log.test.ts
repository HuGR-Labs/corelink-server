import { describe, it, expect } from "vitest";
import { safeLog, sanitizeContext } from "@/lib/safe-log";

describe("safeLog PII redaction", () => {
  it("retains allowlisted fields", () => {
    const out = sanitizeContext({
      request_id: "req-1",
      user_id_hash: "abc",
      error_code: "E_FOO",
      route: "/dashboard",
    });
    expect(out).toEqual({
      request_id: "req-1",
      user_id_hash: "abc",
      error_code: "E_FOO",
      route: "/dashboard",
    });
  });

  it("strips denylisted fields", () => {
    const out = sanitizeContext({
      email: "x@y.com",
      pat: "corelink_secret",
      tenant_id: "tnt_1",
      password: "hunter2",
      mfa_code: "123456",
      authorization: "Bearer abc",
      request_id: "req-2",
    });
    expect(out).toEqual({ request_id: "req-2" });
    expect("email" in out).toBe(false);
    expect("pat" in out).toBe(false);
  });

  it("drops non-allowlisted unknown keys (even safe-looking ones)", () => {
    const out = sanitizeContext({
      random_field: "ok",
      hello: "world",
      request_id: "r",
    });
    expect(out).toEqual({ request_id: "r" });
  });

  it("drops object/array values even on allowlisted keys", () => {
    const out = sanitizeContext({
      request_id: { nested: "leak" } as unknown,
      route: ["/x"] as unknown,
      error_code: "E_OK",
    });
    expect(out).toEqual({ error_code: "E_OK" });
  });

  it("property test: 200 random objects with denylisted keys never leak", () => {
    for (let i = 0; i < 200; i++) {
      const ctx: Record<string, unknown> = {
        email: `user${i}@example.com`,
        pat: `corelink_${i}`,
        tenant_id: `tnt_${i}`,
        request_id: `req-${i}`,
      };
      const sanitized = sanitizeContext(ctx);
      expect("email" in sanitized).toBe(false);
      expect("pat" in sanitized).toBe(false);
      expect("tenant_id" in sanitized).toBe(false);
      expect(sanitized["request_id"]).toBe(`req-${i}`);
    }
  });

  it("safeLog produces stable shape", () => {
    const r = safeLog("error", "boom", { request_id: "r", email: "x@y.com" });
    expect(r.level).toBe("error");
    expect(r.msg).toBe("boom");
    expect(r.context).toEqual({ request_id: "r" });
  });
});
