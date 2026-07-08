/**
 * Public sub-processors page — Phase 0 §A `LEGAL-FOOTER-WIRE` deliverable,
 * reconciled 2026-05-27 against actual first-party wiring evidence.
 *
 * Source of truth: `apps/admin-ui/src/content/sub-processors.json`
 * Audit cross-reference: `specs/_audits/2026-05-27-sub-processors-finalization.md`
 *
 * Vendor inventory verified by grep against:
 *   Cloudflare  — wrangler.toml (5 locations)
 *   Clerk       — apps/admin-ui/middleware.ts + sign-up page
 *   Stripe      — apps/admin-ui/src/app/api/checkout/session/route.ts
 *   Resend      — apps/admin-ui/src/app/api/newsletter/subscribe/route.ts
 *               + apps/analytics-worker/wrangler.toml (weekly digest)
 *   Sentry      — apps/admin-ui/sentry.{server,client,edge}.config.ts
 *   Plausible   — apps/docs/docusaurus.config.ts
 *   BetterStack — apps/docs/src/components/StatusPill/StatusPill.tsx
 *   GitHub      — .github/workflows/ (5+ workflow files)
 *   PostHog     — NOT wired; removed from active list.
 */

import Layout from "@theme/Layout";
import type { ReactElement } from "react";

import subProcessorsData from "../../../../admin-ui/src/content/sub-processors.json";

import styles from "./legal.module.css";

const GITHUB_REPO = "https://github.com/humangr-labs/corelink-server";
const COMMIT_HISTORY_URL = `${GITHUB_REPO}/commits/main/apps/docs/src/pages/legal/sub-processors.tsx`;
const NEWSLETTER_SUBSCRIBE_URL =
  "https://corelink-admin.humangr.com/newsletter";
const PRIVACY_EMAIL = "privacy@humangr.com";

interface SubProcessor {
  id: string;
  name: string;
  role: string;
  region: string;
  certifications: string[];
  dpa_url: string;
  last_audit: string;
}

interface SubProcessorList {
  version: string;
  items: SubProcessor[];
}

const subProcessors: SubProcessorList =
  subProcessorsData as SubProcessorList;

export default function SubProcessorsPage(): ReactElement {
  const { version, items } = subProcessors;

  return (
    <Layout
      title="Sub-processors"
      description="CoreLink sub-processors: third-party service providers that process customer personal data on behalf of HuGR Labs, with regions, certifications, and DPA source URLs."
    >
      <main className={styles.page}>
        <header className={styles.header}>
          <h1>Sub-processors</h1>
          <p className={styles.meta}>
            Version <code>{version}</code> &middot; Last updated{" "}
            <time dateTime={version}>{version}</time> &middot; Source:{" "}
            <code>apps/admin-ui/src/content/sub-processors.json</code>
          </p>
        </header>

        {/* ── Notification commitment ── */}
        <section className={styles.section}>
          <p>
            HuGR Labs will notify customers at least{" "}
            <strong>30 days before any new sub-processor</strong> begins
            processing customer personal data, and before any existing
            sub-processor materially changes its processing role, region, or
            own sub-processor list. To subscribe an additional address
            (security team, DPO, procurement) to these notices, email{" "}
            <a href={`mailto:${PRIVACY_EMAIL}`}>{PRIVACY_EMAIL}</a> with
            subject &ldquo;Sub-processor notice subscription&rdquo; and include
            your tenant identifier, or sign up at{" "}
            <a
              href={NEWSLETTER_SUBSCRIBE_URL}
              target="_blank"
              rel="noopener noreferrer"
            >
              {NEWSLETTER_SUBSCRIBE_URL}
            </a>
            .
          </p>
          <p>
            Any enterprise customer may request a counter-signed DPA. The
            default Common Paper DPA template applies otherwise. Executed DPAs
            are available from the{" "}
            <a href="/explanation/compliance/dpa">Data Processing Addendum</a>{" "}
            page.
          </p>
        </section>

        {/* ── Active sub-processor table ── */}
        <section className={styles.section}>
          <h2>Active sub-processors</h2>
          <p>
            These providers are currently wired and may process customer
            personal data on behalf of HuGR Labs. Each row carries a
            verifiable DPA or certification source URL.
          </p>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">Sub-processor</th>
                <th scope="col">Service</th>
                <th scope="col">Data processed</th>
                <th scope="col">Region</th>
                <th scope="col">DPA / Cert</th>
                <th scope="col">Effective</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>
                  <strong>Cloudflare, Inc.</strong>
                </td>
                <td>CDN, edge compute, R2, KV, Durable Objects, D1, Pages, Email Routing</td>
                <td>App traffic + customer artifacts (encrypted at rest)</td>
                <td>Global with regional pinning</td>
                <td>
                  <a
                    href="https://www.cloudflare.com/cloudflare-customer-dpa/"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    DPA
                  </a>{" "}
                  &middot; ISO 27001 &middot; SOC 2 Type II &middot; ISO 27018
                  &middot; PCI DSS L1 &middot; EU–US DPF
                </td>
                <td>
                  <time dateTime="2026-04-01">2026-04-01</time>
                </td>
              </tr>
              <tr>
                <td>
                  <strong>Clerk, Inc.</strong>
                </td>
                <td>Authentication + user identity (SSO, MFA)</td>
                <td>Email, name, password hash, session tokens</td>
                <td>United States (us-east-1)</td>
                <td>
                  <a
                    href="https://clerk.com/legal/dpa"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    DPA
                  </a>{" "}
                  &middot; SOC 2 Type II &middot; GDPR &middot; CCPA &middot;
                  EU–US DPF
                </td>
                <td>
                  <time dateTime="2026-04-01">2026-04-01</time>
                </td>
              </tr>
              <tr>
                <td>
                  <strong>Stripe, Inc.</strong>
                </td>
                <td>Payment processing + subscription billing</td>
                <td>Billing email + payment-method tokens (PCI-DSS L1)</td>
                <td>US; EU via Stripe Payments Europe Ltd. (Ireland)</td>
                <td>
                  <a
                    href="https://stripe.com/legal/dpa"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    DPA
                  </a>{" "}
                  &middot; PCI DSS L1 &middot; SOC 2 Type II &middot; ISO 27001
                  &middot; EU–US DPF
                </td>
                <td>
                  <time dateTime="2026-04-01">2026-04-01</time>
                </td>
              </tr>
              <tr>
                <td>
                  <strong>Resend</strong>
                </td>
                <td>Transactional email + newsletter Audience</td>
                <td>Recipient email + delivery status</td>
                <td>United States</td>
                <td>
                  <a
                    href="https://resend.com/legal/dpa"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    DPA
                  </a>{" "}
                  &middot; SOC 2 Type II (in progress per Resend docs)
                </td>
                <td>
                  <time dateTime="2026-05-27">2026-05-27</time>
                </td>
              </tr>
              <tr>
                <td>
                  <strong>Sentry (Functional Software, Inc.)</strong>
                </td>
                <td>Application error monitoring (SaaS Cloud)</td>
                <td>
                  Error events — PII scrubbed by config (Authorization, Cookie,
                  X-Api-Key, Proxy-Authorization, svix-* headers stripped)
                </td>
                <td>United States (Sentry SaaS Cloud)</td>
                <td>
                  <a
                    href="https://sentry.io/legal/dpa/"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    DPA
                  </a>{" "}
                  &middot; SOC 2 Type II &middot; GDPR
                </td>
                <td>
                  <time dateTime="2026-05-27">2026-05-27</time>
                </td>
              </tr>
              <tr>
                <td>
                  <strong>Plausible Analytics</strong>
                </td>
                <td>Privacy-first web analytics (EU cloud)</td>
                <td>
                  Anonymised event payloads — cookieless, no PII, no
                  fingerprinting
                </td>
                <td>European Union (Germany)</td>
                <td>
                  <a
                    href="https://plausible.io/data-policy"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    Data Policy
                  </a>{" "}
                  &middot; GDPR (cookieless by design)
                </td>
                <td>
                  <time dateTime="2026-05-27">2026-05-27</time>
                </td>
              </tr>
              <tr>
                <td>
                  <strong>Better Stack, Inc. (Better Uptime)</strong>
                </td>
                <td>Public status page</td>
                <td>Public status board only — no customer personal data</td>
                <td>European Union</td>
                <td>
                  <a
                    href="https://betterstack.com/privacy"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    Privacy Policy
                  </a>{" "}
                  &middot; GDPR
                </td>
                <td>
                  <time dateTime="2026-05-27">2026-05-27</time>
                </td>
              </tr>
              <tr>
                <td>
                  <strong>GitHub, Inc. (Microsoft)</strong>
                </td>
                <td>Source code hosting + CI for releases</td>
                <td>Code + issues + CI artifact logs</td>
                <td>United States</td>
                <td>
                  <a
                    href="https://docs.github.com/en/site-policy/privacy-policies/github-data-protection-agreement"
                    target="_blank"
                    rel="noopener noreferrer"
                  >
                    DPA
                  </a>{" "}
                  &middot; SOC 2 Type II &middot; ISO 27001 &middot; GDPR
                  &middot; EU–US DPF
                </td>
                <td>
                  <time dateTime="2026-04-01">2026-04-01</time>
                </td>
              </tr>
            </tbody>
          </table>
        </section>

        {/* ── Transfer mechanisms ── */}
        <section className={styles.section}>
          <h2>Transfer mechanisms in force</h2>
          <p>
            Every cross-border transfer of personal data is supported by one of
            the mechanisms below, documented per-vendor in the CoreLink Records
            of Processing Activities (ROPA):
          </p>
          <ul>
            <li>
              <strong>EU–US Data Privacy Framework (DPF):</strong> active for
              Cloudflare, Clerk, Stripe (US entity), and GitHub. Adequacy
              decision of 10 July 2023 (Commission Implementing Decision (EU)
              2023/1795). DPF withdrawal triggers automatic SCC fallback already
              contractually in place.
            </li>
            <li>
              <strong>Standard Contractual Clauses (SCC):</strong> EU
              Commission Implementing Decision 2021/914, Modules 2 and 3,
              backstops every US-bound transfer. Each SCC is paired with a
              Schrems II Transfer Impact Assessment on file.
            </li>
            <li>
              <strong>UK International Data Transfer Addendum:</strong> ICO
              Addendum (in force from 21 March 2022) layers onto EU SCCs for
              UK-originated transfers.
            </li>
            <li>
              <strong>LGPD transfer mechanism (Art. 33):</strong> ANPD
              Resolução CD/ANPD nº 19/2024 SCCs, executed as an integrated
              annex to the CoreLink DPA.
            </li>
          </ul>
        </section>

        {/* ── Change notification policy ── */}
        <section className={styles.section}>
          <h2>Change notification policy</h2>
          <p>
            CoreLink commits to a <strong>30-day prior notice</strong> before
            any new sub-processor begins processing customer personal data, and
            before a sub-processor materially changes its processing role,
            region, or sub-sub-processor list (GDPR Art. 28(2) compliant, per
            DPA §16). Notices are DKIM-signed and delivered to (a) the account
            owner of record, (b) every address subscribed to sub-processor
            notices, and (c) the DPO contact of each Enterprise tenant.
          </p>
          <p>
            You have the right to object to a new sub-processor on reasonable
            grounds. If unresolved within 30 days, you may terminate the
            affected portion of the Service with a pro-rated refund of
            pre-paid unused fees (Terms §10).
          </p>
        </section>

        {/* ── Audit and assistance rights ── */}
        <section className={styles.section}>
          <h2>Audit, assistance, and documentation rights</h2>
          <p>
            On reasonable written notice and subject to confidentiality
            commitments, CoreLink will make available (a) third-party audit
            reports of every active sub-processor, (b) executed DPA between
            CoreLink and each sub-processor, (c) Schrems II Transfer Impact
            Assessments, (d) relevant ROPA sections, and (e) annual pen-test
            executive summaries. CoreLink also provides assistance with
            controller-side DPIA (GDPR Art. 35) and ANPD Relatório de Impacto
            (LGPD Art. 38) obligations.
          </p>
        </section>

        <p className={styles.footnote}>
          See also:{" "}
          <a href="/legal/privacy">Privacy Notice</a> &middot;{" "}
          <a href="/legal/terms">Terms of Service</a> &middot;{" "}
          <a href="/explanation/compliance/dpa">Data Processing Addendum</a>
          {" "}&middot;{" "}
          <a
            href={COMMIT_HISTORY_URL}
            target="_blank"
            rel="noopener noreferrer"
          >
            Commit history (GitHub) — audit prior versions
          </a>
        </p>
      </main>
    </Layout>
  );
}
