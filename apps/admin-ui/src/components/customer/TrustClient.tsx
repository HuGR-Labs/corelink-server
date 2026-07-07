// Customer-side Trust & compliance center — BYOK status, self-serve config,
// data residency, audit & evidence, DSR/erasure, DPA/sub-processors, and
// compliance posture.
//
// W8 (customer-dashboard build wave). Kit-only: every surface is a
// `@/components/ui/linear` primitive. Data-truth is honored strictly:
//   · BYOK STATUS is [live] — read from getOverview().byok.
//   · BYOK self-serve CONFIG is [not-wired → BE-8] — getByokConfig throws, so we
//     render a teaching Callout, NEVER a fabricated config.
//   · Region / residency is [not-wired] — teaching Callout only.
//   · Audit & evidence is [live] at /[locale]/customer/audit — we LINK to it,
//     we do NOT fetch the audit chain here.
//   · DSR / erasure is [live] at the top-level /[locale]/dsr tree — we LINK to it,
//     we do not rebuild it here.
//   · DPA / sub-processors are [live] public pages — we LINK to them.
//   · Compliance posture (SOC2 / ISO / DPA) is presented honestly — SOC 2 / ISO
//     are shown as "in progress" (warn), never claimed as certified.
//
// Vertical rhythm: cards are stacked with `.lin-mt-lg` (24px between groups) —
// NEVER `.lin-checklist` (4px, checklist rows only) as a card vstack.

"use client";

import React from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { useAuth } from "@clerk/nextjs";
import { CustomerClient } from "@/lib/customer-client";
import type { CustomerOverview } from "@/lib/customer-types";
import {
  Badge,
  Button,
  Callout,
  Card,
  EmptyState,
  HelpPopover,
  InlineError,
  Skeleton,
} from "@/components/ui/linear";

/** Map the [live] BYOK status enum to a human label + a Badge tone. */
function byokStatusView(status: CustomerOverview["byok"]["status"]): {
  label: string;
  tone: "neutral" | "success" | "warn";
} {
  switch (status) {
    case "active":
      return { label: "Active — your key is protecting this tenant", tone: "success" };
    case "rotation_pending":
      return { label: "Rotation pending", tone: "warn" };
    case "none":
    default:
      return { label: "Not configured — CoreLink-managed keys in use", tone: "neutral" };
  }
}

function TrustInner(): React.ReactElement {
  const params = useParams();
  const localeParam = params?.locale;
  const locale = Array.isArray(localeParam) ? localeParam[0] : localeParam ?? "en";
  const dsrHref = `/${locale}/dsr`;

  const { getToken } = useAuth();
  const client = React.useMemo(() => new CustomerClient({ getToken }), [getToken]);

  const [byok, setByok] = React.useState<CustomerOverview["byok"] | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [error, setError] = React.useState<unknown | null>(null);

  const reload = React.useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const overview = await client.getOverview();
      setByok(overview.byok);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  }, [client]);

  React.useEffect(() => {
    void reload();
  }, [reload]);

  const statusView = byok ? byokStatusView(byok.status) : null;

  const auditHref = `/${locale}/customer/audit`;
  const subProcessorsHref = `/${locale}/privacy/sub-processors`;
  const termsHref = `/${locale}/legal/terms`;
  const privacyHref = `/${locale}/privacy`;

  return (
    // Card stack — rhythm via `.lin-mt-lg` (24px) on every card after the first;
    // `.lin-card` has zero margin by design.
    <div data-testid="trust-shell">
      {/* ── BYOK status [live] ─────────────────────────────────── */}
      <Card
        title="Encryption — bring your own key (BYOK)"
        meta="Who holds the key that encrypts your cached data."
        actions={
          <HelpPopover label="What is BYOK / a CMK?">
            BYOK (bring-your-own-key) lets you encrypt your cached data with a key you
            control in your own cloud KMS — a customer master key, or CMK — so CoreLink
            never holds the raw key. Combined with a kill-switch, disabling the CMK makes
            your data unreadable instantly. Without BYOK, your data is encrypted with
            CoreLink-managed keys.
          </HelpPopover>
        }
      >
        <div data-testid="trust-byok-status">
          {loading ? (
            <div aria-busy="true" aria-label="Loading BYOK status">
              <Skeleton rows={2} />
            </div>
          ) : error != null ? (
            <InlineError error={error} onRetry={() => void reload()} />
          ) : statusView && byok ? (
            <p>
              Status:{" "}
              <Badge tone={statusView.tone} dot>
                {statusView.label}
              </Badge>
              {byok.cmk_id ? (
                <>
                  {" "}
                  · CMK <code>{byok.cmk_id}</code>
                </>
              ) : null}
              {byok.last_rotated_at ? (
                <>
                  {" "}
                  · last rotated {byok.last_rotated_at.slice(0, 10)}
                </>
              ) : null}
            </p>
          ) : (
            <EmptyState
              title="No customer-managed key"
              body="Your data is encrypted with CoreLink-managed keys."
            />
          )}
        </div>

        {/* ── Configure BYOK [not-wired → BE-8]: teaching Callout only. ── */}
        <div data-testid="trust-byok-config">
          <Callout tone="info">
            Bring-your-own-key with kill-switch — pick your KMS (AWS, GCP, Azure, or
            Vault), rotate on your schedule, and disable the key to make your data
            unreadable instantly. Available on <strong>Max</strong> and{" "}
            <strong>Enterprise</strong> plans.{" "}
            <a href="mailto:support@humangr.com?subject=Enable%20BYOK">
              Contact us to enable
            </a>
            .
          </Callout>
        </div>
      </Card>

      {/* ── Region / residency [not-wired]: teaching Callout only. ── */}
      <Card
        className="lin-mt-lg"
        title="Data residency"
        meta="Where your cached data physically lives."
        actions={
          <HelpPopover label="What is data residency?">
            Data residency (or residency region) is the physical location where your
            cached data is stored. Regulated workloads often require data to stay within
            a specific jurisdiction — for example, keeping EU customer data inside the EU.
          </HelpPopover>
        }
      >
        <div data-testid="trust-region">
          <Callout tone="info">
            EU data residency is available for regulated workloads — keep your cached data
            within the EU. Region selection is coming to self-serve;{" "}
            <a href="mailto:support@humangr.com?subject=EU%20data%20residency">
              contact us
            </a>{" "}
            to provision an EU-resident tenant today.
          </Callout>
        </div>
      </Card>

      {/* ── Audit & evidence [live] — LINK to /[locale]/customer/audit. ─── */}
      <Card
        className="lin-mt-lg"
        title="Audit & evidence"
        meta="Your tamper-evident audit log and signed evidence exports."
        actions={
          <HelpPopover label="What is a tamper-evident audit log?">
            Every privileged action on your tenant — sign-ins, token lifecycle,
            encryption-key rotations, team changes — is written to a hash-chained
            audit log. Because each entry commits to the one before it, any later
            edit or deletion breaks the chain and is detectable — that is what makes
            it tamper-evident. You can review the chain and export a signed evidence
            bundle for auditors.
          </HelpPopover>
        }
      >
        <div data-testid="trust-audit">
          <p>
            Review the complete, hash-chained record of privileged actions on your
            tenant, and export a signed evidence bundle for auditors and compliance
            reviews.
          </p>
          <Link
            href={auditHref}
            data-testid="trust-audit-link"
            className="lin-mt lin-btn lin-btn--primary"
          >
            View audit log
          </Link>
        </div>
      </Card>

      {/* ── DSR / erasure [live] — LINK to the top-level /[locale]/dsr tree. ── */}
      <Card
        className="lin-mt-lg"
        title="Data-subject requests & erasure"
        meta="Export or permanently erase the personal data held for your tenant."
        actions={
          <HelpPopover label="What is a DSR?">
            A data-subject request (DSR) is a GDPR/CCPA right to access, export, or erase
            personal data held about you. CoreLink processes these against your whole
            tenant and issues a signed attestation on completion.
          </HelpPopover>
        }
      >
        <p>
          Request a full export of your data, or a permanent erasure. Each request is
          tracked and produces a signed completion attestation.
        </p>
        <Link
          href={dsrHref}
          data-testid="trust-dsr-link"
          className="lin-mt lin-btn lin-btn--primary"
        >
          Request data export / erasure
        </Link>
      </Card>

      {/* ── DPA & sub-processors [live public pages] — LINK out. ────── */}
      <Card
        className="lin-mt-lg"
        title="Data Processing Agreement & sub-processors"
        meta="The contract we process your data under, and the third parties in the data path."
        actions={
          <HelpPopover label="What is a sub-processor?">
            A sub-processor is a third party (for example an infrastructure or email
            provider) that CoreLink uses to process your data on your behalf. The Data
            Processing Agreement (DPA) is the contract governing that processing under
            GDPR/CCPA. We publish the full, current list so you always know exactly who
            is in your data path.
          </HelpPopover>
        }
      >
        <div data-testid="trust-dpa">
          <p>
            Review the terms your data is processed under, the current list of
            sub-processors in the data path, and our privacy policy.
          </p>
          <div className="lin-mt lin-card__actions">
            <Button href={subProcessorsHref} variant="ghost" size="sm">
              Sub-processors
            </Button>
            <Button href={termsHref} variant="ghost" size="sm">
              Terms of service
            </Button>
            <Button href={privacyHref} variant="ghost" size="sm">
              Privacy policy
            </Button>
          </div>
        </div>
      </Card>

      {/* ── Compliance posture — honest status, Badge tone=neutral/warn. ── */}
      <Card
        className="lin-mt-lg"
        title="Compliance posture"
        meta="Where our certifications and attestations stand today."
      >
        <div data-testid="trust-compliance">
          <p>
            Per-tenant encrypted, isolated storage{" "}
            <Badge tone="neutral" dot>
              In force
            </Badge>
          </p>
          <p>
            Signed Data Processing Agreement (DPA){" "}
            <Badge tone="neutral" dot>
              Available for every paid tenant
            </Badge>
          </p>
          <p>
            SOC 2 Type II{" "}
            <Badge tone="warn" dot>
              In progress
            </Badge>
          </p>
          <p>
            ISO 27001{" "}
            <Badge tone="warn" dot>
              In progress
            </Badge>
          </p>
          <p className="lin-mt">
            SOC 2 and ISO are not yet certified — programs are underway. Reach out for
            our current attestation package and the latest DPA version:{" "}
            <a href="mailto:support@humangr.com?subject=Compliance%20package">
              support@humangr.com
            </a>
            .
          </p>
        </div>
      </Card>
    </div>
  );
}

export function TrustClient(): React.ReactElement {
  return <TrustInner />;
}

export default TrustClient;
