/**
 * Unit tests for the pricing library — Phase 0.E 3-tier launch shape.
 *
 * Covers tier boundaries, hard-cap behaviour, Enterprise contact-sales
 * handling, annual ("2 months free") discount, defensive clamping,
 * format honesty, and the absence of any `provisional` flag.
 */

import { describe, it, expect } from "vitest";

import {
  ANNUAL_DISCOUNT_RATE,
  CANONICAL_TIERS,
  DEFAULT_USAGE,
  TIER_RATE_CARD,
  applyPeriodDiscount,
  computeBreakEven,
  estimateAllTiers,
  estimateTier,
  formatUsd,
  recommendTier,
} from "./pricing";
import type { TierShape, UsageInputs } from "./pricing";

const zeroUsage: UsageInputs = {
  casGbStored: 0,
  requestsPerMonth: 0,
};

describe("CANONICAL_TIERS — launch shape", () => {
  it("is exactly Free / Pro / Enterprise (3 tiers)", () => {
    expect([...CANONICAL_TIERS]).toEqual(["free", "pro", "enterprise"]);
  });

  it("contains no `provisional` field on any tier", () => {
    for (const tier of CANONICAL_TIERS) {
      const card: TierShape = TIER_RATE_CARD[tier];
      expect((card as unknown as Record<string, unknown>).provisional).toBeUndefined();
    }
  });

  it("Free is $0", () => {
    expect(TIER_RATE_CARD.free.usdMonthlyBase).toBe(0);
  });

  it("Pro is $25/mo or $250/yr per pricing-benchmarks §5", () => {
    expect(TIER_RATE_CARD.pro.usdMonthlyBase).toBe(25);
    expect(TIER_RATE_CARD.pro.usdAnnualList).toBe(250);
  });

  it("Enterprise is contact-sales (no anchor)", () => {
    expect(TIER_RATE_CARD.enterprise.usdMonthlyBase).toBeNull();
    expect(TIER_RATE_CARD.enterprise.usdAnnualList).toBeNull();
  });

  it("Enterprise gates BYOK + SSO + SLA + DPA + audit-log export", () => {
    const ent = TIER_RATE_CARD.enterprise;
    expect(ent.byok).toBe(true);
    expect(ent.sso).toBe(true);
    expect(ent.slaCredits).toBe(true);
    expect(ent.dpa).toBe(true);
    expect(ent.auditLogExport).toBe(true);
  });

  it("Free and Pro are hard-capped (no silent overage billing)", () => {
    expect(TIER_RATE_CARD.free.hardCap).toBe(true);
    expect(TIER_RATE_CARD.pro.hardCap).toBe(true);
  });

  it("Free quotas match CF R2 free tier (10 GB / 500k req)", () => {
    expect(TIER_RATE_CARD.free.includedCasGb).toBe(10);
    expect(TIER_RATE_CARD.free.includedRequests).toBe(500_000);
  });

  it("Pro quotas are 500 GB / 20M req per pricing-benchmarks §5", () => {
    expect(TIER_RATE_CARD.pro.includedCasGb).toBe(500);
    expect(TIER_RATE_CARD.pro.includedRequests).toBe(20_000_000);
  });
});

describe("estimateTier — boundaries", () => {
  it("Free with zero usage costs $0 and fits without overage", () => {
    const r = estimateTier("free", zeroUsage);
    expect(r.monthlyTotal).toBe(0);
    expect(r.fitsWithoutOverage).toBe(true);
  });

  it("Pro at exactly its included quotas costs the flat base", () => {
    const card = TIER_RATE_CARD.pro;
    const usage: UsageInputs = {
      casGbStored: card.includedCasGb,
      requestsPerMonth: card.includedRequests,
    };
    const r = estimateTier("pro", usage);
    expect(r.fitsWithoutOverage).toBe(true);
    expect(r.monthlyTotal).toBe(card.usdMonthlyBase);
  });

  it("Pro storage 1 GB above quota is reported as exceeded (hard cap, no overage charge)", () => {
    const card = TIER_RATE_CARD.pro;
    const usage: UsageInputs = {
      casGbStored: card.includedCasGb + 1,
      requestsPerMonth: 0,
    };
    const r = estimateTier("pro", usage);
    expect(r.fitsWithoutOverage).toBe(false);
    // Hard cap ⇒ overage cost is `null` (not $0); the UI renders
    // "Hard cap — upgrade to continue" rather than a $0 anchor.
    expect(r.breakdown.casOverage).toBeNull();
    // Total is still just the flat base — there is no silent billing.
    expect(r.monthlyTotal).toBe(card.usdMonthlyBase);
  });

  it("Free 1 GB above the 10 GB quota does not bill but does not fit", () => {
    const usage: UsageInputs = { casGbStored: 11, requestsPerMonth: 0 };
    const r = estimateTier("free", usage);
    expect(r.fitsWithoutOverage).toBe(false);
    expect(r.monthlyTotal).toBe(0);
    expect(r.breakdown.casOverage).toBeNull();
  });
});

describe("estimateTier — Enterprise contact-sales", () => {
  it("Enterprise returns null monthlyTotal regardless of usage (no $0 anchor)", () => {
    const r = estimateTier("enterprise", DEFAULT_USAGE);
    expect(r.monthlyTotal).toBeNull();
    expect(r.annualizedMonthlyTotal).toBeNull();
    expect(r.breakdown.base).toBeNull();
  });

  it("Enterprise always reports `fitsWithoutOverage` (contact sales never rejects)", () => {
    const huge: UsageInputs = {
      casGbStored: 10_000_000,
      requestsPerMonth: 10_000_000_000,
    };
    expect(estimateTier("enterprise", huge).fitsWithoutOverage).toBe(true);
  });
});

describe("applyPeriodDiscount + annual billing", () => {
  it("annual discount equals '2 months free' = 1 - 10/12 ≈ 16.67%", () => {
    expect(ANNUAL_DISCOUNT_RATE).toBeCloseTo(1 / 6, 10);
  });

  it("monthly applies no discount; annual scales by (1 - discount)", () => {
    expect(applyPeriodDiscount(100, "monthly")).toBe(100);
    expect(applyPeriodDiscount(100, "annual")).toBeCloseTo(
      100 * (1 - ANNUAL_DISCOUNT_RATE),
      10,
    );
  });

  it("Pro annual base × 12 equals the published $250/year list (within $1)", () => {
    const monthlyBase = TIER_RATE_CARD.pro.usdMonthlyBase ?? 0;
    const annualEffectiveMonthly = applyPeriodDiscount(monthlyBase, "annual");
    expect(annualEffectiveMonthly * 12).toBeCloseTo(250, 5);
  });
});

describe("recommendTier", () => {
  it("zero usage recommends Free", () => {
    expect(recommendTier(zeroUsage)).toBe("free");
  });

  it("usage slightly above Free recommends Pro", () => {
    const usage: UsageInputs = { casGbStored: 50, requestsPerMonth: 600_000 };
    expect(recommendTier(usage)).toBe("pro");
  });

  it("usage above Pro quota returns null (-> route to Enterprise)", () => {
    const usage: UsageInputs = {
      casGbStored: 1_000_000,
      requestsPerMonth: 1_000_000_000,
    };
    expect(recommendTier(usage)).toBeNull();
  });
});

describe("computeBreakEven", () => {
  it("returns headroom under the recommended tier", () => {
    const usage: UsageInputs = { casGbStored: 50, requestsPerMonth: 600_000 };
    const be = computeBreakEven(usage);
    expect(be).not.toBeNull();
    expect(be?.recommended).toBe("pro");
    expect(be?.headroomCasGb).toBe(TIER_RATE_CARD.pro.includedCasGb - 50);
    expect(be?.headroomRequests).toBe(
      TIER_RATE_CARD.pro.includedRequests - 600_000,
    );
  });

  it("returns null when no quantifiable tier fits", () => {
    const usage: UsageInputs = {
      casGbStored: 1_000_000,
      requestsPerMonth: 1_000_000_000,
    };
    expect(computeBreakEven(usage)).toBeNull();
  });
});

describe("defensive clamping", () => {
  it("negative inputs are treated as zero", () => {
    const usage: UsageInputs = {
      casGbStored: -10,
      requestsPerMonth: -1,
    };
    const r = estimateTier("free", usage);
    expect(r.fitsWithoutOverage).toBe(true);
    expect(r.monthlyTotal).toBe(0);
  });

  it("NaN / Infinity inputs are clamped to zero", () => {
    const usage: UsageInputs = {
      casGbStored: Number.NaN,
      requestsPerMonth: Number.POSITIVE_INFINITY,
    };
    const r = estimateTier("free", usage);
    expect(r.fitsWithoutOverage).toBe(true);
  });
});

describe("formatUsd — honest framing", () => {
  it("null becomes 'Contact us' (no misleading $0 for Enterprise)", () => {
    expect(formatUsd(null)).toBe("Contact us");
  });

  it("integer-thousands use locale grouping", () => {
    expect(formatUsd(1234)).toBe("$1,234");
  });

  it("micro-amounts keep two decimals", () => {
    expect(formatUsd(0.5)).toBe("$0.50");
  });

  it("zero formats as $0 (Free tier honest disclosure)", () => {
    expect(formatUsd(0)).toBe("$0");
  });
});

describe("estimateAllTiers", () => {
  it("returns one estimate per canonical tier in order Free / Pro / Enterprise", () => {
    const results = estimateAllTiers(DEFAULT_USAGE);
    expect(results.map((r) => r.tier)).toEqual([...CANONICAL_TIERS]);
  });
});
