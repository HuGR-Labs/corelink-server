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
  formatRetention,
  formatUsd,
  recommendTier,
} from "./pricing";
import type { TierShape, UsageInputs } from "./pricing";

const zeroUsage: UsageInputs = {
  casGbStored: 0,
  requestsPerMonth: 0,
};

describe("CANONICAL_TIERS — launch shape", () => {
  it("is exactly the FROZEN 6-tier launch ladder", () => {
    expect([...CANONICAL_TIERS]).toEqual([
      "free",
      "solo",
      "starter",
      "pro",
      "max",
      "enterprise",
    ]);
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

  it("Pro is $50/mo or $500/yr per pricing-benchmarks §5", () => {
    expect(TIER_RATE_CARD.pro.usdMonthlyBase).toBe(50);
    expect(TIER_RATE_CARD.pro.usdAnnualList).toBe(500);
  });

  it("paid ladder is Solo $15 / Starter $35 / Pro $50 / Max $149", () => {
    expect(TIER_RATE_CARD.solo.usdMonthlyBase).toBe(15);
    expect(TIER_RATE_CARD.starter.usdMonthlyBase).toBe(35);
    expect(TIER_RATE_CARD.pro.usdMonthlyBase).toBe(50);
    expect(TIER_RATE_CARD.max.usdMonthlyBase).toBe(149);
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

  it("Pro annual base × 12 equals the published $500/year list (within $1)", () => {
    const monthlyBase = TIER_RATE_CARD.pro.usdMonthlyBase ?? 0;
    const annualEffectiveMonthly = applyPeriodDiscount(monthlyBase, "annual");
    expect(annualEffectiveMonthly * 12).toBeCloseTo(500, 5);
  });
});

describe("recommendTier", () => {
  it("zero usage recommends Free", () => {
    expect(recommendTier(zeroUsage)).toBe("free");
  });

  it("usage slightly above Free recommends Solo (the next tier up)", () => {
    const usage: UsageInputs = { casGbStored: 50, requestsPerMonth: 600_000 };
    expect(recommendTier(usage)).toBe("solo");
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
    expect(be?.recommended).toBe("solo");
    expect(be?.headroomCasGb).toBe(TIER_RATE_CARD.solo.includedCasGb - 50);
    expect(be?.headroomRequests).toBe(
      TIER_RATE_CARD.solo.includedRequests - 600_000,
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

describe("retention promise — PROPOSAL-2026-06-10 §3.5 (ratified Q5c)", () => {
  it("pins the per-tier retention ladder (7/30/90/365/365/365 days)", () => {
    // Mirrors crates/corelink-eviction/src/tier.rs ttl_for_tier composed
    // with crates/corelink-ratelimit/src/tier.rs tier_for_billing_label:
    // free→Free 7d, solo→Solo 30d, starter→Team 90d, pro→Business 365d,
    // max→Business 365d, enterprise→Enterprise 365d.
    expect(TIER_RATE_CARD.free.retentionDays).toBe(7);
    expect(TIER_RATE_CARD.solo.retentionDays).toBe(30);
    expect(TIER_RATE_CARD.starter.retentionDays).toBe(90);
    expect(TIER_RATE_CARD.pro.retentionDays).toBe(365);
    expect(TIER_RATE_CARD.max.retentionDays).toBe(365);
    expect(TIER_RATE_CARD.enterprise.retentionDays).toBe(365);
  });

  it("Enterprise is the ONLY tier with the 730-day contract override cap", () => {
    expect(TIER_RATE_CARD.enterprise.retentionOverrideCapDays).toBe(730);
    for (const tier of CANONICAL_TIERS) {
      if (tier !== "enterprise") {
        expect(TIER_RATE_CARD[tier].retentionOverrideCapDays).toBeNull();
      }
    }
  });

  it("retention is monotonically non-decreasing across the price ladder", () => {
    // A higher tier must never promise SHORTER retention than a
    // cheaper one (same monotonicity rule as the rate-class mapping).
    let last = 0;
    for (const tier of CANONICAL_TIERS) {
      const days = TIER_RATE_CARD[tier].retentionDays;
      expect(days).toBeGreaterThanOrEqual(last);
      last = days;
    }
  });

  it("formatRetention renders the flat promise for self-serve tiers", () => {
    expect(formatRetention(TIER_RATE_CARD.starter)).toBe(
      "90-day cache retention",
    );
    expect(formatRetention(TIER_RATE_CARD.max)).toBe(
      "365-day cache retention",
    );
  });

  it("formatRetention appends the contract cap for Enterprise", () => {
    expect(formatRetention(TIER_RATE_CARD.enterprise)).toBe(
      "365-day cache retention — up to 730 days by contract",
    );
  });
});

describe("estimateAllTiers", () => {
  it("returns one estimate per canonical tier in ladder order (Free → Enterprise)", () => {
    const results = estimateAllTiers(DEFAULT_USAGE);
    expect(results.map((r) => r.tier)).toEqual([...CANONICAL_TIERS]);
  });
});
