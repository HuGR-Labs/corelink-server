/**
 * Unit tests for the pricing single-source-of-truth (src/lib/pricing.ts).
 *
 * The 5 self-serve runner SKUs (runner_starter…runner_max) are the focus:
 * once an id is in `CHECKOUT_TIER_IDS`, the `/api/checkout/session`
 * `PAID_TIERS` gate (`new Set(CHECKOUT_TIER_IDS)`) AND the `/upgrade?plan=`
 * validator (`normalizeCheckoutTier` / `isCheckoutTierId`) BOTH accept it —
 * so proving it here proves both surfaces (they can't drift; they derive
 * from the same const). The runner ids MUST be byte-identical to the
 * backend tier-select contract — a mismatch silently breaks checkout.
 */

import { describe, it, expect } from "vitest";
import {
  CHECKOUT_TIER_IDS,
  TIERS,
  isCheckoutTierId,
  normalizeCheckoutTier,
  DEFAULT_CHECKOUT_TIER,
  type Tier,
} from "@/lib/pricing";

// The FROZEN backend contract — byte-identical ids. If the backend ever
// renames a runner SKU this list is the canary that catches the drift.
const RUNNER_TIER_IDS = [
  "runner_starter",
  "runner_pro",
  "runner_team",
  "runner_scale",
  "runner_max",
] as const;

// The gate the checkout route builds verbatim: `new Set(CHECKOUT_TIER_IDS)`.
const PAID_TIERS = new Set<string>(CHECKOUT_TIER_IDS);

describe("runner SKUs — checkout-able contract", () => {
  it("every runner id is in CHECKOUT_TIER_IDS", () => {
    for (const id of RUNNER_TIER_IDS) {
      expect(CHECKOUT_TIER_IDS as readonly string[]).toContain(id);
    }
  });

  it("every runner id passes the /api/checkout/session PAID_TIERS gate", () => {
    for (const id of RUNNER_TIER_IDS) {
      expect(PAID_TIERS.has(id)).toBe(true);
    }
  });

  it("every runner id passes the isCheckoutTierId type guard", () => {
    for (const id of RUNNER_TIER_IDS) {
      expect(isCheckoutTierId(id)).toBe(true);
    }
  });

  it("every runner id survives the /upgrade?plan= validator unchanged", () => {
    for (const id of RUNNER_TIER_IDS) {
      // exact
      expect(normalizeCheckoutTier(id)).toBe(id);
      // case-insensitive (the validator lower-cases)
      expect(normalizeCheckoutTier(id.toUpperCase())).toBe(id);
      // repeated param → first occurrence honoured
      expect(normalizeCheckoutTier([id, "pro"])).toBe(id);
    }
  });

  it("does not accept a mistyped/invented runner id", () => {
    for (const bogus of ["runner", "runner_startr", "runners_max", "runner_enterprise"]) {
      expect(isCheckoutTierId(bogus)).toBe(false);
      expect(PAID_TIERS.has(bogus)).toBe(false);
      // falls back to the anchor SKU, never a runner tier
      expect(normalizeCheckoutTier(bogus)).toBe(DEFAULT_CHECKOUT_TIER);
    }
  });
});

describe("runner SKUs — display catalog", () => {
  const runnerCards: Tier[] = TIERS.filter((t) => t.group === "runner");

  it("renders exactly 5 runner cards, one per contract id", () => {
    expect(runnerCards).toHaveLength(5);
    expect(runnerCards.map((c) => c.id).sort()).toEqual(
      [...RUNNER_TIER_IDS].sort(),
    );
  });

  it("carries the owner-ratified price ladder", () => {
    const priceById = Object.fromEntries(
      runnerCards.map((c) => [c.id, c.price]),
    );
    expect(priceById).toEqual({
      runner_starter: "$16",
      runner_pro: "$40",
      runner_team: "$100",
      runner_scale: "$200",
      runner_max: "$400",
    });
  });

  it("every runner card is a monthly subscription with concurrency + vCPU-h bullets", () => {
    for (const card of runnerCards) {
      expect(card.cadence).toBe("/mo");
      expect(card.features.some((f) => /concurrent runners/i.test(f))).toBe(true);
      expect(card.features.some((f) => /vCPU-hours\/mo/i.test(f))).toBe(true);
      // CTA drives the self-serve checkout entry point for this exact id.
      expect(card.ctaHref).toBe(`/upgrade?plan=${card.id}`);
    }
  });

  it("keeps runner cards on their own product axis (group !== cache)", () => {
    for (const card of runnerCards) {
      expect(card.group).toBe("runner");
    }
    // and the cache ladder never leaks into the runner section
    const cacheCards = TIERS.filter((t) => (t.group ?? "cache") === "cache");
    expect(cacheCards.every((c) => !c.id.startsWith("runner_"))).toBe(true);
  });
});

describe("catalog integrity", () => {
  it("has no duplicate tier ids", () => {
    const ids = TIERS.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("every non-mailto card CTA points at a routable checkout/sign-up path", () => {
    for (const card of TIERS) {
      if (card.ctaHref.startsWith("mailto:")) continue;
      expect(card.ctaHref.startsWith("/")).toBe(true);
    }
  });
});
