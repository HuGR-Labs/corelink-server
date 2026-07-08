/**
 * Incident history — public postmortem corpus at `/trust/incident-history`.
 *
 * Wave-29 stream-8 deliverable. Pre-GA, this page is a STRUCTURED PLACEHOLDER
 * that publishes the two redacted internal drill postmortems we already have
 * (cold-restore drill, active failover drill). Real customer-impacting
 * incidents will be wired in T+30d post-GA via the postmortem corpus
 * generator (planned ADR — not in scope for this wave).
 *
 * Honest framing: we say "no customer-impacting incidents to date" because
 * we are pre-GA. We do NOT pad the page with synthetic incidents.
 *
 * Source files (eventual auto-pull target post-GA):
 *   - `specs/_compliance/drill-evidence/` — drill postmortems
 *   - `docs/postmortems/` — customer-incident postmortems (not yet existing)
 */

import Layout from "@theme/Layout";
import Link from "@docusaurus/Link";
import Translate from "@docusaurus/Translate";
import type { ReactElement } from "react";

type IncidentKind = "drill" | "customer-incident";
type IncidentSeverity = "internal-drill" | "SEV-1" | "SEV-2" | "SEV-3";

interface IncidentEntry {
  readonly date: string;
  readonly kind: IncidentKind;
  readonly severity: IncidentSeverity;
  readonly title: string;
  readonly summary: string;
  readonly postmortemHref: string;
}

const ENTRIES: readonly IncidentEntry[] = [
  {
    date: "2026-05-04",
    kind: "drill",
    severity: "internal-drill",
    title: "Active failover drill — primary region BR → US",
    summary:
      "Quarterly active failover drill. Simulated complete loss of BR primary region; traffic re-pinned to US within 7m12s (target ≤ 10m). No customer impact (drill executed in shadow plane).",
    postmortemHref:
      "https://github.com/HumanGuardrail/corelink-server/blob/main/specs/_compliance/drill-evidence/2026-05-04-active-failover-drill.md",
  },
  {
    date: "2026-04-15",
    kind: "drill",
    severity: "internal-drill",
    title: "Cold-restore drill — full-region rebuild from R2 + audit chain",
    summary:
      "Quarterly cold-restore drill. Restored full-region cache index from R2 cold storage + audit chain inclusion proofs. Recovery time 42m18s (target ≤ 60m); recovery point 0 (audit-chain replay).",
    postmortemHref:
      "https://github.com/HumanGuardrail/corelink-server/blob/main/specs/_compliance/drill-evidence/2026-04-15-cold-restore-drill.md",
  },
];

const SEVERITY_COLORS: Record<
  IncidentSeverity,
  { bg: string; fg: string; border: string }
> = {
  "internal-drill": { bg: "#e0e7ff", fg: "#1e3a8a", border: "#3730a3" },
  "SEV-1": { bg: "#fee2e2", fg: "#7f1d1d", border: "#b91c1c" },
  "SEV-2": { bg: "#fef3c7", fg: "#7c2d12", border: "#b45309" },
  "SEV-3": { bg: "#dcfce7", fg: "#14532d", border: "#16a34a" },
};

function SeverityBadge({ severity }: { readonly severity: IncidentSeverity }): ReactElement {
  const { bg, fg, border } = SEVERITY_COLORS[severity];
  return (
    <span
      style={{
        display: "inline-block",
        padding: "0.125rem 0.5rem",
        background: bg,
        color: fg,
        border: `1px solid ${border}`,
        borderRadius: "4px",
        fontSize: "0.75rem",
        fontWeight: 700,
        letterSpacing: "0.02em",
        textTransform: "uppercase",
      }}
    >
      {severity}
    </span>
  );
}

function EntryCard({ entry }: { readonly entry: IncidentEntry }): ReactElement {
  return (
    <article
      style={{
        padding: "1.25rem",
        border: "1px solid var(--ifm-color-emphasis-300)",
        borderRadius: "8px",
        marginBottom: "1rem",
        background: "var(--ifm-background-surface-color)",
      }}
    >
      <div
        style={{
          display: "flex",
          gap: "0.75rem",
          alignItems: "center",
          marginBottom: "0.5rem",
          flexWrap: "wrap",
        }}
      >
        <code style={{ fontSize: "0.9rem" }}>{entry.date}</code>
        <SeverityBadge severity={entry.severity} />
        <span className="u-muted" style={{ fontSize: "0.85rem" }}>
          {entry.kind === "drill" ? "Internal drill (no customer impact)" : "Customer-impacting"}
        </span>
      </div>
      <h3 style={{ marginTop: 0, marginBottom: "0.25rem" }}>{entry.title}</h3>
      <p style={{ marginBottom: "0.5rem" }}>{entry.summary}</p>
      <a href={entry.postmortemHref} rel="noopener noreferrer" target="_blank">
        Read full postmortem &rarr;
      </a>
    </article>
  );
}

export default function IncidentHistory(): ReactElement {
  return (
    <Layout
      title="Incident history"
      description="CoreLink incident history — public postmortem corpus. Pre-GA: drill postmortems only (no customer-impacting incidents to date). Real-incident wire T+30d post-GA."
    >
      <main className="container margin-top--lg margin-bottom--xl" style={{ maxWidth: "1100px" }}>
        <header>
          <h1>
            <Translate
              id="trust.incident.title"
              description="Incident history page title"
            >
              Incident history
            </Translate>
          </h1>
          <p className="u-muted" style={{ fontSize: "1.05rem" }}>
            <Translate
              id="trust.incident.subhead"
              description="Incident history subheadline"
            >
              Public postmortems for all customer-impacting incidents and
              internal drills. We commit to publishing a redacted postmortem
              within 14 days of resolution for any incident at SEV-2 or
              higher.
            </Translate>
          </p>
        </header>

        <aside
          role="note"
          style={{
            marginTop: "1.5rem",
            padding: "1rem 1.25rem",
            border: "2px solid #b45309",
            background: "#fef3c7",
            color: "#7c2d12",
            borderRadius: "8px",
          }}
        >
          <strong>
            <Translate
              id="trust.incident.preGA.title"
              description="Pre-GA banner title"
            >
              Pre-GA disclosure
            </Translate>
          </strong>
          <p style={{ margin: "0.5rem 0 0", color: "#7c2d12" }}>
            <Translate
              id="trust.incident.preGA.body"
              description="Pre-GA banner body"
            >
              CoreLink has had no customer-impacting incidents to date — the
              service is pre-GA and operating in beta. The entries below are
              internal drill postmortems (active failover + cold restore)
              already executed against the pre-production fleet. The
              auto-pull from the live postmortem corpus wires in T+30d
              post-GA; this page is otherwise honest by construction (no
              synthetic incidents are listed).
            </Translate>
          </p>
        </aside>

        <section aria-labelledby="entries-heading" style={{ marginTop: "2rem" }}>
          <h2 id="entries-heading">
            <Translate
              id="trust.incident.entries.title"
              description="Entries section title"
            >
              Recent entries
            </Translate>
          </h2>
          {ENTRIES.map((entry) => (
            <EntryCard key={entry.date} entry={entry} />
          ))}
        </section>

        <section style={{ marginTop: "2rem" }}>
          <h2>
            <Translate
              id="trust.incident.policy.title"
              description="Policy section title"
            >
              Publication policy
            </Translate>
          </h2>
          <ul>
            <li>
              <strong>SEV-1 + SEV-2</strong>:{" "}
              <Translate
                id="trust.incident.policy.sev12"
                description="SEV-1/2 publication policy"
              >
                Redacted postmortem published within 14 days of resolution.
                Includes timeline, contributing factors, customer impact,
                remediation, and follow-up action items with owners + dates.
              </Translate>
            </li>
            <li>
              <strong>SEV-3</strong>:{" "}
              <Translate
                id="trust.incident.policy.sev3"
                description="SEV-3 publication policy"
              >
                Summary published on the status page; full postmortem on
                request.
              </Translate>
            </li>
            <li>
              <strong>Internal drills</strong>:{" "}
              <Translate
                id="trust.incident.policy.drill"
                description="Drill publication policy"
              >
                Published on a best-effort basis after redaction for
                operational-security reasons.
              </Translate>
            </li>
          </ul>
          <p>
            <Translate
              id="trust.incident.policy.notification"
              description="Notification policy"
            >
              Customer notification commitments are detailed in
            </Translate>{" "}
            <Link to="/trust/incident-response">
              <Translate
                id="trust.incident.policy.ir.linktext"
                description="Incident response link text"
              >
                Incident response
              </Translate>
            </Link>{" "}
            <Translate
              id="trust.incident.policy.notification.suffix"
              description="Notification policy suffix"
            >
              (72-hour breach-notification commitment under DPA §8).
            </Translate>
          </p>
        </section>

        <section style={{ marginTop: "2rem" }}>
          <h2>
            <Translate
              id="trust.incident.related.title"
              description="Related section title"
            >
              Related
            </Translate>
          </h2>
          <ul>
            <li>
              <Link to="/trust">Trust Center</Link>
            </li>
            <li>
              <Link to="/trust/incident-response">Incident response process</Link>
            </li>
            <li>
              <a
                href="https://status.corelink.humangr.com"
                rel="noopener noreferrer"
                target="_blank"
              >
                status.corelink.humangr.com
              </a>
            </li>
          </ul>
        </section>
      </main>
    </Layout>
  );
}
