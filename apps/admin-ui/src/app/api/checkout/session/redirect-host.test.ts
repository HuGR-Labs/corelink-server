/**
 * Frontend-audit finding (2026-07-11): the checkout success/cancel URL host was
 * built from the client-suppliable `x-forwarded-host` header, so a direct caller
 * could steer Stripe's post-checkout redirect off-domain. The host is now
 * allow-listed to `corelink-*.humangr.com` (+ localhost dev). This guards the
 * predicate so a future edit can't silently re-open it.
 */

import { describe, expect, it } from "vitest";
import { isAllowedRedirectHost } from "./route";

describe("checkout redirect host allow-list", () => {
  it("accepts the real prod app hosts", () => {
    for (const h of [
      "corelink-admin.humangr.com",
      "humangr.com", // public app apex (path-mounted at /corelink)
      "corelink-docs.humangr.com",
    ]) {
      expect(isAllowedRedirectHost(h), h).toBe(true);
    }
  });

  it("accepts localhost for dev", () => {
    expect(isAllowedRedirectHost("localhost")).toBe(true);
    expect(isAllowedRedirectHost("localhost:3000")).toBe(true);
    expect(isAllowedRedirectHost("127.0.0.1:3001")).toBe(true);
  });

  it("REJECTS attacker-supplied / off-domain hosts", () => {
    for (const h of [
      "evil.com",
      "corelink-admin.humangr.com.evil.com",
      "evil.humangr.com", // not the corelink-* shape
      "corelink-admin.humangr.com:8080@evil.com",
      "notcorelink-admin.humangr.com",
      "",
    ]) {
      expect(isAllowedRedirectHost(h), h).toBe(false);
    }
  });

  it("is case-insensitive on the host but still shape-bound", () => {
    expect(isAllowedRedirectHost("CoreLink-Admin.HumanGR.com")).toBe(true);
    expect(isAllowedRedirectHost("EVIL.COM")).toBe(false);
  });
});
