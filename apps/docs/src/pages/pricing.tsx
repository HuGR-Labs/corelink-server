/**
 * Public pricing page — wave-29 stream-7 r-prep deliverable.
 *
 * 5-tier comparison (Free / Starter / Team / Pro / Enterprise) following
 * the canonical wave-13 tier taxonomy. Every numeric value is provisional
 * per honest pre-GA framing; rates remain `<TBD per GA pricing review>`
 * until Finance / Legal / Product sign off in CONTENT-REVIEW.md.
 */

import Layout from "@theme/Layout";
import { useState } from "react";
import type { ReactElement } from "react";

import {
  CANONICAL_TIERS,
  TIER_RATE_CARD,
  applyPeriodDiscount,
  formatUsd,
} from "../lib/pricing";
import type { BillingPeriod, TierShape } from "../lib/pricing";

import styles from "./pricing.module.css";

function tierHeadlinePrice(card: TierShape, period: BillingPeriod): string {
  if (card.usdMonthlyBase === null) {
    return "Contact us";
  }
  if (card.usdMonthlyBase === 0) {
    return "$0";
  }
  const effective = applyPeriodDiscount(card.usdMonthlyBase, period);
  return `${formatUsd(effective)}/mo`;
}

function CheckOrDash({ included }: { included: boolean }): ReactElement {
  return included ? (
    <span className={styles.checkmark} aria-label="Included">
      {"✓"}
    </span>
  ) : (
    <span className={styles.dash} aria-label="Not included">
      {"—"}
    </span>
  );
}

export default function Pricing(): ReactElement {
  const [period, setPeriod] = useState<BillingPeriod>("monthly");

  return (
    <Layout
      title="Pricing"
      description="CoreLink plans: Free, Starter, Team, Pro, Enterprise. Pricing is provisional pending GA."
    >
      <main className={styles.page}>
        <header className={styles.header}>
          <h1>Pricing</h1>
          <p>
            Five plans — from a free single-developer tier to enterprise BYOK.
            Pick the smallest one that covers your usage.
          </p>
        </header>

        <div className={styles.honestNote} role="status">
          <strong>Pre-GA notice.</strong> Every dollar amount on this page is
          provisional and subject to refinement at GA. The shape (tier
          features, included quotas, overage axes) is stable; magnitudes float
          until Finance, Legal, and Product sign off. The pilot tier is free
          during evaluation — see <a href="/pilot/apply">Apply for pilot</a>.
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
            Annual (save 15%)
          </button>
        </div>

        <section aria-label="Tier headline cards" className={styles.tierGrid}>
          {CANONICAL_TIERS.map((tierId) => {
            const card = TIER_RATE_CARD[tierId];
            return (
              <article key={tierId} className={styles.tierCard}>
                <h3>{card.label}</h3>
                <div className={styles.tierPrice}>
                  {tierHeadlinePrice(card, period)}
                </div>
                <div className={styles.tierSubprice}>
                  {card.usdMonthlyBase === null
                    ? "Custom contract"
                    : period === "annual"
                      ? "billed annually"
                      : "billed monthly"}
                </div>
                <ul className={styles.tierFeatures}>
                  <li>{card.includedCasGb.toLocaleString("en-US")} GB CAS storage</li>
                  <li>
                    {card.includedTransferGb.toLocaleString("en-US")} GB
                    transfer / mo
                  </li>
                  <li>
                    {card.includedAuditEvents.toLocaleString("en-US")} audit
                    events / mo
                  </li>
                  <li>
                    {card.includedRegions} region
                    {card.includedRegions === 1 ? "" : "s"}
                  </li>
                  <li>
                    {card.includedByokProviders === 0
                      ? "No BYOK"
                      : `${card.includedByokProviders} BYOK provider${card.includedByokProviders === 1 ? "" : "s"}`}
                  </li>
                  <li>
                    {card.includedSeats} admin seat
                    {card.includedSeats === 1 ? "" : "s"}
                  </li>
                  <li>
                    Support:{" "}
                    {card.supportResponseHours === "custom"
                      ? "Custom SLA"
                      : `${card.supportResponseHours} h response`}
                  </li>
                </ul>
                <a className={styles.cta} href="/pilot/apply">
                  {card.usdMonthlyBase === null ? "Contact sales" : "Start pilot"}
                </a>
              </article>
            );
          })}
        </section>

        <h2>Feature comparison</h2>
        <p>
          Every numeric quota below is the included allowance for the plan; once
          you exceed it, the per-unit overage rate from the calculator applies.
        </p>

        <table className={styles.compareTable}>
          <thead>
            <tr>
              <th scope="col">Capability</th>
              {CANONICAL_TIERS.map((t) => (
                <th key={t} scope="col">
                  {TIER_RATE_CARD[t].label}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            <tr>
              <th scope="row">CAS storage (GB included)</th>
              {CANONICAL_TIERS.map((t) => (
                <td key={t}>
                  {TIER_RATE_CARD[t].includedCasGb.toLocaleString("en-US")}
                </td>
              ))}
            </tr>
            <tr>
              <th scope="row">Transfer (GB / mo included)</th>
              {CANONICAL_TIERS.map((t) => (
                <td key={t}>
                  {TIER_RATE_CARD[t].includedTransferGb.toLocaleString("en-US")}
                </td>
              ))}
            </tr>
            <tr>
              <th scope="row">Audit events / mo</th>
              {CANONICAL_TIERS.map((t) => (
                <td key={t}>
                  {TIER_RATE_CARD[t].includedAuditEvents.toLocaleString("en-US")}
                </td>
              ))}
            </tr>
            <tr>
              <th scope="row">Regions included</th>
              {CANONICAL_TIERS.map((t) => (
                <td key={t}>{TIER_RATE_CARD[t].includedRegions}</td>
              ))}
            </tr>
            <tr>
              <th scope="row">BYOK providers</th>
              {CANONICAL_TIERS.map((t) => (
                <td key={t}>
                  <CheckOrDash
                    included={TIER_RATE_CARD[t].includedByokProviders > 0}
                  />
                  {TIER_RATE_CARD[t].includedByokProviders > 0
                    ? `${TIER_RATE_CARD[t].includedByokProviders} included`
                    : ""}
                </td>
              ))}
            </tr>
            <tr>
              <th scope="row">Admin seats</th>
              {CANONICAL_TIERS.map((t) => (
                <td key={t}>{TIER_RATE_CARD[t].includedSeats}</td>
              ))}
            </tr>
            <tr>
              <th scope="row">Support response</th>
              {CANONICAL_TIERS.map((t) => {
                const r = TIER_RATE_CARD[t].supportResponseHours;
                return <td key={t}>{r === "custom" ? "Custom" : `${r} h`}</td>;
              })}
            </tr>
            <tr>
              <th scope="row">DPA + SOC 2 (when issued)</th>
              {CANONICAL_TIERS.map((t) => (
                <td key={t}>
                  <CheckOrDash included={t !== "free"} />
                </td>
              ))}
            </tr>
            <tr>
              <th scope="row">SSO (SAML / OIDC)</th>
              {CANONICAL_TIERS.map((t) => (
                <td key={t}>
                  <CheckOrDash included={t === "pro" || t === "enterprise"} />
                </td>
              ))}
            </tr>
            <tr>
              <th scope="row">SCIM provisioning</th>
              {CANONICAL_TIERS.map((t) => (
                <td key={t}>
                  <CheckOrDash included={t === "enterprise"} />
                </td>
              ))}
            </tr>
          </tbody>
        </table>

        <p>
          See the{" "}
          <a href="/pricing/calculator">interactive calculator</a> to plug in
          your usage profile and see per-tier estimates side-by-side. Numbers
          remain provisional until Finance signs off the rate card at GA.
        </p>
      </main>
    </Layout>
  );
}
