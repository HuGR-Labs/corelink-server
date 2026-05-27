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
          <h2>Change notification policy</h2>
          <p>
            CoreLink commits to a <strong>30-day prior notice</strong> before
            any new sub-processor begins processing customer personal data.
            Notices are DKIM-signed and delivered to the account owner
            contact. To subscribe an additional email address (security,
            procurement, DPO) to sub-processor notices, email{" "}
            <a href="mailto:privacy@humangr.com">privacy@humangr.com</a> with
            the subject &ldquo;Sub-processor notice subscription&rdquo;.
          </p>
          <p>
            You have the right to object to a new sub-processor on
            reasonable grounds. If your objection cannot be resolved within
            the 30-day window, you may terminate the affected portion of the
            Service per the Terms §10 with a pro-rated refund.
          </p>
        </section>

        <section className={styles.section}>
          <h2>How this page stays accurate</h2>
          <p>
            The active table is generated from{" "}
            <code>apps/admin-ui/src/content/sub-processors.json</code> at
            build time — the same file the Admin UI privacy console
            consumes. Adding a sub-processor is a single JSON edit that
            propagates to both surfaces on the next deploy, eliminating the
            two-surface drift class of compliance bug. The deeper compliance
            explainer (DPA Annex I/II, Schrems II Transfer Impact
            Assessments, ROPA mapping) lives at{" "}
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
