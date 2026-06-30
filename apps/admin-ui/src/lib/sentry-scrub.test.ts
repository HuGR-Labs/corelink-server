import { describe, it, expect } from "vitest";
import { scrubText, scrubObject, scrubSentryEvent, REDACTED } from "./sentry-scrub";

describe("sentry-scrub", () => {
  describe("scrubText — secret/PII patterns", () => {
    it("redacts a CoreLink PAT token", () => {
      expect(scrubText("token corelink_pat_abc123DEF-456_xyz failed")).toBe(
        `token ${REDACTED} failed`,
      );
    });
    it("redacts an email address", () => {
      expect(scrubText("user alice.smith+test@example.co.uk not found")).toBe(
        `user ${REDACTED} not found`,
      );
    });
    it("redacts a bearer authorization value", () => {
      expect(scrubText("Authorization: Bearer eyJabc.def-ghi_123")).toContain(REDACTED);
      expect(scrubText("Bearer eyJabc.def")).toBe(REDACTED);
    });
    it("redacts a Stripe secret key", () => {
      expect(scrubText("key sk_test_51AbCdEfGhIjKlMnOp boom")).toBe(`key ${REDACTED} boom`);
    });
    it("redacts a Stripe webhook signing secret", () => {
      expect(scrubText("whsec_aBcDeF1234567890")).toBe(REDACTED);
    });
    it("passes through benign text unchanged", () => {
      const benign = "cache miss for object 9f86d081 in tenant bucket (200 OK)";
      expect(scrubText(benign)).toBe(benign);
    });
  });

  describe("scrubObject — default-deny by key", () => {
    it("redacts known-sensitive keys wholesale", () => {
      const out = scrubObject({
        authorization: "Bearer secret",
        cookie: "session=abc",
        "x-corelink-internal-auth": "internalkey",
        password: "hunter2",
        email: "a@b.com",
        keep: "ok",
      });
      expect(out.authorization).toBe(REDACTED);
      expect(out.cookie).toBe(REDACTED);
      expect(out["x-corelink-internal-auth"]).toBe(REDACTED);
      expect(out.password).toBe(REDACTED);
      expect(out.email).toBe(REDACTED);
      expect(out.keep).toBe("ok");
    });
    it("recurses and substring-scrubs nested values", () => {
      const out = scrubObject({
        note: "ping alice@example.com now",
        nested: { pat: "corelink_pat_zzz999" },
      });
      expect(out.note).toBe(`ping ${REDACTED} now`);
      expect((out.nested as Record<string, unknown>).pat).toBe(REDACTED);
    });
  });

  describe("scrubSentryEvent — message + exception bodies", () => {
    it("redacts message, exception value, extra and request", () => {
      const event = {
        message: "login failed for bob@corp.com",
        exception: { values: [{ type: "AuthError", value: "bad token corelink_pat_leak123" }] },
        extra: { stripe: "sk_live_AbCdEf123456789", note: "fine" },
        request: { headers: { authorization: "Bearer abc" }, query_string: "email=carol@x.io" },
      };
      const out = scrubSentryEvent(event) as typeof event;
      expect(out.message).toBe(`login failed for ${REDACTED}`);
      expect(out.exception.values[0]?.value).toBe(`bad token ${REDACTED}`);
      expect(out.extra.stripe).toBe(REDACTED);
      expect(out.extra.note).toBe("fine");
      expect(out.request.headers.authorization).toBe(REDACTED);
      expect(out.request.query_string).toBe(`email=${REDACTED}`);
    });
    it("is a no-op for a clean event", () => {
      const event = { message: "ok", extra: { a: 1 } };
      expect(scrubSentryEvent(event)).toEqual({ message: "ok", extra: { a: 1 } });
    });
  });
});
