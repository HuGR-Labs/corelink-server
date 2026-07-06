// Customer-side Settings — spend cap ($-ceiling), region, notification prefs,
// and the danger zone (GDPR self-erasure of the whole tenant).
//
// W8 (customer-dashboard build wave). Kit-only: every surface is a
// `@/components/ui/linear` primitive. Data-truth is honored strictly:
//   · $-ceiling is [not-wired → BE-7] — getDollarCeiling throws, so we render a
//     teaching Callout ONLY. NO fake number and NO dead probe: we never call the
//     throwing read just to catch it — we render the honest teaching state.
//   · Region + notification prefs are [not-wired] — teaching / coming Callouts.
//   · Delete account is [live] — deleteAccount() is wired; it is a GDPR
//     self-erasure, irreversible, so it sits behind a ConfirmDialog and fires a
//     Toast on success.

"use client";

import React from "react";
import { useAuth } from "@clerk/nextjs";
import { CustomerClient } from "@/lib/customer-client";
import {
  Button,
  Callout,
  Card,
  ConfirmDialog,
  HelpPopover,
  ToastProvider,
  useToast,
} from "@/components/ui/linear";

function SettingsInner(): React.ReactElement {
  const { getToken } = useAuth();
  const client = React.useMemo(() => new CustomerClient({ getToken }), [getToken]);
  const { toast } = useToast();

  const [confirmDelete, setConfirmDelete] = React.useState(false);

  async function onConfirmDelete(): Promise<void> {
    try {
      const r = await client.deleteAccount();
      toast({
        title: `Erasure requested (${r.request_id}). You'll be signed out shortly.`,
        tone: "success",
      });
    } catch {
      toast({ title: "Couldn't start account deletion", tone: "danger" });
    }
  }

  return (
    <div data-testid="settings-shell" className="lin-checklist">
      {/* ── $-ceiling (spend cap) [not-wired → BE-7]: teaching Callout only. ── */}
      <Card
        title="Monthly spend cap"
        meta="A hard limit on what your tenant can spend in a month."
        actions={
          <HelpPopover label="What is a spend cap ($-ceiling)?">
            A spend cap (or $-ceiling) is a hard monthly limit on your usage-based
            charges. Unlike a soft budget alert, hitting the cap stops further billable
            usage — you get a 429 (rate-limited) response, not a surprise invoice.
          </HelpPopover>
        }
      >
        <div data-testid="settings-dollar-ceiling">
          <Callout tone="info">
            Set a hard monthly spend cap — coming soon. Once available, at 100% of your
            cap you get a <strong>429</strong>, not a surprise bill. Until then, your tier
            is already hard-capped on requests and storage, so you can never be charged
            beyond your plan.
          </Callout>
        </div>
      </Card>

      {/* ── Region [not-wired]: teaching / coming Callout. ─────────── */}
      <Card
        title="Region"
        meta="Where your cached data is served from."
        actions={
          <HelpPopover label="What does region control?">
            Region controls where your cached data is stored and served. Choosing a region
            close to your CI reduces latency; a specific region can also satisfy data
            residency requirements.
          </HelpPopover>
        }
      >
        <div data-testid="settings-region">
          <Callout tone="info">
            Self-serve region selection is coming soon. To provision your tenant in a
            specific region today (including EU residency), see{" "}
            <strong>Trust &amp; compliance</strong> or contact us.
          </Callout>
        </div>
      </Card>

      {/* ── Notification preferences [not-wired]: coming Callout. ──── */}
      <Card title="Notifications" meta="How we alert you about usage and billing.">
        <div data-testid="settings-notifications">
          <Callout tone="info">
            Notification preferences — usage-threshold and billing alerts by email — are
            coming soon. In-app alerts fire at 70% and 100% of any cap today.
          </Callout>
        </div>
      </Card>

      {/* ── Danger zone: account self-delete [live]. ───────────────── */}
      <Card title="Danger zone" meta="Irreversible actions.">
        <div data-testid="settings-danger-zone">
          <Callout tone="danger">
            Deleting your account permanently erases this tenant and all of its data —
            cached artifacts, tokens, team members, and audit history. This is a GDPR
            self-erasure and <strong>cannot be undone</strong>. You may be asked to
            re-authenticate with MFA to continue.
          </Callout>
          <Button
            variant="danger"
            data-testid="settings-delete-account"
            onClick={() => setConfirmDelete(true)}
          >
            Delete account &amp; all data
          </Button>
        </div>
      </Card>

      <ConfirmDialog
        open={confirmDelete}
        title="Delete account & erase all data?"
        danger
        confirmLabel="Delete everything"
        body={
          <Callout tone="danger">
            This permanently erases <strong>this entire tenant</strong> — every cached
            artifact, token, team member, and audit record. It is a GDPR self-erasure and
            <strong> cannot be undone</strong>. MFA re-authentication may be required to
            complete it.
          </Callout>
        }
        onClose={() => setConfirmDelete(false)}
        onConfirm={onConfirmDelete}
      />
    </div>
  );
}

export function SettingsClient(): React.ReactElement {
  return (
    <ToastProvider>
      <SettingsInner />
    </ToastProvider>
  );
}

export default SettingsClient;
