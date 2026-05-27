/**
 * Public sub-processors page — Phase 0 §A `LEGAL-FOOTER-WIRE` deliverable.
 *
 * Wraps `apps/admin-ui/src/content/sub-processors.json` as the single
 * source of truth — keeping this page and the Admin UI privacy console in
 * lock-step so a single edit propagates to both surfaces. The JSON is
 * imported directly (Docusaurus + esbuild handles JSON imports natively);
 * editing the JSON triggers a docs rebuild via the existing CI on push.
 *
 * Charter note: ships the conservative subset (Cloudflare, Clerk, Stripe)
 * that the canonical JSON authorises as actively processing personal data
 * today, per `specs/_audits/2026-05-27-phase-0-execution-plan.md` §2.A
 * hard-pause trigger (b). The plan-for-GA addendum below transparently
 * lists providers under integration review (Neon, Resend, Sentry,
 * Plausible/PostHog) without representing them as live processors — exactly
 * the conservative posture the spec requires.
 */

import Layout from "@theme/Layout";
import type { ReactElement } from "react";

import subProcessorsData from "../../../../admin-ui/src/content/sub-processors.json";

import styles from "./legal.module.css";

interface SubProcessor {
  id: string;
  name: string;
  role: string;
  region: string;
  certifications: string[];
  last_audit: string;
}

interface SubProcessorList {
  version: string;
  items: SubProcessor[];
}

interface PendingSubProcessor {
  name: string;
  role: string;
  region: string;
  notes: string;
}

// Providers under integration review — listed transparently per the
// `subject to final legal review prior to GA` posture, but NOT represented
// as active sub-processors until they appear in the canonical JSON.
const PENDING_SUB_PROCESSORS: PendingSubProcessor[] = [
  {
    name: "Neon Inc.",
    role: "Managed Postgres for control-plane metadata (account, billing, consent, DSR rows)",
    region: "EU (Frankfurt) for EU tenants; US (us-east-1) for US tenants",
    notes:
      "Selected; integration in flight; will move to the active list once the production cutover commits.",
  },
  {
    name: "Resend",
    role: "Transactional email (account verification, DSR receipts, breach notifications)",
    region: "United States",
    notes:
      "Selected; SDK wired in `signup-worker`; activation pending production secret apply (CF-Pages-Secrets-Prep runbook).",
  },
  {
    name: "Sentry",
    role: "Application error tracking (opt-in for EU subjects pending Schrems II TIA)",
    region: "United States (EU region available on Enterprise plan)",
    notes:
      "Disabled by default for EU subjects until the Sentry-specific TIA is on file (tracked as GAP-14 in `GDPR-SCC-EXECUTION-2026-05-15.md`).",
  },
  {
    name: "Plausible Analytics (preferred) / PostHog Cloud EU (fallback)",
    role: "Privacy-preserving website analytics on public docs and Admin UI",
    region: "EU (DE) — Plausible Cloud; EU (Frankfurt) — PostHog Cloud EU",
    notes:
      "Gated by cookie-consent `analytics` category; falls back to PostHog if Plausible Cloud is rate-limited at the Hobby tier.",
  },
];

const subProcessors: SubProcessorList =
  subProcessorsData as SubProcessorList;

function formatCertifications(certs: string[]): string {
  return certs.length === 0 ? "—" : certs.join(", ");
}

export default function SubProcessorsPage(): ReactElement {
  const { version, items } = subProcessors;

  return (
    <Layout
      title="Sub-processors"
      description="CoreLink sub-processors: third-party service providers that process customer personal data on behalf of HuGR Labs, with regions, certifications, and last-audit dates."
    >
      <main className={styles.page}>
        <header className={styles.header}>
          <h1>Sub-processors</h1>
          <p className={styles.meta}>
            Canonical list version <code>{version}</code> · Source of truth:{" "}
            <code>apps/admin-ui/src/content/sub-processors.json</code>
          </p>
        </header>

        <p className={styles.draftNote} role="status">
          <strong>Subject to final legal review prior to GA.</strong> The
          table below mirrors the canonical JSON consumed by the Admin UI
          privacy console; the &ldquo;Under integration review&rdquo; section
          lists providers selected for activation before GA but not yet
          processing customer personal data in production.
        </p>

        <section className={styles.section}>
          <h2>Active sub-processors</h2>
          <p>
            These providers process customer personal data on behalf of HuGR
            Labs today. Each entry includes the processing role, region of
            primary processing, current third-party security certifications,
            and the date of the most recent vendor-security review on file.
          </p>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">Provider</th>
                <th scope="col">Role</th>
                <th scope="col">Region</th>
                <th scope="col">Certifications</th>
                <th scope="col">Last review</th>
              </tr>
            </thead>
            <tbody>
              {items.map((sp) => (
                <tr key={sp.id}>
                  <td>
                    <strong>{sp.name}</strong>
                  </td>
                  <td>{sp.role}</td>
                  <td>{sp.region}</td>
                  <td>{formatCertifications(sp.certifications)}</td>
                  <td>
                    <time dateTime={sp.last_audit}>{sp.last_audit}</time>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>

        <section className={styles.section}>
          <h2>Under integration review (pre-GA)</h2>
          <p>
            The providers below are selected for activation before General
            Availability but are <strong>not</strong> currently processing
            customer personal data in production. They are listed
            transparently so prospective customers can vet the planned
            stack; each will move to the active table above on its production
            cutover, with the standard 30-day prior notice.
          </p>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">Provider</th>
                <th scope="col">Planned role</th>
                <th scope="col">Region</th>
                <th scope="col">Status</th>
              </tr>
            </thead>
            <tbody>
              {PENDING_SUB_PROCESSORS.map((sp) => (
                <tr key={sp.name}>
                  <td>
                    <strong>{sp.name}</strong>
                  </td>
                  <td>{sp.role}</td>
                  <td>{sp.region}</td>
                  <td>{sp.notes}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>

        <section className={styles.section}>
          <h2>Active sub-processor profiles</h2>
          <h3>Cloudflare, Inc.</h3>
          <p>
            Cloudflare is the foundation of CoreLink&rsquo;s delivery path:
            Workers execute every API request, Durable Objects host
            per-tenant state, R2 holds the content-addressable object store,
            KV holds session and rate-limit indexes, Queues mediate the
            asynchronous fan-out, and the anycast edge terminates TLS for
            the public surfaces. Personal data Cloudflare necessarily
            observes is limited to (a) source IP addresses for routing and
            DDoS mitigation, (b) HTTP headers, (c) authenticated request
            bodies in transit, and (d) at-rest state for tenant resources
            in R2/KV/D1/Durable Objects. Cloudflare commits to a strict
            sub-processor list of its own, contractually flows down GDPR
            Article 28 obligations through its Master Subscription
            Agreement and DPA, and supports EU-only Workers and R2
            jurisdiction restrictions on Enterprise plans (planned for
            CoreLink Enterprise tier post-GA). Cloudflare is DPF-certified
            (active enrolment with the US Department of Commerce); EU/EEA
            transfers additionally rely on the EU SCC 2021/914 Module 3
            (processor-to-processor) flow embedded in the Cloudflare DPA.
          </p>
          <h3>Clerk, Inc.</h3>
          <p>
            Clerk is the identity provider for both the Admin UI and the
            CoreLink API token-exchange flow. Clerk observes (a) name and
            primary email at signup, (b) OAuth/OIDC subject identifiers
            from upstream identity providers (Google, GitHub, Microsoft),
            (c) MFA factor enrolments (TOTP secret hashes, WebAuthn
            credential IDs — never the credential private material),
            (d) session metadata (issue time, user agent, source IP
            network, last-active timestamp). Clerk does <strong>not</strong>{" "}
            observe API payloads, customer cache content, billing data, or
            DSR payloads — those flow exclusively through the CoreLink
            Worker. Clerk is SOC 2 Type II audited; the most recent report
            is dated 30 January 2026 and is available under NDA via the
            CoreLink trust portal. Clerk operates from the United States;
            EU/EEA transfers rely on SCC 2021/914 Module 2 plus Clerk&rsquo;s
            Schrems II Transfer Impact Assessment, reviewed by HuGR Labs on
            file as part of the vendor onboarding (entry SP-VENDOR-CLERK-01
            in the ROPA).
          </p>
          <h3>Stripe, Inc.</h3>
          <p>
            Stripe processes all card payments and operates the billing
            invoice generation pipeline. Cardholder data never traverses
            CoreLink infrastructure — the Checkout and Customer Portal flows
            redirect the browser to Stripe-controlled domains, and CoreLink
            stores only the Stripe customer identifier, the subscription
            identifier, and the metered-usage event identifiers. Stripe is
            PCI DSS Level 1 attested and SOC 2 Type II audited; the latest
            attestations are dated 4 March 2026. EU/EEA transfers operate
            under Stripe Payments Europe Ltd. (Dublin, Ireland) as the data
            controller for EU customers, eliminating most onward-transfer
            considerations for that population; US-originated traffic flows
            through Stripe, Inc. with DPF-certified status and SCC backstop.
          </p>
        </section>

        <section className={styles.section}>
          <h2>Transfer mechanisms in force</h2>
          <p>
            Every cross-border transfer of personal data is supported by
            one of the mechanisms below, each documented in the per-vendor
            entry of the CoreLink Records of Processing Activities (ROPA):
          </p>
          <ul>
            <li>
              <strong>EU-US Data Privacy Framework (DPF):</strong> active for
              Cloudflare, Clerk, and Stripe (US entity). Adequacy decision
              of 10 July 2023 (Commission Implementing Decision (EU)
              2023/1795) supplies the lawful basis. Withdrawal or
              suspension of the DPF triggers an automatic fallback to the
              SCC mechanism listed below — the contractual flow-down is
              already in place in every executed DPA so no separate
              re-papering is required.
            </li>
            <li>
              <strong>Standard Contractual Clauses (SCC):</strong> EU
              Commission Implementing Decision 2021/914, Module 2
              (controller-to-processor) and Module 3
              (processor-to-processor), backstops every US transfer. Each
              executed SCC is paired with a Schrems II Transfer Impact
              Assessment (TIA) covering (i) the legal regime of the
              recipient country (FISA §702 surveillance scope, Executive
              Order 12333), (ii) the technical and organizational measures
              the importer applies (encryption-in-transit and at-rest, key
              isolation, access-control review cadence), and (iii) the
              practical likelihood that government access requests will
              reach the importer (sector, customer profile, historical
              transparency-report data).
            </li>
            <li>
              <strong>UK International Data Transfer Addendum:</strong> the
              UK ICO Addendum (in force from 21 March 2022) layers onto the
              EU SCCs for any onward transfer from a UK-established
              controller; CoreLink countersigns the Addendum where the
              counterparty is UK-based.
            </li>
            <li>
              <strong>LGPD international transfer mechanism (Art. 33):</strong>{" "}
              transfers from Brazil rely on the
              standard-contractual-clauses model published by ANPD on
              23 August 2024 (Resolução CD/ANPD nº 19/2024), executed
              alongside the GDPR SCCs as a single integrated annex to the
              CoreLink DPA.
            </li>
          </ul>
        </section>

        <section className={styles.section}>
          <h2>Change notification policy</h2>
          <p>
            CoreLink commits to a <strong>30-day prior notice</strong> before
            any new sub-processor begins processing customer personal data,
            and to the same 30-day notice before a sub-processor materially
            changes its processing role, region, or sub-sub-processor list.
            Notices are emitted by the{" "}
            <code>corelink_privacy::sub_processor</code> Rust module, signed
            with DKIM at the SMTP edge, and delivered to (a) the account
            owner of record, (b) every email address subscribed to the
            sub-processor notification distribution, and (c) the DPO contact
            of each Enterprise tenant. The notice mechanism is GDPR
            Article 28(2) compliant and is mirrored in the executed DPA at
            §16 (&ldquo;Changes to Sub-processors&rdquo;).
          </p>
          <p>
            To subscribe an additional email address (security, procurement,
            DPO) to sub-processor notices, email{" "}
            <a href="mailto:privacy@humangr.com">privacy@humangr.com</a> with
            the subject &ldquo;Sub-processor notice subscription&rdquo; and
            include the tenant identifier you administer. Subscriptions are
            confirmed by an opt-in link delivered within one business day.
          </p>
          <p>
            You have the right to object to a new sub-processor on
            reasonable grounds (for example, an established adverse legal
            judgment, a regulator enforcement action, or an unresolved
            audit-finding pattern). If the objection cannot be resolved
            within the 30-day window — by selecting an alternative
            sub-processor, restricting the processing scope, or offering a
            jurisdictional carve-out — you may terminate the affected
            portion of the Service per the Terms §10 with a pro-rated
            refund of pre-paid unused fees.
          </p>
        </section>

        <section className={styles.section}>
          <h2>Audit, assistance, and documentation rights</h2>
          <p>
            On reasonable written notice and subject to confidentiality
            commitments, CoreLink will make available to each controller
            customer (a) the third-party audit reports of every active
            sub-processor (SOC 2 Type II, ISO 27001, ISO 27018, PCI DSS as
            applicable), (b) the executed DPA between CoreLink and each
            sub-processor, (c) the Schrems II Transfer Impact Assessment
            for each US-onward transfer, (d) the relevant sections of the
            CoreLink Records of Processing Activities (ROPA) covering
            shared sub-processors, and (e) annual penetration-test
            executive summaries. Audit rights under GDPR Art. 28(3)(h) and
            LGPD Art. 39 are met first by these documents; on-site audits
            are available to Enterprise tier customers under the executed
            Order Form, with audit costs allocated per the standard
            inspection-with-cause / inspection-without-cause split.
            CoreLink also provides assistance with controller-side DPIA
            (GDPR Art. 35) and ANPD Relatório de Impacto (LGPD Art. 38)
            obligations, including pre-completed processor-side annexes
            that map our processing activities to the relevant risk
            categories.
          </p>
        </section>

        <section className={styles.section}>
          <h2>Vendor evaluation and audit cadence</h2>
          <p>
            Every sub-processor is onboarded only after an internal Vendor
            Security Review (VSR) is recorded in the ROPA. The VSR checklist
            covers (a) third-party attestations (SOC 2, ISO 27001, ISO
            27018, PCI DSS, HIPAA where relevant), (b) the executed DPA and
            its sub-processor clauses, (c) the data-processing region and
            backup/replication geography, (d) the Schrems II Transfer
            Impact Assessment where applicable, (e) the breach-notification
            SLA from the sub-processor to CoreLink (we require a sub-72-hour
            inbound notification to remain within our outbound 72-hour
            obligation under GDPR Art. 33), and (f) the data-deletion
            commitment on termination. CoreLink reviews each active
            sub-processor at least annually; the &ldquo;Last review&rdquo;
            column in the active table is the most recent VSR date on file.
          </p>
        </section>

        <section className={styles.section}>
          <h2>Data categories per sub-processor</h2>
          <p>
            The table below maps each active sub-processor to the
            categories of personal data it observes, supporting the GDPR
            Art. 30 Records of Processing obligation and the LGPD Art. 37
            equivalent. Categories follow the taxonomy in the Privacy
            Notice §2.
          </p>
          <table className={styles.table}>
            <thead>
              <tr>
                <th scope="col">Provider</th>
                <th scope="col">Identity</th>
                <th scope="col">Auth</th>
                <th scope="col">Usage telemetry</th>
                <th scope="col">Support</th>
                <th scope="col">Tenant content</th>
                <th scope="col">Payment</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>
                  <strong>Cloudflare</strong>
                </td>
                <td>—</td>
                <td>Yes (session in transit)</td>
                <td>Yes (edge logs)</td>
                <td>—</td>
                <td>Yes (R2 / D1 / KV / DO)</td>
                <td>—</td>
              </tr>
              <tr>
                <td>
                  <strong>Clerk</strong>
                </td>
                <td>Yes</td>
                <td>Yes</td>
                <td>—</td>
                <td>—</td>
                <td>—</td>
                <td>—</td>
              </tr>
              <tr>
                <td>
                  <strong>Stripe</strong>
                </td>
                <td>Yes (billing email)</td>
                <td>—</td>
                <td>—</td>
                <td>—</td>
                <td>—</td>
                <td>Yes</td>
              </tr>
            </tbody>
          </table>
          <p>
            &ldquo;Yes&rdquo; indicates the sub-processor structurally
            handles the category as part of providing its service.
            &ldquo;—&rdquo; indicates the category is not observed by that
            sub-processor under the current CoreLink deployment. CoreLink
            itself is the controller-side processor that retains the
            relationship to the customer-tenant data subject; the
            sub-processors operate strictly under instruction.
          </p>
        </section>

        <section className={styles.section}>
          <h2>How this page stays accurate</h2>
          <p>
            The active table is generated from{" "}
            <code>apps/admin-ui/src/content/sub-processors.json</code> at
            build time — the same file the Admin UI privacy console
            consumes via the canonical{" "}
            <code>corelink_privacy::sub_processor</code> Rust binding.
            Adding a sub-processor is a single JSON edit that propagates to
            both surfaces on the next deploy, eliminating the two-surface
            drift class of compliance bug. The deeper compliance explainer
            (DPA Annex I/II, Schrems II Transfer Impact Assessments, ROPA
            mapping) lives at{" "}
            <a href="/explanation/compliance/sub-processors">
              /explanation/compliance/sub-processors
            </a>
            .
          </p>
        </section>

        <p className={styles.footnote}>
          See also: <a href="/legal/privacy">Privacy Notice</a> ·{" "}
          <a href="/legal/terms">Terms of Service</a> ·{" "}
          <a href="/explanation/compliance/dpa">Data Processing Addendum</a>
        </p>
      </main>
    </Layout>
  );
}
