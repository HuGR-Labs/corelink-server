/**
 * PricingCalculator — interactive cache-hit-ratio + cost estimator stub.
 *
 * Phase 1 (this WI): a deterministic placeholder that emits `$X` for every
 * numeric output so that no specific dollar amounts ship before Finance
 * sign-off (S-18 spec contract §10 anti-scope).
 *
 * Phase 2 (post-sign-off): wire to the canonical rate card and surface live
 * estimates. The component contract (props, inputs surfaced) is intentionally
 * frozen here so the swap is a single-file change.
 */

import type { ReactElement } from "react";

import type { TierId } from "../lib/pricing";

export interface PricingCalculatorInputs {
  readonly storageGb: number;
  readonly monthlyCacheHits: number;
  readonly cacheHitRatio: number;
  readonly egressGb: number;
  /** Canonical 6-tier id from the signed rate card (lib/pricing TierId). */
  readonly plan: TierId;
}

export const DEFAULT_PRICING_INPUTS: PricingCalculatorInputs = {
  storageGb: 100,
  monthlyCacheHits: 1_000_000,
  cacheHitRatio: 0.85,
  egressGb: 50,
  plan: "pro",
};

export function PricingCalculator(): ReactElement {
  return (
    <section
      data-testid="pricing-calculator-stub"
      role="region"
      aria-label="Pricing calculator stub — placeholder values pending Finance sign-off"
      style={{
        border: "1px dashed #6b7280",
        padding: "1rem",
        borderRadius: "0.5rem",
        background: "#f9fafb",
      }}
    >
      <p>
        <strong>Calculator stub.</strong> Interactive estimator pending Finance
        sign-off on the rate card. All monetary outputs are <code>$X</code>{" "}
        placeholders until then.
      </p>
      <dl>
        <dt>Estimated storage cost</dt>
        <dd>$X / month</dd>
        <dt>Estimated egress cost</dt>
        <dd>$X / month</dd>
        <dt>Estimated total</dt>
        <dd>$X / month</dd>
      </dl>
    </section>
  );
}
