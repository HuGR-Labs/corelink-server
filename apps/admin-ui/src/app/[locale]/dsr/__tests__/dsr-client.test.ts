import { describe, expect, it, vi } from "vitest";
import {
  DsrClientError,
  isMfaFresh,
  MFA_FRESH_MAX_AGE_MS,
  submitDsr,
} from "@/lib/dsr-client";
import { redact, redactString } from "@/lib/safe-log";

describe("dsr-client does NOT log session token (Test 11)", () => {
  it("never serialises the bearer token in error responses", async () => {
    const fakeFetch = vi.fn(async () => {
      return new Response("internal error", { status: 500 });
    });

    let err: unknown = null;
    try {
      await submitDsr(
        { action: "access" },
        {
          token: "Bearer corelink_pat_LEAKY_TOKEN_AB12",
          fetchImpl: fakeFetch as unknown as typeof fetch,
        },
      );
    } catch (e) {
      err = e;
    }
    expect(err).toBeInstanceOf(DsrClientError);
    const stringified = JSON.stringify({
      err: (err as DsrClientError).message,
      body: (err as DsrClientError).message,
    });
    expect(stringified).not.toContain("corelink_pat_LEAKY_TOKEN_AB12");
  });

  it("redactString strips bearer-shaped, JWT-shaped, and corelink tokens", () => {
    const samples = [
      "Authorization: Bearer abc.def.ghi",
      "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJ4In0.signature",
      "corelink_pat_AAA111",
      "corelink_ci_BBB222",
    ];
    for (const s of samples) {
      expect(redactString(s)).toContain("[REDACTED]");
    }
  });

  it("redact() replaces Authorization headers in nested objects", () => {
    const input = {
      headers: { Authorization: "Bearer secret_123", "X-Other": "ok" },
      nested: { token: "corelink_pat_AAA" },
    };
    const out = redact(input) as typeof input;
    expect(out.headers.Authorization).toBe("[REDACTED]");
    expect(out.headers["X-Other"]).toBe("ok");
    expect(JSON.stringify(out)).not.toContain("corelink_pat_AAA");
  });

  it("isMfaFresh respects the 30-minute CTRL-AUTH-010 window", () => {
    const now = 1_000_000_000_000;
    expect(isMfaFresh(now - 1_000, now)).toBe(true);
    expect(isMfaFresh(now - MFA_FRESH_MAX_AGE_MS + 1, now)).toBe(true);
    expect(isMfaFresh(now - MFA_FRESH_MAX_AGE_MS, now)).toBe(false);
    expect(isMfaFresh(null, now)).toBe(false);
    // Future-dated stamp must not be trusted (clock skew defence).
    expect(isMfaFresh(now + 5_000, now)).toBe(false);
  });

  it("maps 401 to mfa translation key and 429 to rate-limit key", async () => {
    const f401 = vi.fn(async () =>
      new Response("unauthorized", { status: 401 }),
    );
    const f429 = vi.fn(async () =>
      new Response("too many", { status: 429 }),
    );
    let e401: DsrClientError | null = null;
    let e429: DsrClientError | null = null;
    try {
      await submitDsr(
        { action: "access" },
        { token: "t", fetchImpl: f401 as unknown as typeof fetch },
      );
    } catch (e) {
      e401 = e as DsrClientError;
    }
    try {
      await submitDsr(
        { action: "access" },
        { token: "t", fetchImpl: f429 as unknown as typeof fetch },
      );
    } catch (e) {
      e429 = e as DsrClientError;
    }
    expect(e401?.translationKey).toBe("dsr.form.submit_error_mfa");
    expect(e429?.translationKey).toBe("dsr.form.submit_error_rate_limit");
  });

  it("posts to the canonical /v1/privacy/dsr/{action} path", async () => {
    let capturedUrl = "";
    const fakeFetch = vi.fn(async (url: string) => {
      capturedUrl = url;
      return new Response(
        JSON.stringify({
          request_id: "r1",
          action: "erasure",
          jurisdiction: "gdpr",
          sla_deadline: "2026-06-01T00:00:00Z",
          jwt_receipt: "abc.def.ghi",
        }),
        { status: 200 },
      );
    });
    await submitDsr(
      { action: "erasure", reason: "test" },
      {
        token: "t",
        fetchImpl: fakeFetch as unknown as typeof fetch,
        baseUrl: "https://api.test",
      },
    );
    expect(capturedUrl).toBe("https://api.test/v1/privacy/dsr/erasure");
  });
});
