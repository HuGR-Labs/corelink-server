/**
 * Public privacy page — Phase 0 §A `LEGAL-FOOTER-WIRE` deliverable.
 *
 * Mirrors the canonical content from `apps/admin-ui/src/content/privacy-notice.en.md`
 * (the single source of truth for end-user-facing privacy text) and adds a
 * controller-of-record block summarising the GDPR/LGPD regimes that govern
 * CoreLink processing. Deep technical detail (article-by-article DSR table,
 * 12-backend erasure pipeline, SCC/DPF transfer mechanisms) lives in the
 * Diátaxis explainers at `/explanation/privacy/gdpr` and
 * `/explanation/privacy/lgpd-full` — this public page links out rather than
 * duplicating, keeping the legal surface short enough for a non-specialist
 * reader to absorb in under two minutes.
 *
 * Charter note: shipped with the `subject to final legal review prior to GA`
 * banner per `specs/_audits/2026-05-27-phase-0-execution-plan.md` §2.A
 * hard-pause trigger (c). Termageddon ($119/yr, Phase 1) supersedes this
 * page when first enterprise contract negotiates.
 */

import Layout from "@theme/Layout";
import type { ReactElement } from "react";

import styles from "./legal.module.css";

const EFFECTIVE_DATE = "2026-05-27";
const VERSION = "1.0.0";

export default function PrivacyPage(): ReactElement {
  return (
    <Layout
      title="Privacy"
      description="CoreLink privacy notice: how CoreLink (HuGR Labs) collects, processes, and protects personal data under GDPR, LGPD, and US-state privacy statutes (CCPA/CPRA, VCDPA)."
    >
      <main className={styles.page}>
        <header className={styles.header}>
          <h1>Privacy Notice</h1>
          <p className={styles.meta}>
            Version {VERSION} · Effective {EFFECTIVE_DATE} · Maintained by HuGR
            Labs Privacy Team
          </p>
        </header>

        <p className={styles.draftNote} role="status">
          <strong>Subject to final legal review prior to GA.</strong> This
          notice is in force for the current pre-GA period and accurately
          describes how CoreLink processes data today. A counter-signed
          Termageddon-managed update lands before the first enterprise
          contract is executed; substantive changes will be announced at least
          30 days in advance via the in-app banner and the administrator email
          on file.
        </p>

        <p>
          CoreLink (&ldquo;we&rdquo;, &ldquo;us&rdquo;, &ldquo;our&rdquo;) is
          a product operated by HuGR Labs. We process personal data on behalf
          of our customers (&ldquo;controllers&rdquo;) as a data{" "}
          <strong>processor</strong> under the EU General Data Protection
          Regulation (Regulation 2016/679 — GDPR), the Brazilian Lei Geral de
          Proteção de Dados (Lei 13.709/2018 — LGPD), and applicable
          state-level US privacy statutes (CCPA/CPRA, VCDPA, CPA, CTDPA,
          UCPA). This notice describes the categories of personal data we
          collect when you visit our public surfaces (this site, our marketing
          pages, the support portal) or use the CoreLink Admin UI as an end
          user of a customer tenant.
        </p>

        <section className={styles.section}>
          <h2>1. Controller of record</h2>
          <table className={styles.table}>
            <tbody>
              <tr>
                <th scope="row">Legal entity (current)</th>
                <td>HuGR Labs — sole-proprietorship of Gustavo Schneiter</td>
              </tr>
              <tr>
                <th scope="row">Legal entity (post-incorporation)</th>
                <td>
                  CoreLink, Inc. — Delaware C-corp, Stripe Atlas filing in
                  flight; the corporate successor inherits this notice with no
                  break in the data-protection chain.
                </td>
              </tr>
              <tr>
                <th scope="row">Privacy contact</th>
                <td>
                  <a href="mailto:privacy@humangr.com">privacy@humangr.com</a>{" "}
                  (interim Data Protection Officer:{" "}
                  <a href="mailto:gustavo@humangr.com">gustavo@humangr.com</a>)
                </td>
              </tr>
              <tr>
                <th scope="row">EU representative (Art. 27)</th>
                <td>
                  Appointment in flight; published here before the first
                  EU-established Lighthouse customer goes live.
                </td>
              </tr>
              <tr>
                <th scope="row">Brazilian DPO (LGPD Art. 41)</th>
                <td>
                  Gustavo Schneiter (interim) —{" "}
                  <a href="mailto:dpo@humangr.com">dpo@humangr.com</a>
                </td>
              </tr>
            </tbody>
          </table>
        </section>

        <section className={styles.section}>
          <h2>2. Categories of personal data</h2>
          <ul>
            <li>
              <strong>Identity data:</strong> name, email address, organization
              affiliation, locale preference.
            </li>
            <li>
              <strong>Authentication data:</strong> hashed session tokens,
              OAuth/OIDC subject identifiers from your identity provider, MFA
              enrollment status.
            </li>
            <li>
              <strong>Usage telemetry:</strong> API endpoint, latency, response
              status, anonymized IP-network prefix (truncated to /24 for IPv4,
              /48 for IPv6).
            </li>
            <li>
              <strong>Support context:</strong> information you voluntarily
              disclose in support tickets.
            </li>
            <li>
              <strong>Customer tenant content (controller-owned):</strong> the
              blobs your tenant uploads to the content-addressable cache. We
              store the bytes, a content-addressable hash, and minimal
              metadata; we do <strong>not</strong> inspect the content. Where
              your blobs embed third-party personal data, you remain the
              controller and we act as your processor under our DPA.
            </li>
          </ul>
        </section>

        <section className={styles.section}>
          <h2>3. Purposes and legal bases</h2>
          <p>
            We process the categories above to (a) provide the Service under
            our contract with you or your organization (GDPR Art. 6(1)(b)
            / LGPD Art. 7-V); (b) comply with legal obligations such as
            financial-record retention and audit-log preservation (Art. 6(1)(c)
            / Art. 7-II); and (c) pursue our legitimate interest in
            maintaining product reliability and security (Art. 6(1)(f)
            / Art. 7-IX). Where consent is required, we collect it via the
            cookie banner and the Consent Management UI; consent can be
            withdrawn at any time with the same friction as granting.
          </p>
        </section>

        <section className={styles.section}>
          <h2>4. Retention</h2>
          <p>
            Personal data is retained only as long as needed for the purposes
            above. Default retention windows:
          </p>
          <ul>
            <li>Authentication logs — 90 days.</li>
            <li>
              Audit trail — 7 years (SOC 2 / SOX baseline; mandatory for
              processor-side accountability).
            </li>
            <li>Support tickets — 3 years from closure.</li>
            <li>
              Customer tenant content — for the duration of the tenant&rsquo;s
              subscription plus 30 calendar days of grace, after which a
              tombstone replay erases CAS, AC, audit-PII, and KV bindings.
            </li>
          </ul>
        </section>

        <section className={styles.section}>
          <h2>5. Your rights</h2>
          <p>
            You may exercise the rights of access, rectification, deletion,
            restriction, objection, and portability granted by GDPR Articles
            15–22, LGPD Articles 18–22, and analogous US-state statutes via
            the Data-Subject Request (DSR) flow in your account, or by
            emailing <a href="mailto:privacy@humangr.com">privacy@humangr.com</a>.
            We respond within 30 calendar days (45 under CCPA, with one 45-day
            extension permitted under Cal. Civ. Code §1798.130).
          </p>
          <p>
            For the full article-by-article SLA table, the WebAuthn step-up
            policy for destructive requests, and the 12-backend erasure
            propagation order, see the explainers at{" "}
            <a href="/explanation/privacy/gdpr">GDPR rights</a> and{" "}
            <a href="/explanation/privacy/lgpd-full">LGPD rights</a>.
          </p>
        </section>

        <section className={styles.section}>
          <h2>6. International transfers</h2>
          <p>
            CoreLink processes data on Cloudflare&rsquo;s anycast edge plus a
            Neon-hosted Postgres control plane (EU or US region, selectable).
            Transfers from the EU/EEA to the US rely on the EU-US Data Privacy
            Framework (DPF) adequacy decision (10 July 2023) and on Standard
            Contractual Clauses (Commission Implementing Decision 2021/914,
            Module 2) where the recipient is not DPF-certified. A Schrems II
            Transfer Impact Assessment is on file for every US-onward leg; the
            register is available on request from{" "}
            <a href="mailto:privacy@humangr.com">privacy@humangr.com</a>.
          </p>
        </section>

        <section className={styles.section}>
          <h2>7. Sub-processors</h2>
          <p>
            The current list of sub-processors and their regions is published
            on the <a href="/legal/sub-processors">Sub-processors page</a>.
            Material changes are announced at least 30 days in advance via
            in-app banner and DKIM-signed email to administrators.
          </p>
        </section>

        <section className={styles.section}>
          <h2>8. Cookies and analytics</h2>
          <p>
            We use a small set of first-party cookies for session continuity,
            cookie-consent state, and (when you opt in to the
            &ldquo;analytics&rdquo; category) privacy-preserving usage
            measurement via Plausible. We do not run cross-site tracking, ad
            networks, or browser fingerprinting. The consent banner offers a
            one-click reject for every non-essential category.
          </p>
        </section>

        <section className={styles.section}>
          <h2>9. Breach notification</h2>
          <p>
            We notify affected controllers without undue delay, and in any
            case within <strong>72 hours</strong> of becoming aware of a
            personal data breach (GDPR Art. 33 / LGPD Art. 48). Our internal
            timeline targets supervisory-authority notification within 48
            hours — beating the statutory cap by 24 hours. Affected data
            subjects receive notice when the breach is likely to result in
            high risk to their rights and freedoms.
          </p>
        </section>

        <section className={styles.section}>
          <h2>10. Contact and complaints</h2>
          <p>
            Data Protection contact —{" "}
            <a href="mailto:privacy@humangr.com">privacy@humangr.com</a>.
            Postal address forthcoming with the Stripe Atlas Delaware
            registration. You have the right to lodge a complaint with your
            local supervisory authority (ANPD in Brazil; your member-state
            DPA in the EU/EEA; the relevant state attorney general in the US)
            and to seek a judicial remedy.
          </p>
        </section>

        <p className={styles.footnote}>
          See also: <a href="/legal/terms">Terms of Service</a> ·{" "}
          <a href="/legal/sub-processors">Sub-processors</a> ·{" "}
          <a href="/explanation/compliance/dpa">Data Processing Addendum</a>
        </p>
      </main>
    </Layout>
  );
}
