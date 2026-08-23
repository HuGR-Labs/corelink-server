/**
 * Interactive cost calculator — Phase 0.E 3-tier launch shape.
 *
 * Inputs (monthly): CAS GB stored, cache requests / month. Outputs:
 * per-tier verdict (fits / exceeds / contact sales) across Free / Pro
 * / Enterprise, plus the smallest tier that fits and headroom under
 * that tier. Pure client-side; formulas live in `../../lib/pricing.ts`.
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

const APP_BASE = "https://humangr.com/corelink";
const SIGNUP_URL = `${APP_BASE}/sign-up`;
const UPGRADE_PRO_URL = `${APP_BASE}/en/upgrade?plan=pro`;
const SALES_MAILTO = "mailto:sales@humangr.com?subject=CoreLink%20Enterprise%20inquiry";

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
    key: "requestsPerMonth",
    label: "Cache requests / month",
    hint: "Total cache reads + writes across all workspaces.",
    step: 100_000,
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

  const estimates = useMemo(
    () => estimateAllTiers(usage, period),
    [usage, period],
  );
  const recommended = useMemo(() => recommendTier(usage), [usage]);
  const breakEven = useMemo(() => computeBreakEven(usage), [usage]);

  const setField = (key: keyof UsageInputs, value: number): void => {
    setUsage((prev) => ({ ...prev, [key]: value }));
  };

  return (
    <Layout
      title="Pricing calculator"
      description="Interactive cost calculator for CoreLink's 6 tiers: Free, Solo, Starter, Pro, Max, Enterprise."
    >
      <main className={styles.page}>
        <header className={styles.header}>
          <h1>Pricing calculator</h1>
          <p>
            Plug in your expected monthly storage and cache requests. We show
            the smallest tier that fits without overage.
          </p>
        </header>

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
            Annual (2 months free)
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

        <h2>Per-tier verdict</h2>

        <section className={styles.resultsGrid} aria-label="Per-tier verdicts">
          {estimates.map((estimate) => {
            const isRecommended = recommended === estimate.tier;
            const card = TIER_RATE_CARD[estimate.tier];
            const isEnterprise = estimate.tier === "enterprise";
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
                  {isEnterprise ? "Custom" : formatUsd(estimate.monthlyTotal)}
                </div>
                <div className={styles.tierSubprice}>
                  {isEnterprise
                    ? "Contact sales"
                    : period === "annual"
                      ? "/ month, billed annually"
                      : "/ month"}
                </div>
                {isEnterprise ? (
                  <p>
                    Enterprise is contract-based against your security,
                    compliance, and capacity requirements. Talk to us.
                  </p>
                ) : estimate.fitsWithoutOverage ? (
                  <p className={styles.fitsOk}>
                    <strong>Fits.</strong> Your usage is within{" "}
                    {card.includedCasGb.toLocaleString("en-US")} GB /{" "}
                    {card.includedRequests.toLocaleString("en-US")} req
                    included.
                  </p>
                ) : (
                  <p className={styles.fitsExceeded}>
                    <strong>Exceeds quota.</strong> Hard cap at 100% — pick a
                    larger tier to avoid <code>429 Quota Exceeded</code>.
                  </p>
                )}
                <small>
                  Included: {card.includedCasGb.toLocaleString("en-US")} GB
                  CAS, {card.includedRequests.toLocaleString("en-US")} req/mo.
                </small>
              </article>
            );
          })}
        </section>

        <h2>Headroom under your tier</h2>
        {breakEven !== null ? (
          <div className={styles.breakEven}>
            <p>
              Your usage profile fits inside{" "}
              <strong>{TIER_RATE_CARD[breakEven.recommended].label}</strong>{" "}
              without overage. Headroom before you would move up:
            </p>
            <ul>
              <li>
                {breakEven.headroomCasGb.toLocaleString("en-US")} GB CAS
                storage remaining
              </li>
              <li>
                {breakEven.headroomRequests.toLocaleString("en-US")} cache
                requests remaining this month
              </li>
            </ul>
            <a
              className={styles.cta}
              href={breakEven.recommended === "free" ? SIGNUP_URL : UPGRADE_PRO_URL}
            >
              {breakEven.recommended === "free"
                ? "Start free"
                : "Upgrade to Pro"}
            </a>
          </div>
        ) : (
          <div className={styles.breakEven}>
            <p>
              Your usage exceeds the Pro tier&apos;s included quotas. Talk to
              us about <strong>Enterprise</strong> — pricing is contract-based
              and tuned to your capacity, compliance, and SLA needs.
            </p>
            <a className={styles.cta} href={SALES_MAILTO}>
              Contact sales
            </a>
          </div>
        )}
      </main>
    </Layout>
  );
}
