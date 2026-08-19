/**
 * Regression (2026-08-19 red-team, "leaked shared internal key → cross-tenant
 * disclosure" + the recurring shared-master-amplifier theme):
 *
 * A privileged `/_internal/*` consumer must NEVER silently degrade to the broad
 * shared `CORELINK_INTERNAL_AUTH_KEY` when its dedicated key is unset. Before
 * this fix `resolveConsumerKey` fell back to the shared key for EVERY consumer
 * on a missing dedicated key, so a single leaked shared secret unlocked the
 * whole control plane the moment any one dedicated key went unset. Now the
 * dedicated-required consumers fail CLOSED (null → 503) instead; only `admin`
 * (no dedicated key in prod) keeps the shared-key path.
 */
import { describe, it, expect } from "vitest";
import { resolveConsumerKey } from "../src/lib/internal_auth.js";
import type { Env } from "../src/index.js";

const SHARED = "shared-internal-master-key-32-bytes-xx";
const DEDICATED = "dedicated-consumer-key-32-bytes-yyyyy";

// Only the shared key bound; every dedicated key unset.
const sharedOnly = { CORELINK_INTERNAL_AUTH_KEY: SHARED } as unknown as Env;

describe("resolveConsumerKey — the dedicated-required consumer never degrades to the shared key", () => {
  it("quota_read: unset dedicated key → null (fail-CLOSED), NOT the shared key", () => {
    // The LOW finding's surface: a cross-tenant read must never be unlockable by
    // the broad shared master key, even if its dedicated key goes unset.
    expect(resolveConsumerKey(sharedOnly, "quota_read")).toBeNull();
  });

  it("quota_read: a bound dedicated key is still used", () => {
    const env = {
      CORELINK_INTERNAL_AUTH_KEY: SHARED,
      CORELINK_QUOTA_READ_AUTH_KEY: DEDICATED,
    } as unknown as Env;
    expect(resolveConsumerKey(env, "quota_read")).toBe(DEDICATED);
  });

  // The other privileged consumers keep their shared fallback for now (their
  // edge exposure is closed wholesale by the Inc-2 /_internal/* network
  // lockdown, without churning their shared-fallback test fixtures).
  for (const consumer of ["pat_mint", "erase", "dsr_anchor", "runner_mint", "admin"] as const) {
    it(`${consumer}: unset dedicated key → the shared key (unchanged this increment)`, () => {
      expect(resolveConsumerKey(sharedOnly, consumer)).toBe(SHARED);
    });
  }
});
