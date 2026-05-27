/**
 * Trust — charter-aligned security controls summary at `/trust`.
 *
 * Phase-0 launch-readiness deliverable (2026-05-27). This page is the new
 * `/trust` landing; it surfaces the eight charter security controls that
 * CoreLink ships with TODAY (LIVE), each anchored to its SEAL audit and
 * primary spec evidence. The richer five-quadrant Trust Center now lives
 * at `/trust/center` (`src/pages/trust/center.tsx`) and is linked at the
 * bottom of this page.
 *
 * Honest pre-GA framing — same rules as `/trust/center` (see
 * `specs/_audits/sealed/2026-05-16-trust-center-consolidation.md` §C2):
 *
 *   - We DO NOT claim certifications we do not yet hold.
 *   - SOC 2 Type II is IN-AUDIT (observation window opened 2026-05-15),
 *     not "certified". A customer-facing "in progress" answer is the
 *     only correct framing pre-GA.
 *   - Every LIVE bullet maps to a sealed audit under
 *     `specs/_audits/sealed/` so procurement can verify with the source.
 *
 * Scope of THIS page: just the engineering-side controls. Compliance
 * frameworks (SOC 2, ISO 27001, GDPR/LGPD/CCPA, FedRAMP posture), privacy
 * posture, reliability, and operations sit on `/trust/center`. Long-form
 * single-page printable for procurement / DPO is `/trust/overview`
 * (`docs/trust/index.mdx`).
 *
 * Sources of truth (read before editing claims):
 *   - `specs/_audits/sealed/2026-05-16-inv-critical-tla-coverage-final.md`
 *     → 61 CRITICAL invariants, all TLA+ verified, PR+nightly green.
 *   - `specs/_audits/sealed/2026-05-15-audit-chain-retention.md`
 *     → RFC 6962-style audit chain, BLAKE3 hash chain, 7-year retention.
 *   - `specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md`
 *     → AWS KMS, GCP Cloud KMS, Azure Key Vault, HashiCorp Vault.
 *   - `specs/_audits/sealed/2026-05-14-license-audit.md`
 *     → cargo-deny enforcement on supply chain.
 *   - `specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md`
 *     → workspace-wide `#![forbid(unsafe_code)]`, per-tenant isolation.
 *
 * i18n: copy is wrapped in `<Translate>` so the 4 locales (en-US default,
 * pt-BR, es-419, de) pick up `i18n/<locale>/code.json` overrides.
 */

import Layout from "@theme/Layout";
import Link from "@docusaurus/Link";
import Translate from "@docusaurus/Translate";
import type { ReactElement } from "react";

interface Control {
  readonly id: string;
  readonly title: string;
  readonly summary: string;
  readonly evidence: string;
  readonly evidenceHref?: string;
}

/**
 * Eight charter controls that CoreLink enforces TODAY. Every entry is
 * LIVE (no aspirational items). Every `evidence` string names a sealed
 * audit under `specs/_audits/sealed/` (file paths only, no public URL —
 * specs are internal). The `evidenceHref`, when present, points at the
 * customer-facing trust page that summarizes the same evidence.
 */
const CONTROLS: readonly Control[] = [
  {
    id: "forbid-unsafe",
    title: "Workspace-wide #![forbid(unsafe_code)]",
    summary:
      "Every Rust crate in the CoreLink workspace declares forbid(unsafe_code). Memory-safety bugs at the language level are statically impossible across the entire control plane and data plane.",
    evidence:
      "specs/_audits/sealed/2026-05-22-wave32-prod-deploy-spec.md (production deploy gate) — enforced by cargo workspace lint config.",
  },
  {
    id: "tla-verified-invariants",
    title: "61 CRITICAL invariants verified in TLA+",
    summary:
      "Every invariant declared CRITICAL in the invariant registry §3 has a TLA+ model that TLC checks both on PR and nightly. Coverage is 61 / 61 — no exemptions. Failures block merge.",
    evidence:
      "specs/_audits/sealed/2026-05-16-inv-critical-tla-coverage-final.md (Wave-26 final pre-GA audit, verdict PASS).",
  },
  {
    id: "audit-chain",
    title: "RFC 6962-style audit chain (BLAKE3 hash chain)",
    summary:
      "Every state-changing operation is appended to a per-tenant Merkle-linked audit log keyed by BLAKE3. Inclusion proofs are exportable. Append-only is a TLA+-verified critical invariant (INV-AUDIT-APPEND-ONLY).",
    evidence:
      "specs/_audits/sealed/2026-05-15-audit-chain-retention.md — 7-year retention, Sigstore-mirrored transparency log.",
    evidenceHref: "/security/audit-chain",
  },
  {
    id: "byok-4-kms",
    title: "4-KMS BYOK (AWS / GCP / Azure / Vault)",
    summary:
      "Customers can bring their own key from any of four KMS providers. CoreLink only holds wrapped DEKs; revocation is a single API call that fails closed on the data plane (TLA+-verified). Drill rehearsed quarterly.",
    evidence:
      "specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md + 2026-05-14-byok-kill-switch-drill-aws.md (revoke drill, AWS arm).",
    evidenceHref: "/security/byok",
  },
  {
    id: "cargo-deny",
    title: "cargo-deny enforcement on supply chain",
    summary:
      "Every dependency, license, and advisory is gated by cargo-deny on PR and nightly. Disallowed licenses, vulnerable versions, and unaudited git sources fail CI immediately. The deny.toml is checked into the repo and reviewed at each wave close.",
    evidence:
      "specs/_audits/sealed/2026-05-14-license-audit.md — 100 % crate coverage, zero advisory bypasses.",
  },
  {
    id: "tenant-isolation",
    title: "Per-tenant isolation (data, audit, eviction)",
    summary:
      "Tenant scoping is a TLA+-verified critical invariant across CAS reads, AC reads, audit emissions, and eviction. A twin-tenant TLC model proves no cross-tenant observability under any interleaving. R2 buckets and DO namespaces are physically partitioned by tenant ID.",
    evidence:
      "INV-AC-TENANT-SCOPED, INV-AC-EVICT-TENANT-SCOPED, INV-CAS-TENANT-SCOPED — see specs/_audits/sealed/2026-05-16-inv-critical-tla-coverage-final.md §A.",
  },
  {
    id: "encryption",
    title: "TLS 1.3 in transit, AES-256-GCM at rest",
    summary:
      "All customer payloads are encrypted in transit (TLS 1.3, modern-cipher-only, no downgrade path) and at rest (AES-256-GCM via Cloudflare R2 platform encryption, layered under BYOK envelope encryption when a tenant supplies a KMS key).",
    evidence:
      "specs/_audits/sealed/2026-05-14-security-walkthrough-s13.md — transport + storage encryption walkthrough.",
    evidenceHref: "/trust/data-handling#encryption",
  },
  {
    id: "dpa",
    title: "DPA available (Common Paper template)",
    summary:
      "A Common Paper-based Data Processing Addendum is available for any customer requiring GDPR Art. 28 / 32 contractual coverage. The DPA is countersigned at order-form acceptance — no separate negotiation cycle.",
    evidence: "Procurement: contact security@humangr.com — Common Paper DPA template attached to MSA.",
    evidenceHref: "/legal/privacy",
  },
];

const STATUS_BADGE_STYLE = {
  display: "inline-block",
  padding: "0.125rem 0.5rem",
  borderRadius: "999px",
  fontSize: "0.75rem",
  fontWeight: 600,
  backgroundColor: "#dcfce7",
  color: "#14532d",
  border: "1px solid #16a34a",
  letterSpacing: "0.02em",
} as const;

function ControlCard({ control }: { readonly control: Control }): ReactElement {
  return (
    <article
      style={{
        border: "1px solid var(--ifm-color-emphasis-300)",
        borderRadius: "8px",
        padding: "1.25rem 1.5rem",
        marginBottom: "1rem",
        backgroundColor: "var(--ifm-background-surface-color)",
      }}
    >
      <header style={{ display: "flex", justifyContent: "space-between", alignItems: "baseline", gap: "1rem", marginBottom: "0.5rem" }}>
        <h3 style={{ margin: 0, fontSize: "1.125rem" }}>{control.title}</h3>
        <span style={STATUS_BADGE_STYLE}>LIVE</span>
      </header>
      <p style={{ margin: "0 0 0.75rem 0" }}>{control.summary}</p>
      <p style={{ margin: 0, fontSize: "0.875rem", color: "var(--ifm-color-emphasis-700)" }}>
        <strong>
          <Translate id="trust.evidence.label" description="Label preceding the citation to the SEAL audit / spec.">
            Evidence:
          </Translate>{" "}
        </strong>
        {control.evidenceHref ? (
          <>
            {control.evidence}{" "}
            <Link to={control.evidenceHref}>
              <Translate id="trust.evidence.viewPage" description="Inline link to the customer-facing trust page.">
                View customer-facing page →
              </Translate>
            </Link>
          </>
        ) : (
          control.evidence
        )}
      </p>
    </article>
  );
}

export default function TrustPage(): ReactElement {
  return (
    <Layout
      title="Trust — Security Controls"
      description="CoreLink security controls shipped today, each anchored to a sealed audit."
    >
      <main style={{ maxWidth: "880px", margin: "0 auto", padding: "2.5rem 1.5rem" }}>
        <h1>
          <Translate id="trust.title" description="Page title for /trust">
            Trust — Security Controls
          </Translate>
        </h1>
        <p style={{ fontSize: "1.0625rem", lineHeight: 1.6 }}>
          <Translate id="trust.intro" description="Intro paragraph below the H1 on /trust">
            Eight controls CoreLink enforces today, each anchored to a sealed
            audit and a TLA+-verified or cargo-deny-enforced gate. No
            aspirational items — every bullet below is LIVE in production.
          </Translate>
        </p>

        <section style={{ marginTop: "2rem" }} aria-labelledby="controls-heading">
          <h2 id="controls-heading" style={{ marginBottom: "1rem" }}>
            <Translate id="trust.controls.heading" description="Section heading listing the eight charter controls.">
              Charter security controls
            </Translate>
          </h2>
          {CONTROLS.map((control) => (
            <ControlCard key={control.id} control={control} />
          ))}
        </section>

        <section style={{ marginTop: "2.5rem" }} aria-labelledby="compliance-heading">
          <h2 id="compliance-heading">
            <Translate id="trust.compliance.heading" description="Section heading for compliance framing.">
              On compliance frameworks
            </Translate>
          </h2>
          <p>
            <Translate id="trust.compliance.body" description="Pre-GA honest framing on SOC 2 and ISO 27001 status.">
              We do not claim certifications we do not yet hold. SOC 2 Type I
              and ISO 27001:2022 are IN-AUDIT (observation window opened
              2026-05-15 with Drata). SOC 2 Type II requires a 6-month
              continuous-operation window before report issuance. If a
              customer asks about SOC 2 status today, the correct answer is
              "in progress" — not "certified". The five-quadrant Trust
              Center below has the current status of every framework with
              source evidence.
            </Translate>
          </p>
        </section>

        <section style={{ marginTop: "2.5rem" }} aria-labelledby="deeper-heading">
          <h2 id="deeper-heading">
            <Translate id="trust.deeper.heading" description="Section heading for deeper trust-center links.">
              Deeper trust evidence
            </Translate>
          </h2>
          <ul>
            <li>
              <Link to="/trust/center">
                <Translate id="trust.deeper.center" description="Link to the five-quadrant Trust Center.">
                  Trust Center — five-quadrant compliance / security / privacy / reliability / operations map
                </Translate>
              </Link>
            </li>
            <li>
              <Link to="/trust/overview">
                <Translate id="trust.deeper.overview" description="Link to the long-form trust overview page.">
                  Trust Overview — long-form single-page printable for procurement / DPO
                </Translate>
              </Link>
            </li>
            <li>
              <Link to="/trust/sub-processor-register">
                <Translate id="trust.deeper.subprocessors" description="Link to the live sub-processor register.">
                  Sub-processor register — live attestation, change log, customer notice policy
                </Translate>
              </Link>
            </li>
            <li>
              <Link to="/trust/incident-history">
                <Translate id="trust.deeper.incidents" description="Link to the incident history page.">
                  Incident history — public post-mortems and status page archive
                </Translate>
              </Link>
            </li>
            <li>
              <Link to="/legal/privacy">
                <Translate id="trust.deeper.privacy" description="Link to the privacy page (which carries the DPA pointer).">
                  Privacy & DPA — Common Paper template, GDPR Art. 28 / 32 coverage
                </Translate>
              </Link>
            </li>
          </ul>
        </section>

        <footer style={{ marginTop: "3rem", paddingTop: "1.5rem", borderTop: "1px solid var(--ifm-color-emphasis-200)", fontSize: "0.875rem", color: "var(--ifm-color-emphasis-700)" }}>
          <p style={{ margin: 0 }}>
            <Translate id="trust.contact" description="Security contact footer line on /trust.">
              Security questions, vulnerability reports, or auditor evidence requests:
            </Translate>{" "}
            <a href="mailto:security@humangr.com">security@humangr.com</a>{" "}
            (<Link to="/security/policy">
              <Translate id="trust.contact.vdp" description="Link to the vulnerability-disclosure policy.">
                vulnerability-disclosure policy
              </Translate>
            </Link>
            ).
          </p>
        </footer>
      </main>
    </Layout>
  );
}
