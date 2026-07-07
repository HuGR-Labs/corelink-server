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
import { internalConsumerForPath } from "../src/index.js";
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
