/**
 * Unit tests for the pricing library. Covers tier boundaries,
 * overage formulas, break-even recommendation, Enterprise contact-sales
 * handling, period discount, defensive clamping, and format honesty.
 *
 * Wave-29 stream-7 / r-prep pricing page deliverable.
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
import type { UsageInputs } from "./pricing";

const zeroUsage: UsageInputs = {
  casGbStored: 0,
  casGbTransferred: 0,
  auditEventsPerMonth: 0,
  regions: 0,
  byokProviders: 0,
  adminSeats: 0,
};

describe("CANONICAL_TIERS", () => {
  it("matches the wave-13 5-tier shape", () => {
    expect([...CANONICAL_TIERS]).toEqual([
      "free",
      "starter",
      "team",
      "pro",
      "enterprise",
    ]);
  });

  it("every tier card is flagged provisional (honest framing)", () => {
    for (const tier of CANONICAL_TIERS) {
      expect(TIER_RATE_CARD[tier].provisional).toBe(true);
    }
  });
});

describe("estimateTier — boundaries", () => {
  it("Free with zero usage costs $0 and fits without overage", () => {
    const r = estimateTier("free", zeroUsage);
    expect(r.monthlyTotal).toBe(0);
    expect(r.fitsWithoutOverage).toBe(true);
    expect(r.fits).toBe(true);
  });

  it("Starter at exactly its included quotas has zero overage", () => {
    const card = TIER_RATE_CARD.starter;
    const usage: UsageInputs = {
      casGbStored: card.includedCasGb,
      casGbTransferred: card.includedTransferGb,
      auditEventsPerMonth: card.includedAuditEvents,
      regions: card.includedRegions,
      byokProviders: card.includedByokProviders,
      adminSeats: card.includedSeats,
    };
    const r = estimateTier("starter", usage);
    expect(r.breakdown.casOverage).toBe(0);
    expect(r.breakdown.transferOverage).toBe(0);
    expect(r.breakdown.auditOverage).toBe(0);
    expect(r.monthlyTotal).toBe(card.usdMonthlyBase);
    expect(r.fitsWithoutOverage).toBe(true);
  });

  it("Team overage: 1 GB above CAS quota costs exactly one unit", () => {
    const card = TIER_RATE_CARD.team;
    const usage: UsageInputs = {
      ...DEFAULT_USAGE,
      casGbStored: card.includedCasGb + 1,
      casGbTransferred: 0,
      auditEventsPerMonth: 0,
      regions: 0,
      byokProviders: 0,
      adminSeats: 0,
    };
    const r = estimateTier("team", usage);
    expect(r.breakdown.casOverage).toBeCloseTo(card.usdPerCasGbOverage, 10);
    expect(r.monthlyTotal).toBeCloseTo(
      (card.usdMonthlyBase ?? 0) + card.usdPerCasGbOverage,
      10,
    );
  });

  it("Pro overage: BYOK +1 provider beyond included is billed", () => {
    const card = TIER_RATE_CARD.pro;
    const usage: UsageInputs = {
      ...zeroUsage,
      byokProviders: card.includedByokProviders + 1,
    };
    const r = estimateTier("pro", usage);
    expect(r.breakdown.byokOverage).toBeCloseTo(card.usdPerByokProviderOverage, 10);
  });
});

describe("estimateTier — Enterprise contact-sales", () => {
  it("Enterprise returns null monthlyTotal regardless of usage (no $0 anchor)", () => {
    const r = estimateTier("enterprise", DEFAULT_USAGE);
    expect(r.monthlyTotal).toBeNull();
    expect(r.annualizedMonthlyTotal).toBeNull();
    expect(r.breakdown.base).toBeNull();
  });

  it("Enterprise fits any usage profile (contact sales never rejects)", () => {
    const huge: UsageInputs = {
      casGbStored: 10_000_000,
      casGbTransferred: 10_000_000,
      auditEventsPerMonth: 10_000_000_000,
      regions: 50,
      byokProviders: 50,
      adminSeats: 50_000,
    };
    expect(estimateTier("enterprise", huge).fits).toBe(true);
  });
});

describe("applyPeriodDiscount + annual billing", () => {
  it("annual applies ANNUAL_DISCOUNT_RATE to base only", () => {
    expect(applyPeriodDiscount(100, "monthly")).toBe(100);
    expect(applyPeriodDiscount(100, "annual")).toBeCloseTo(
      100 * (1 - ANNUAL_DISCOUNT_RATE),
      10,
    );
  });

  it("Team annual quote discounts base but not overage", () => {
    const card = TIER_RATE_CARD.team;
    const usage: UsageInputs = {
      ...zeroUsage,
      casGbStored: card.includedCasGb + 100, // 100 GB overage
    };
    const monthly = estimateTier("team", usage, "monthly");
    const annual = estimateTier("team", usage, "annual");
    // Overage identical, base discounted.
    expect(annual.breakdown.casOverage).toBe(monthly.breakdown.casOverage);
    expect(annual.breakdown.base).toBeCloseTo(
      (card.usdMonthlyBase ?? 0) * (1 - ANNUAL_DISCOUNT_RATE),
      10,
    );
  });
});

describe("recommendTier", () => {
  it("zero usage recommends Free", () => {
    expect(recommendTier(zeroUsage)).toBe("free");
  });

  it("usage slightly above Free recommends Starter", () => {
    const usage: UsageInputs = { ...zeroUsage, casGbStored: 50 };
    expect(recommendTier(usage)).toBe("starter");
  });

  it("very heavy usage above Pro returns null (-> contact Enterprise)", () => {
    const usage: UsageInputs = {
      casGbStored: 100_000,
      casGbTransferred: 1_000_000,
      auditEventsPerMonth: 1_000_000_000,
      regions: 20,
      byokProviders: 10,
      adminSeats: 1_000,
    };
    expect(recommendTier(usage)).toBeNull();
  });
});

describe("computeBreakEven", () => {
  it("returns headroom under the recommended tier", () => {
    const usage: UsageInputs = { ...zeroUsage, casGbStored: 50 };
    const be = computeBreakEven(usage);
    expect(be).not.toBeNull();
    expect(be?.recommended).toBe("starter");
    expect(be?.headroomCasGb).toBe(TIER_RATE_CARD.starter.includedCasGb - 50);
  });

  it("returns null when no tier fits without overage", () => {
    const usage: UsageInputs = {
      casGbStored: 100_000,
      casGbTransferred: 1_000_000,
      auditEventsPerMonth: 1_000_000_000,
      regions: 20,
      byokProviders: 10,
      adminSeats: 1_000,
    };
    expect(computeBreakEven(usage)).toBeNull();
  });
});

describe("defensive clamping", () => {
  it("negative inputs are treated as zero", () => {
    const usage: UsageInputs = {
      casGbStored: -10,
      casGbTransferred: -5,
      auditEventsPerMonth: -1,
      regions: -2,
      byokProviders: -1,
      adminSeats: -3,
    };
    const r = estimateTier("starter", usage);
    expect(r.breakdown.casOverage).toBe(0);
    expect(r.breakdown.transferOverage).toBe(0);
    expect(r.breakdown.seatOverage).toBe(0);
  });

  it("NaN / Infinity inputs are clamped to zero", () => {
    const usage: UsageInputs = {
      casGbStored: Number.NaN,
      casGbTransferred: Number.POSITIVE_INFINITY,
      auditEventsPerMonth: Number.NEGATIVE_INFINITY,
      regions: Number.NaN,
      byokProviders: Number.NaN,
      adminSeats: Number.NaN,
    };
    const r = estimateTier("free", usage);
    expect(r.fitsWithoutOverage).toBe(true);
  });
});

describe("formatUsd — honest framing", () => {
  it("null becomes 'Contact us' (no misleading $0)", () => {
    expect(formatUsd(null)).toBe("Contact us");
  });

  it("integer-thousands use locale grouping", () => {
    expect(formatUsd(1234)).toBe("$1,234");
  });

  it("micro-amounts keep two decimals", () => {
    expect(formatUsd(0.5)).toBe("$0.50");
  });

  it("zero formats as $0 (free tier honest disclosure)", () => {
    expect(formatUsd(0)).toBe("$0");
  });
});

describe("estimateAllTiers", () => {
  it("returns one estimate per canonical tier, in order", () => {
    const results = estimateAllTiers(DEFAULT_USAGE);
    expect(results.map((r) => r.tier)).toEqual([...CANONICAL_TIERS]);
  });
});
