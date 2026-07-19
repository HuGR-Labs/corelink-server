/**
 * Frontend-audit finding (2026-07-11): the checkout success/cancel URL host was
 * built from the client-suppliable `x-forwarded-host` header, so a direct caller
 * could steer Stripe's post-checkout redirect off-domain. The host is now
 * allow-listed to the SOLE canonical app host `humangr.com` (+ localhost dev);
 * the retired `corelink-admin`/`corelink-app` subdomains are no longer accepted.
 * This guards the predicate so a future edit can't silently re-open it.
 */

import { describe, expect, it } from "vitest";
import { isAllowedRedirectHost } from "./route";

describe("checkout redirect host allow-list", () => {
  it("accepts the canonical prod app host", () => {
    // `humangr.com` is the sole canonical app host (path-mounted at /corelink).
    expect(isAllowedRedirectHost("humangr.com")).toBe(true);
  });

  it("accepts localhost for dev", () => {
    expect(isAllowedRedirectHost("localhost")).toBe(true);
    expect(isAllowedRedirectHost("localhost:3000")).toBe(true);
    expect(isAllowedRedirectHost("127.0.0.1:3001")).toBe(true);
  });

  it("REJECTS attacker-supplied / off-domain hosts (incl. the retired subdomains)", () => {
    for (const h of [
      "evil.com",
      "humangr.com.evil.com",
      "evil.humangr.com", // not the canonical apex
      "humangr.com:8080@evil.com",
      "nothumangr.com",
      "corelink-admin.humangr.com", // retired — no longer accepted
      "corelink-docs.humangr.com", // not a checkout-redirect target
      "",
    ]) {
      expect(isAllowedRedirectHost(h), h).toBe(false);
    }
  });

  it("is case-insensitive on the host but still apex-bound", () => {
    expect(isAllowedRedirectHost("HumanGR.com")).toBe(true);
    expect(isAllowedRedirectHost("EVIL.COM")).toBe(false);
  });
});
