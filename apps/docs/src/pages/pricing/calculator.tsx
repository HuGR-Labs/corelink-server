/**
 * Interactive cost calculator — wave-29 stream-7 r-prep deliverable.
 *
 * Inputs (monthly): CAS GB stored, CAS GB transferred, audit events,
 * regions, BYOK providers, admin seats. Outputs: per-tier monthly cost
 * (Free / Starter / Team / Pro / Enterprise) with break-even analysis.
 * Pure client-side; pricing formulas live in `../../lib/pricing.ts`.
 *
 * Honest framing: every dollar rendered is provisional pre-GA. Enterprise
 * surfaces "Contact us" rather than a misleading $0.
 */

import Layout from "@theme/Layout";
import { useMemo, useState } from "react";
import type { ReactElement } from "react";

import {
  DEFAULT_USAGE,
  TIER_RATE_CARD,
  computeBreakEven,
  estimateAllTiers,
  formatUsd,
  recommendTier,
} from "../../lib/pricing";
import type { BillingPeriod, UsageInputs } from "../../lib/pricing";

import styles from "../pricing.module.css";

interface NumberFieldSpec {
  readonly key: keyof UsageInputs;
  readonly label: string;
  readonly hint: string;
  readonly step: number;
  readonly min: number;
}

const FIELDS: readonly NumberFieldSpec[] = [
  {
    key: "casGbStored",
    label: "CAS stored (GB / month)",
    hint: "Average CAS storage footprint over the month.",
    step: 10,
    min: 0,
  },
  {
    key: "casGbTransferred",
    label: "CAS transferred (GB / month)",
    hint: "Cross-region egress and cold-tier pulls; intra-region is not billed.",
    step: 100,
    min: 0,
  },
  {
    key: "auditEventsPerMonth",
    label: "Audit events / month",
    hint: "Auth events, admin actions, policy decisions logged.",
    step: 1000,
    min: 0,
  },
  {
    key: "regions",
    label: "Regions",
    hint: "Distinct CoreLink regions you read or write from.",
    step: 1,
    min: 0,
  },
  {
    key: "byokProviders",
    label: "BYOK providers",
    hint: "AWS KMS / GCP KMS / Azure Key Vault / HashiCorp Vault count.",
    step: 1,
    min: 0,
  },
  {
    key: "adminSeats",
    label: "Admin seats",
    hint: "Users with admin / write permissions in the console.",
    step: 1,
    min: 0,
  },
];

function parseNonNegativeInt(raw: string): number {
  const n = Number(raw);
  if (!Number.isFinite(n) || n < 0) {
    return 0;
  }
  return Math.floor(n);
}

export default function Calculator(): ReactElement {
  const [usage, setUsage] = useState<UsageInputs>(DEFAULT_USAGE);
  const [period, setPeriod] = useState<BillingPeriod>("monthly");

  const estimates = useMemo(() => estimateAllTiers(usage, period), [usage, period]);
  const recommended = useMemo(() => recommendTier(usage), [usage]);
  const breakEven = useMemo(() => computeBreakEven(usage), [usage]);

  const setField = (key: keyof UsageInputs, value: number): void => {
    setUsage((prev) => ({ ...prev, [key]: value }));
  };

  return (
    <Layout
      title="Pricing calculator"
      description="Interactive cost calculator for the 5 CoreLink tiers. Pricing is provisional pre-GA."
    >
      <main className={styles.page}>
        <header className={styles.header}>
          <h1>Pricing calculator</h1>
          <p>
            Plug in your expected monthly usage. We compute a side-by-side
            estimate across every tier so you can see where you land — and the
            smallest tier whose included quotas cover you.
          </p>
        </header>

        <div className={styles.honestNote} role="status">
          <strong>Pre-GA estimates.</strong> Every dollar amount is provisional
          and subject to refinement at GA. Enterprise pricing is contract-based
          and rendered as <em>Contact us</em> — we do not anchor a misleading
          $0. See the audit doc{" "}
          <code>specs/_audits/sealed/2026-05-16-pricing-page.md</code> for the rate
          card status.
        </div>

        <div className={styles.periodToggle} role="group" aria-label="Billing period">
          <button
            type="button"
            aria-pressed={period === "monthly"}
            onClick={() => setPeriod("monthly")}
          >
            Monthly
          </button>
          <button
            type="button"
            aria-pressed={period === "annual"}
            onClick={() => setPeriod("annual")}
          >
            Annual (save 15% on base)
          </button>
        </div>

        <form
          className={styles.calcForm}
          aria-label="Usage inputs"
          onSubmit={(event) => event.preventDefault()}
        >
          {FIELDS.map((field) => (
            <div key={field.key} className={styles.calcField}>
              <label htmlFor={`calc-${field.key}`}>{field.label}</label>
              <input
                id={`calc-${field.key}`}
                type="number"
                inputMode="numeric"
                min={field.min}
                step={field.step}
                value={usage[field.key]}
                onChange={(event) =>
                  setField(field.key, parseNonNegativeInt(event.target.value))
                }
              />
              <small>{field.hint}</small>
            </div>
          ))}
        </form>

        <h2>Per-tier monthly estimate</h2>

        <section className={styles.resultsGrid} aria-label="Per-tier cost estimates">
          {estimates.map((estimate) => {
            const isRecommended = recommended === estimate.tier;
            const card = TIER_RATE_CARD[estimate.tier];
            return (
              <article
                key={estimate.tier}
                className={
                  isRecommended
                    ? `${styles.resultCard} ${styles.recommended}`
                    : styles.resultCard
                }
              >
                {isRecommended ? (
                  <span className={styles.recommendedBadge}>Recommended</span>
                ) : null}
                <h3>{estimate.label}</h3>
                <div className={styles.tierPrice}>
                  {formatUsd(estimate.monthlyTotal)}
                </div>
                <div className={styles.tierSubprice}>
                  {estimate.monthlyTotal === null
                    ? "Custom contract"
                    : period === "annual"
                      ? "/ month, billed annually"
                      : "/ month"}
                </div>
                {estimate.monthlyTotal !== null ? (
                  <ul className={styles.tierFeatures}>
                    <li>
                      Base: {formatUsd(estimate.breakdown.base)}
                    </li>
                    <li>
                      CAS overage:{" "}
                      {formatUsd(estimate.breakdown.casOverage)}
                    </li>
                    <li>
                      Transfer overage:{" "}
                      {formatUsd(estimate.breakdown.transferOverage)}
                    </li>
                    <li>
                      Audit overage:{" "}
                      {formatUsd(estimate.breakdown.auditOverage)}
                    </li>
                    <li>
                      Region overage:{" "}
                      {formatUsd(estimate.breakdown.regionOverage)}
                    </li>
                    <li>
                      BYOK overage:{" "}
                      {formatUsd(estimate.breakdown.byokOverage)}
                    </li>
                    <li>
                      Seat overage:{" "}
                      {formatUsd(estimate.breakdown.seatOverage)}
                    </li>
                  </ul>
                ) : (
                  <p>
                    Enterprise pricing is negotiated against your security,
                    compliance, and capacity requirements. Talk to us.
                  </p>
                )}
                <small>
                  Included: {card.includedCasGb} GB CAS,{" "}
                  {card.includedTransferGb} GB transfer,{" "}
                  {card.includedRegions} region
                  {card.includedRegions === 1 ? "" : "s"}.
                </small>
              </article>
            );
          })}
        </section>

        <h2>Break-even analysis</h2>
        {breakEven !== null ? (
          <div className={styles.breakEven}>
            <p>
              Your usage profile fits inside <strong>{TIER_RATE_CARD[breakEven.recommended].label}</strong>{" "}
              without overage. Headroom before you would move to the next tier:
            </p>
            <ul>
              <li>{breakEven.headroomCasGb.toLocaleString("en-US")} GB CAS storage</li>
              <li>
                {breakEven.headroomTransferGb.toLocaleString("en-US")} GB transfer / mo
              </li>
              <li>
                {breakEven.headroomAuditEvents.toLocaleString("en-US")} audit events / mo
              </li>
              <li>{breakEven.headroomRegions} additional region(s)</li>
              <li>{breakEven.headroomSeats} admin seat(s)</li>
            </ul>
          </div>
        ) : (
          <div className={styles.breakEven}>
            <p>
              Your usage exceeds the quantifiable tiers&apos; included quotas. Talk
              to us about <strong>Enterprise</strong> — pricing is contract-based and
              tuned to your capacity, compliance, and SLA needs.
            </p>
          </div>
        )}

        <a className={styles.cta} href="/pilot/apply">
          Apply for pilot
        </a>
      </main>
    </Layout>
  );
}
