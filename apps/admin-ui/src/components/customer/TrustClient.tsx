// Customer-side Trust & compliance center — BYOK status, self-serve config,
// data residency, DSR/erasure, and compliance status.
//
// W8 (customer-dashboard build wave). Kit-only: every surface is a
// `@/components/ui/linear` primitive. Data-truth is honored strictly:
//   · BYOK STATUS is [live] — read from getOverview().byok.
//   · BYOK self-serve CONFIG is [not-wired → BE-8] — getByokConfig throws, so we
//     render a teaching Callout, NEVER a fabricated config.
//   · Region / residency is [not-wired] — teaching Callout only.
//   · DSR / erasure is [live] at the top-level /[locale]/dsr tree — we LINK to it,
//     we do not rebuild it here.
//   · Compliance status (SOC2 / ISO / DPA) is informational — a Callout.

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

  return (
    <div data-testid="trust-shell" className="lin-checklist">
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

      {/* ── DSR / erasure [live] — LINK to the top-level /[locale]/dsr tree. ── */}
      <Card
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
        <Link href={dsrHref} data-testid="trust-dsr-link" className="lin-btn lin-btn--primary">
          Request data export / erasure
        </Link>
      </Card>

      {/* ── Compliance status (informational Callout). ─────────────── */}
      <Card
        title="Compliance"
        meta="Certifications, attestations, and agreements."
      >
        <div data-testid="trust-compliance">
          <Callout tone="info">
            CoreLink runs on isolated, per-tenant encrypted storage. A signed Data
            Processing Agreement (DPA) is in force for every paid tenant. SOC 2 Type II
            and ISO 27001 programs are in progress; reach out for our current attestation
            package and the latest DPA version.
          </Callout>
        </div>
      </Card>
    </div>
  );
}

export function TrustClient(): React.ReactElement {
  return <TrustInner />;
}

export default TrustClient;
