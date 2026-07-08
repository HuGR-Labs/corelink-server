/**
 * Regression: the per-user DSR legitimacy anchor (`/_internal/dsr/anchor`, #634)
 * must gate on its OWN consumer key (`CORELINK_DSR_ANCHOR_AUTH_KEY`, held by
 * githugr), NOT the eraser's `CORELINK_ERASE_AUTH_KEY`.
 *
 * The go-live blocker: `internalConsumerForPath` had no `dsr_anchor` case, so
 * `/_internal/dsr/anchor` fell through to the `/_internal/dsr/*` → `erase`
 * catch-all. The worker front-gate then compared githugr's anchor key against the
 * ERASE key → 401 before the request ever reached the container's own anchor
 * gate — so binding/forwarding the anchor key alone could never unblock it. These
 * tests pin the two-authority split: anchor path → anchor key; erase paths → erase key.
 */
import { describe, it, expect } from "vitest";
import { internalConsumerForPath, isDsrEraseFanoutPath } from "../src/index.js";
import { resolveConsumerKey } from "../src/lib/internal_auth.js";
import type { Env } from "../src/index.js";

describe("internalConsumerForPath — DSR anchor is a distinct consumer", () => {
  it("routes /_internal/dsr/anchor to the dsr_anchor consumer (NOT erase)", () => {
    expect(internalConsumerForPath("/_internal/dsr/anchor")).toBe("dsr_anchor");
  });

  it("still routes the erase cascade (/_internal/dsr/*) to erase", () => {
    expect(internalConsumerForPath("/_internal/dsr/erase")).toBe("erase");
    expect(internalConsumerForPath("/_internal/dsr/erase/execute")).toBe("erase");
    expect(internalConsumerForPath("/_internal/cas/x/y/erase")).toBe("erase");
  });

  it("keeps the other consumers unchanged", () => {
    expect(internalConsumerForPath("/_internal/pat/mint")).toBe("pat_mint");
    expect(internalConsumerForPath("/_internal/admin/tenants")).toBe("admin");
  });
});

/**
 * Regression: the GDPR cross-residency erase FAN-OUT must be scoped to the
 * byte-erase surface only. The prior `startsWith("/_internal/dsr/")` swept the
 * global-D1 legitimacy anchor (`/_internal/dsr/anchor`) into the fan-out, which
 * fans out to all 4 regional workers and returns 502 unless every (anchor-
 * unprovisioned) region also 2xx's — the live GDPR-close blocker. `/access` and
 * `/portability` return a gathered export (the caller returns the LOCAL body, so
 * fanning them out is useless + only adds 502s); `/rectification` is a single
 * global-D1 write. Only `/erase` (deletes regional R2 bytes) and `/verify`
 * (re-confirms deletion, signs VerifiedComplete) are per-jurisdiction side
 * effects that MUST fan out. Under-fanning an erase is catastrophic (silent
 * residual bytes), so this test pins the exact set.
 */
describe("isDsrEraseFanoutPath — fan-out is scoped to the byte-erase surface", () => {
  it("fans out ONLY the byte-erase side-effect routes", () => {
    expect(isDsrEraseFanoutPath("/_internal/dsr/erase")).toBe(true);
    expect(isDsrEraseFanoutPath("/_internal/dsr/verify")).toBe(true);
  });

  it("does NOT fan out the global-D1 anchor (the live GDPR 502)", () => {
    expect(isDsrEraseFanoutPath("/_internal/dsr/anchor")).toBe(false);
  });

  it("does NOT fan out the gather/write routes (payload-body or global-D1 write)", () => {
    expect(isDsrEraseFanoutPath("/_internal/dsr/access")).toBe(false);
    expect(isDsrEraseFanoutPath("/_internal/dsr/portability")).toBe(false);
    expect(isDsrEraseFanoutPath("/_internal/dsr/rectification")).toBe(false);
  });

  it("does NOT fan out non-DSR internal or unknown routes", () => {
    expect(isDsrEraseFanoutPath("/_internal/cas/x/y/erase")).toBe(false);
    expect(isDsrEraseFanoutPath("/_internal/pat/mint")).toBe(false);
    expect(isDsrEraseFanoutPath("/_internal/dsr/")).toBe(false);
    expect(isDsrEraseFanoutPath("/_internal/dsr/erase/execute")).toBe(false);
  });
});

describe("resolveConsumerKey — dsr_anchor key selection", () => {
  const K64 = "c".repeat(64); // ≥ MIN_INTERNAL_AUTH_KEY_LEN (32)
  const SHARED = "s".repeat(64);

  function env(over: Partial<Env>): Env {
    return { CORELINK_INTERNAL_AUTH_KEY: SHARED, ...over } as unknown as Env;
  }

  it("returns the dedicated anchor key when set and properly sized", () => {
    expect(resolveConsumerKey(env({ CORELINK_DSR_ANCHOR_AUTH_KEY: K64 }), "dsr_anchor")).toBe(K64);
  });

  it("does NOT return the erase key for the anchor consumer", () => {
    const e = env({ CORELINK_ERASE_AUTH_KEY: "e".repeat(64), CORELINK_DSR_ANCHOR_AUTH_KEY: K64 });
    expect(resolveConsumerKey(e, "dsr_anchor")).toBe(K64);
    expect(resolveConsumerKey(e, "dsr_anchor")).not.toBe("e".repeat(64));
  });

  it("falls back to the shared key when the dedicated anchor key is unset", () => {
    expect(resolveConsumerKey(env({}), "dsr_anchor")).toBe(SHARED);
  });

  it("treats a sub-floor dedicated anchor key as absent (falls back to shared)", () => {
    expect(resolveConsumerKey(env({ CORELINK_DSR_ANCHOR_AUTH_KEY: "short" }), "dsr_anchor")).toBe(SHARED);
  });

  it("fails CLOSED (null) when neither the dedicated nor the shared key qualifies", () => {
    const e = { CORELINK_DSR_ANCHOR_AUTH_KEY: "short" } as unknown as Env;
    expect(resolveConsumerKey(e, "dsr_anchor")).toBeNull();
  });
});
