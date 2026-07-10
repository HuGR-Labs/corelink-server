// Customer-side Settings — a real settings surface: a read-only Account
// summary (identity, sourced from the ambient Clerk session — no client
// method), the backend-gated controls shown as honest "coming soon" Callouts
// (spend cap $-ceiling → BE-7, region/residency → BE, notifications → BE), and
// the danger zone (GDPR self-erasure of the whole tenant).
//
// W8 (customer-dashboard build wave). Kit-only: every surface is a
// `@/components/ui/linear` primitive. Data-truth is honored strictly:
//   · Account identity comes from Clerk's already-mounted session context
//     (useUser / useOrganization) — no new client method, no fabricated field;
//     each row renders ONLY when its value is present.
//   · $-ceiling is [not-wired → BE-7] — getDollarCeiling throws, so we render a
//     teaching Callout ONLY. NO fake number and NO dead probe: we never call the
//     throwing read just to catch it — we render the honest teaching state.
//   · Region + notification prefs are [not-wired] — honest "coming soon" Callouts.
//   · Delete account is [live] — deleteAccount() is wired; it is a GDPR
//     self-erasure, irreversible, so it sits behind a ConfirmDialog and fires a
//     Toast on success.
//
// Vertical rhythm follows the frozen Linear contract: `.lin-mt` (16px) between
// related cards, `.lin-mt-lg` (24px) between groups. The legacy `.lin-checklist`
// vstack (4px) was removed — it is for checklist rows only, never a card stack.

"use client";

import React from "react";
import { useOrganization, useUser } from "@clerk/nextjs";
import { useCustomerClient } from "@/lib/use-customer-client";
import {
  Button,
  Callout,
  Card,
  ConfirmDialog,
  HelpPopover,
  Skeleton,
  ToastProvider,
  useToast,
} from "@/components/ui/linear";

/** "org:admin" → "Admin"; null/blank → null (so we never render an empty role). */
function formatRole(role?: string | null): string | null {
  if (!role) return null;
  const bare = role.replace(/^org:/, "").replace(/[_-]+/g, " ").trim();
  if (bare === "") return null;
  return bare.charAt(0).toUpperCase() + bare.slice(1);
}

/** A single read-only identity row — renders nothing when the value is absent. */
function AccountRow({ label, value }: { label: string; value?: string | null }): React.ReactElement | null {
  if (!value) return null;
  return (
    <div className="flex items-baseline justify-between gap-4">
      <span className="text-[13px] lin-t3">{label}</span>
      <span className="text-[14px] lin-t1 truncate text-right">{value}</span>
    </div>
  );
}

function AccountCard(): React.ReactElement {
  const { isLoaded: userLoaded, user } = useUser();
  const { isLoaded: orgLoaded, organization, membership } = useOrganization();

  const email = user?.primaryEmailAddress?.emailAddress ?? null;
  const displayName = user?.fullName ?? null;
  const orgName = organization?.name ?? null;
  const role = formatRole(membership?.role);

  const loaded = userLoaded && orgLoaded;
  const hasAny = Boolean(orgName || email || displayName || role);

  return (
    <Card
      title="Account"
      meta="Your identity and workspace on CoreLink."
      actions={
        <HelpPopover label="What is a tenant / workspace?">
          Your tenant (or workspace) is your isolated CoreLink account — its own
          cache, tokens, team, and billing. All of your data is scoped to it and
          is never shared with other tenants.
        </HelpPopover>
      }
    >
      <div data-testid="settings-account">
        {!loaded ? (
          <Skeleton rows={3} />
        ) : hasAny ? (
          <div className="flex flex-col gap-3">
            <AccountRow label="Workspace" value={orgName} />
            <AccountRow label="Signed in as" value={displayName} />
            <AccountRow label="Email" value={email} />
            <AccountRow label="Your role" value={role} />
          </div>
        ) : (
          <Callout tone="info">
            We couldn&apos;t read your profile from this session. Try reloading the
            page.
          </Callout>
        )}
      </div>
    </Card>
  );
}

function SettingsInner(): React.ReactElement {
  const client = useCustomerClient();
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
    <div data-testid="settings-shell">
      {/* ── Account: read-only identity from the Clerk session. ──────────── */}
      <AccountCard />

      {/* ── $-ceiling (spend cap) [not-wired → BE-7]: teaching Callout only. ── */}
      <Card
        title="Monthly spend cap"
        meta="A hard limit on what your tenant can spend in a month."
        className="lin-mt-lg"
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

      {/* ── Region / data residency [not-wired]: honest coming / contact. ── */}
      <Card
        title="Region & data residency"
        meta="Where your cached data is stored and served from."
        className="lin-mt"
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

      {/* ── Notification preferences [not-wired]: honest coming. ─────────── */}
      <Card
        title="Notifications"
        meta="How we alert you about usage and billing."
        className="lin-mt"
        actions={
          <HelpPopover label="Which alerts fire today?">
            Usage-threshold alerts warn you as you approach a cap; billing alerts cover
            invoices and payment issues. Today these surface in-app; email delivery is on
            the way.
          </HelpPopover>
        }
      >
        <div data-testid="settings-notifications">
          <Callout tone="info">
            Notification preferences — usage-threshold and billing alerts by email — are
            coming soon. In-app alerts fire at 70% and 100% of any cap today.
          </Callout>
        </div>
      </Card>

      {/* ── Danger zone: account self-delete [live] — the one real control. ── */}
      <Card title="Danger zone" meta="Irreversible actions." className="lin-mt-lg">
        <div data-testid="settings-danger-zone">
          <Callout tone="danger">
            Deleting your account permanently erases this tenant and all of its data —
            cached artifacts, tokens, team members, and audit history. This is a GDPR
            self-erasure and <strong>cannot be undone</strong>. You may be asked to
            re-authenticate with MFA to continue.
          </Callout>
          <div className="lin-mt">
            <Button
              variant="danger"
              data-testid="settings-delete-account"
              onClick={() => setConfirmDelete(true)}
            >
              Delete account &amp; all data
            </Button>
          </div>
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
