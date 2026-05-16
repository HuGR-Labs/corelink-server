/**
 * Pricing formulas for the CoreLink calculator + pricing page.
 *
 * Wave-29 stream-7 / r-prep pricing page deliverable.
 *
 * The tier taxonomy here is the canonical 5-tier shape from wave-13
 * (`crates/corelink-tier-selection/src/tier.rs`):
 *
 *     Free, Starter, Team, Pro, Enterprise
 *
 * The numeric rates are honest placeholders flagged with
 * `provisional: true`. Finance / Legal / Product sign-off in
 * `apps/docs/CONTENT-REVIEW.md` will replace them at GA. The shapes
 * (per-tier base, per-GB CAS, per-GB transfer, per-event audit,
 * per-region surcharge, BYOK surcharge) are STABLE; only the magnitudes
 * float — so the calculator UI does not need to be reworked when
 * Finance lands the rate card.
 *
 * NO BACKEND CALLS — all formulas are pure functions so the calculator
 * is fully client-side.
 */

/**
 * Canonical 5 tiers per wave-13 tier-selection contract.
 *
 * The user-facing task spec asked for a 4-tier table
 * (Solo / Team / Business / Enterprise). The charter overrides:
 * "use canonical tier shapes from wave-13". `Solo` ≈ `Starter`,
 * `Business` ≈ `Pro`. The audit doc records the deviation.
 */
export type TierId = "free" | "starter" | "team" | "pro" | "enterprise";

export const CANONICAL_TIERS: readonly TierId[] = [
  "free",
  "starter",
  "team",
  "pro",
  "enterprise",
] as const;

/**
 * Billing period. Annual yields a flat discount applied to the
 * monthly base; usage-based components (CAS storage, transfer, audit,
 * regions, BYOK) are not discounted, since they reflect underlying
 * cloud cost.
 */
export type BillingPeriod = "monthly" | "annual";

/**
 * Annual discount on the per-tier base fee. Honest framing: this is
 * a placeholder, NOT a published commitment.
 */
export const ANNUAL_DISCOUNT_RATE = 0.15;

/**
 * Single tier's published shape. All `usd_` fields are placeholders
 * (`provisional: true`) until Finance sign-off per CONTENT-REVIEW.md.
 */
export interface TierShape {
  readonly id: TierId;
  readonly label: string;
  /** Included CAS storage (GB-month) — overage billed per `usdPerCasGbOverage`. */
  readonly includedCasGb: number;
  /** Included egress / transfer (GB / month) — overage at `usdPerTransferGbOverage`. */
  readonly includedTransferGb: number;
  /** Included audit events / month — overage at `usdPerAuditEventOverage`. */
  readonly includedAuditEvents: number;
  /** Included regions — additional regions charged per `usdPerRegionOverage`. */
  readonly includedRegions: number;
  /** Included BYOK providers (0 if BYOK not available on this tier). */
  readonly includedByokProviders: number;
  /** Included admin seats — overage at `usdPerSeatOverage`. */
  readonly includedSeats: number;
  /** Per-tier base monthly fee (USD). `null` for tiers where pricing
   * is contact-based (Enterprise) or zero (Free). */
  readonly usdMonthlyBase: number | null;
  readonly usdPerCasGbOverage: number;
  readonly usdPerTransferGbOverage: number;
  readonly usdPerAuditEventOverage: number;
  readonly usdPerRegionOverage: number;
  readonly usdPerByokProviderOverage: number;
  readonly usdPerSeatOverage: number;
  readonly supportResponseHours: number | "custom";
  /** Honest disclosure: every numeric rate is provisional until
   * Finance lands the rate card at GA. */
  readonly provisional: true;
}

/**
 * Canonical rate card placeholders. Each magnitude is flagged
 * `provisional: true` and listed in the audit doc as
 * `<TBD per GA pricing review>` — they are NOT a commitment.
 */
export const TIER_RATE_CARD: Readonly<Record<TierId, TierShape>> = {
  free: {
    id: "free",
    label: "Free",
    includedCasGb: 10,
    includedTransferGb: 100,
    includedAuditEvents: 10_000,
    includedRegions: 1,
    includedByokProviders: 0,
    includedSeats: 1,
    usdMonthlyBase: 0,
    usdPerCasGbOverage: 0, // overage not allowed on Free; hard cap.
    usdPerTransferGbOverage: 0,
    usdPerAuditEventOverage: 0,
    usdPerRegionOverage: 0,
    usdPerByokProviderOverage: 0,
    usdPerSeatOverage: 0,
    supportResponseHours: 72,
    provisional: true,
  },
  starter: {
    id: "starter",
    label: "Starter",
    includedCasGb: 100,
    includedTransferGb: 1_000,
    includedAuditEvents: 250_000,
    includedRegions: 1,
    includedByokProviders: 0,
    includedSeats: 5,
    usdMonthlyBase: 29,
    usdPerCasGbOverage: 0.12,
    usdPerTransferGbOverage: 0.05,
    usdPerAuditEventOverage: 0.0001,
    usdPerRegionOverage: 25,
    usdPerByokProviderOverage: 0,
    usdPerSeatOverage: 4,
    supportResponseHours: 48,
    provisional: true,
  },
  team: {
    id: "team",
    label: "Team",
    includedCasGb: 500,
    includedTransferGb: 5_000,
    includedAuditEvents: 1_000_000,
    includedRegions: 2,
    includedByokProviders: 0,
    includedSeats: 15,
    usdMonthlyBase: 149,
    usdPerCasGbOverage: 0.1,
    usdPerTransferGbOverage: 0.04,
    usdPerAuditEventOverage: 0.00008,
    usdPerRegionOverage: 35,
    usdPerByokProviderOverage: 0,
    usdPerSeatOverage: 6,
    supportResponseHours: 24,
    provisional: true,
  },
  pro: {
    id: "pro",
    label: "Pro",
    includedCasGb: 2_000,
    includedTransferGb: 20_000,
    includedAuditEvents: 5_000_000,
    includedRegions: 3,
    includedByokProviders: 1,
    includedSeats: 50,
    usdMonthlyBase: 549,
    usdPerCasGbOverage: 0.08,
    usdPerTransferGbOverage: 0.035,
    usdPerAuditEventOverage: 0.00006,
    usdPerRegionOverage: 50,
    usdPerByokProviderOverage: 75,
    usdPerSeatOverage: 8,
    supportResponseHours: 8,
    provisional: true,
  },
  enterprise: {
    id: "enterprise",
    label: "Enterprise",
    includedCasGb: 10_000,
    includedTransferGb: 100_000,
    includedAuditEvents: 50_000_000,
    includedRegions: 5,
    includedByokProviders: 4,
    includedSeats: 250,
    // `null` ⇒ contact sales; calculator surfaces "Contact us" rather
    // than a misleading anchor like "starting at $0".
    usdMonthlyBase: null,
    usdPerCasGbOverage: 0.06,
    usdPerTransferGbOverage: 0.025,
    usdPerAuditEventOverage: 0.00004,
    usdPerRegionOverage: 0, // negotiated.
    usdPerByokProviderOverage: 0, // negotiated.
    usdPerSeatOverage: 0, // negotiated.
    supportResponseHours: "custom",
    provisional: true,
  },
};

/**
 * Buyer inputs for the calculator. All values are non-negative; the
 * pure formula functions clamp at zero defensively.
 */
export interface UsageInputs {
  readonly casGbStored: number;
  readonly casGbTransferred: number;
  readonly auditEventsPerMonth: number;
  readonly regions: number;
  readonly byokProviders: number;
  readonly adminSeats: number;
}

export const DEFAULT_USAGE: UsageInputs = {
  casGbStored: 250,
  casGbTransferred: 2_000,
  auditEventsPerMonth: 500_000,
  regions: 2,
  byokProviders: 0,
  adminSeats: 10,
};

/**
 * Per-tier cost estimate. `monthlyTotal` is `null` for Enterprise
 * (contact sales) to avoid misleading $0 anchoring.
 */
export interface TierCostEstimate {
  readonly tier: TierId;
  readonly label: string;
  readonly fits: boolean;
  /** USD/month for monthly billing, or null when contact-sales. */
  readonly monthlyTotal: number | null;
  /** USD/month effective price under annual billing (base discounted). */
  readonly annualizedMonthlyTotal: number | null;
  readonly breakdown: {
    readonly base: number | null;
    readonly casOverage: number;
    readonly transferOverage: number;
    readonly auditOverage: number;
    readonly regionOverage: number;
    readonly byokOverage: number;
    readonly seatOverage: number;
  };
  /** True only when every usage input fits within the tier's
   * included quotas (no overage). Used by the calculator to surface
   * the smallest fitting tier as the recommendation. */
  readonly fitsWithoutOverage: boolean;
  readonly provisional: true;
}

const clampNonNegative = (n: number): number => (n < 0 || !Number.isFinite(n) ? 0 : n);

/**
 * Apply the period discount to the monthly base only. Usage-based
 * overage is pass-through (it reflects cloud cost; honest framing).
 */
export function applyPeriodDiscount(
  monthlyBase: number,
  period: BillingPeriod,
): number {
  if (period === "annual") {
    return monthlyBase * (1 - ANNUAL_DISCOUNT_RATE);
  }
  return monthlyBase;
}

/**
 * Pure formula: per-tier monthly cost given a usage profile and
 * billing period. Enterprise returns `monthlyTotal: null` so the UI
 * shows "Contact us" rather than $0.
 */
export function estimateTier(
  tier: TierId,
  usage: UsageInputs,
  period: BillingPeriod = "monthly",
): TierCostEstimate {
  const card = TIER_RATE_CARD[tier];
  const gbStored = clampNonNegative(usage.casGbStored);
  const gbTransfer = clampNonNegative(usage.casGbTransferred);
  const auditEvents = clampNonNegative(usage.auditEventsPerMonth);
  const regions = clampNonNegative(usage.regions);
  const byok = clampNonNegative(usage.byokProviders);
  const seats = clampNonNegative(usage.adminSeats);

  // Quota exceedance is computed on raw GB / event / count axes,
  // *independent* of the overage USD rate — a tier with a $0 overage
  // rate (e.g. Free's hard cap, or Enterprise's negotiated rate) is
  // still considered to have "overage usage" if the buyer exceeds the
  // included quota. This keeps `recommendTier` honest.
  const casUnits = Math.max(0, gbStored - card.includedCasGb);
  const xferUnits = Math.max(0, gbTransfer - card.includedTransferGb);
  const auditUnits = Math.max(0, auditEvents - card.includedAuditEvents);
  const regionUnits = Math.max(0, regions - card.includedRegions);
  const byokUnits = Math.max(0, byok - card.includedByokProviders);
  const seatUnits = Math.max(0, seats - card.includedSeats);

  const casOver = casUnits * card.usdPerCasGbOverage;
  const xferOver = xferUnits * card.usdPerTransferGbOverage;
  const auditOver = auditUnits * card.usdPerAuditEventOverage;
  const regionOver = regionUnits * card.usdPerRegionOverage;
  const byokOver = byokUnits * card.usdPerByokProviderOverage;
  const seatOver = seatUnits * card.usdPerSeatOverage;

  const overageSum = casOver + xferOver + auditOver + regionOver + byokOver + seatOver;

  const fitsWithoutOverage =
    casUnits === 0 &&
    xferUnits === 0 &&
    auditUnits === 0 &&
    regionUnits === 0 &&
    byokUnits === 0 &&
    seatUnits === 0;
  const fitsAtAll =
    tier === "enterprise"
      ? true
      : tier === "free"
        ? fitsWithoutOverage
        : true;

  // Enterprise is contact-sales: do not anchor a number.
  if (card.usdMonthlyBase === null) {
    return {
      tier,
      label: card.label,
      fits: fitsAtAll,
      monthlyTotal: null,
      annualizedMonthlyTotal: null,
      breakdown: {
        base: null,
        casOverage: casOver,
        transferOverage: xferOver,
        auditOverage: auditOver,
        regionOverage: regionOver,
        byokOverage: byokOver,
        seatOverage: seatOver,
      },
      fitsWithoutOverage,
      provisional: true,
    };
  }

  const monthlyTotal = card.usdMonthlyBase + overageSum;
  const annualBase = applyPeriodDiscount(card.usdMonthlyBase, "annual");
  const annualizedMonthlyTotal = annualBase + overageSum;

  return {
    tier,
    label: card.label,
    fits: fitsAtAll,
    monthlyTotal: period === "annual" ? annualizedMonthlyTotal : monthlyTotal,
    annualizedMonthlyTotal,
    breakdown: {
      base: period === "annual" ? annualBase : card.usdMonthlyBase,
      casOverage: casOver,
      transferOverage: xferOver,
      auditOverage: auditOver,
      regionOverage: regionOver,
      byokOverage: byokOver,
      seatOverage: seatOver,
    },
    fitsWithoutOverage,
    provisional: true,
  };
}

/**
 * Estimate every tier in canonical order. Useful for side-by-side
 * comparison rendering in the calculator.
 */
export function estimateAllTiers(
  usage: UsageInputs,
  period: BillingPeriod = "monthly",
): readonly TierCostEstimate[] {
  return CANONICAL_TIERS.map((t) => estimateTier(t, usage, period));
}

/**
 * Pick the smallest tier (by canonical order) whose included quotas
 * fully cover the usage profile. Returns `null` if no quantifiable
 * tier fits (i.e. usage exceeds even Pro's quotas) — caller should
 * route to Enterprise contact-sales in that case.
 */
export function recommendTier(usage: UsageInputs): TierId | null {
  // Free / Starter / Team / Pro are the quantifiable candidates;
  // Enterprise is the contact-sales fallback handled by the caller.
  for (const tier of ["free", "starter", "team", "pro"] as const) {
    const est = estimateTier(tier, usage, "monthly");
    if (est.fitsWithoutOverage) {
      return tier;
    }
  }
  return null;
}

/**
 * Break-even analysis: for the recommended tier, how much headroom
 * does the buyer have on each axis before they hit a higher tier?
 * Returns `null` when the recommended tier is Free (no overage allowed)
 * or when no tier fits without overage.
 */
export interface BreakEvenHeadroom {
  readonly recommended: TierId;
  readonly headroomCasGb: number;
  readonly headroomTransferGb: number;
  readonly headroomAuditEvents: number;
  readonly headroomRegions: number;
  readonly headroomSeats: number;
}

export function computeBreakEven(usage: UsageInputs): BreakEvenHeadroom | null {
  const rec = recommendTier(usage);
  if (rec === null) {
    return null;
  }
  const card = TIER_RATE_CARD[rec];
  return {
    recommended: rec,
    headroomCasGb: Math.max(0, card.includedCasGb - usage.casGbStored),
    headroomTransferGb: Math.max(0, card.includedTransferGb - usage.casGbTransferred),
    headroomAuditEvents: Math.max(
      0,
      card.includedAuditEvents - usage.auditEventsPerMonth,
    ),
    headroomRegions: Math.max(0, card.includedRegions - usage.regions),
    headroomSeats: Math.max(0, card.includedSeats - usage.adminSeats),
  };
}

/**
 * Format a USD amount honestly. `null` ⇒ "Contact us" (no $0 anchor).
 * Negative inputs are clamped to 0.
 */
export function formatUsd(amount: number | null): string {
  if (amount === null) {
    return "Contact us";
  }
  const clamped = clampNonNegative(amount);
  // Two decimals for sub-$10 micro-amounts; whole dollars otherwise.
  if (clamped > 0 && clamped < 10) {
    return `$${clamped.toFixed(2)}`;
  }
  return `$${Math.round(clamped).toLocaleString("en-US")}`;
}
