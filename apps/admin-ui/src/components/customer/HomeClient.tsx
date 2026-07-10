// Customer-side "Home / onboarding checklist" (W1).
//
// DATA-TRUTH (mirrors customer_d1.rs / customer-types.ts):
//   plan .......................... [live]  → snapshot Badge
//   usage.cas_bytes / quota_bytes . [live]  → Storage gauge
//   byok.status ................... [live]  → BYOK Badge
//   recent_activity ............... [stub]  → prod returns [] (BE-3) →
//                                             teaching EmptyState, never a fake row
//   PAT existence (listKeys) ...... [live]  → derives checklist step ② "create a token"
//   DPA acceptance ................ no client signal → step rendered pending with a
//                                   teaching hint (NEVER faked complete)
//   connect-a-tool ................ no client signal → step pending + link (honest)
//   first cache hit ............... no metering signal today (BE-1/BE-2) → step pending
//                                   + teaching hint (never fabricated as done)
//   hit_rate / $-saved ............ [live]  (BE-2, from getUsage — best-effort) → the
//                                   ROI hero. hit_rate null / usage unavailable = cold
//                                   start → teaching EmptyState, NEVER a fabricated %.
//
// The #1 rule for this screen: derive "done" ONLY from a real available signal.
// Every other step renders pending with a teaching hint — we never fake a green
// check, and we never surface a fabricated metric.

"use client";

import React from "react";
import { useCustomerClient } from "@/lib/use-customer-client";
import type { CustomerOverview, CustomerPat, CustomerUsage } from "@/lib/customer-types";
import {
  Badge,
  Button,
  Callout,
  Card,
  Checklist,
  EmptyState,
  Gauge,
  HelpPopover,
  InlineError,
  Skeleton,
  Stat,
} from "@/components/ui/linear";

export interface HomeClientProps {
  /** Locale prefix for in-dashboard links (e.g. `/en/customer/keys`). */
  locale: string;
}

function gib(bytes: number): number {
  return Math.round((bytes / 1024 ** 3) * 100) / 100;
}

// Modeled dollar savings from cents → "$X" (whole dollars once ≥ $10, else 2dp
// so an early-days figure isn't rounded to "$0"). Kept local, like `gib`.
function dollarsSaved(cents: number): string {
  const d = cents / 100;
  return d >= 10
    ? `$${Math.round(d).toLocaleString()}`
    : `$${(Math.round(d * 100) / 100).toLocaleString()}`;
}

function isActivePat(p: CustomerPat): boolean {
  return p.revoked_at == null;
}

/** Human label for a cache tier id — no raw enum reaches the screen. */
const PLAN_LABELS: Record<CustomerOverview["plan"], string> = {
  free: "Free",
  solo: "Solo",
  starter: "Starter",
  team: "Team",
  pro: "Pro",
  max: "Max",
  enterprise: "Enterprise",
};

/** Human label + Badge tone for a subscription status (mirrors BillingClient). */
const BILLING_STATUS_META: Record<
  CustomerOverview["billing"]["status"],
  { label: string; tone: "neutral" | "success" | "warn" | "danger" }
> = {
  trialing: { label: "Trialing", tone: "success" },
  active: { label: "Active", tone: "success" },
  past_due: { label: "Past due", tone: "danger" },
  canceled: { label: "Canceled", tone: "warn" },
  inactive: { label: "No subscription", tone: "neutral" },
};

function money(cents: number, currency: CustomerOverview["billing"]["currency"]): string {
  const sign = currency === "usd" ? "$" : currency === "eur" ? "€" : "R$";
  return sign + (cents / 100).toFixed(2);
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso.slice(0, 10);
  return d.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

const BYOK_TONE: Record<
  CustomerOverview["byok"]["status"],
  "neutral" | "success" | "warn"
> = {
  none: "neutral",
  active: "success",
  rotation_pending: "warn",
};

const BYOK_LABEL: Record<CustomerOverview["byok"]["status"], string> = {
  none: "Not configured",
  active: "Active",
  rotation_pending: "Rotation pending",
};

export function HomeClient({ locale }: HomeClientProps): React.ReactElement {
  const client = useCustomerClient();
  const [overview, setOverview] = React.useState<CustomerOverview | null>(null);
  const [pats, setPats] = React.useState<CustomerPat[] | null>(null);
  // Usage powers the ROI hero (hit-rate + $ saved). It is a best-effort side
  // read — a failure degrades the hero to its cold-start teaching state, never
  // blocks Home and never fabricates a metric.
  const [usage, setUsage] = React.useState<CustomerUsage | null>(null);
  const [err, setErr] = React.useState<unknown>(null);
  const [reloadKey, setReloadKey] = React.useState(0);

  React.useEffect(() => {
    let alive = true;
    setOverview(null);
    setPats(null);
    setUsage(null);
    setErr(null);
    // Overview is the load-bearing fetch; listKeys is a cheap [live] read that
    // lets us honestly mark step ② "create a token" done when a PAT exists, and
    // getUsage feeds the ROI hero. Both side reads degrade to null on failure —
    // a keys/usage failure must NOT block Home.
    Promise.all([
      client.getOverview(),
      client.listKeys().then(
        (k) => k.pats,
        () => null,
      ),
      client.getUsage().then(
        (u) => u,
        () => null,
      ),
    ])
      .then(([o, p, u]) => {
        if (!alive) return;
        setOverview(o);
        setPats(p);
        setUsage(u);
      })
      .catch((e: unknown) => alive && setErr(e));
    return () => {
      alive = false;
    };
  }, [client, reloadKey]);

  if (err) {
    return (
      <div data-testid="home-error">
        <InlineError error={err} onRetry={() => setReloadKey((k) => k + 1)} />
      </div>
    );
  }

  if (!overview) {
    return (
      <div data-testid="home-loading" aria-busy="true" aria-label="Loading your dashboard">
        <Card title="Get set up">
          <Skeleton rows={4} />
        </Card>
        <Card title="Your tenant at a glance" className="lin-mt-lg">
          <Skeleton rows={3} />
        </Card>
      </div>
    );
  }

  const base = `/${locale}/customer`;
  const hasActivePat = (pats ?? []).some(isActivePat);
  const storagePct =
    overview.usage.quota_bytes > 0
      ? overview.usage.cas_bytes / overview.usage.quota_bytes
      : 0;
  const storagePctLabel = `${Math.round(storagePct * 100)}%`;
  const activity = overview.recent_activity;

  // Billing snapshot — all [live] from getOverview().billing. An upcoming
  // invoice only exists for a billing subscription; for inactive/canceled
  // tenants there is no next invoice, so we show an honest note rather than a
  // fabricated $0.00 line (data-honesty is locked).
  const billing = overview.billing;
  const billingMeta = BILLING_STATUS_META[billing.status];
  const invoiceDate = new Date(billing.next_invoice_at);
  const hasUpcomingInvoice =
    (billing.status === "active" ||
      billing.status === "trialing" ||
      billing.status === "past_due") &&
    !Number.isNaN(invoiceDate.getTime());

  // Checklist — "done" derived ONLY from real signals; everything else is an
  // honest pending step with a teaching hint. We NEVER fake a completed step.
  const checklistItems = [
    {
      // ① DPA — no client-side acceptance signal exists yet. Honest pending +
      // teaching hint; never marked done from the dashboard.
      id: "dpa",
      done: false,
      label: (
        <>
          Accept the Data Processing Agreement{" "}
          <HelpPopover label="What is the DPA?">
            The Data Processing Agreement (DPA) is the contract that governs how
            CoreLink processes your data as a GDPR processor. It&rsquo;s a hard gate:
            the cache API returns <code>403 dpa_required</code> until it&rsquo;s
            accepted.
          </HelpPopover>
        </>
      ),
    },
    {
      // ② Create + copy a token — DERIVED from a live signal (an active PAT
      // exists). If listKeys failed we degrade to pending, never guess.
      id: "token",
      done: hasActivePat,
      label: hasActivePat
        ? "Create and copy an access token"
        : "Create and copy your first access token",
      cta: hasActivePat ? undefined : (
        <Button href={`${base}/keys`} variant="ghost" size="sm">
          Create a token
        </Button>
      ),
    },
    {
      // ③ Connect a tool — no per-surface connection signal today. Honest
      // pending + a link to the Connect screen.
      id: "connect",
      done: false,
      label: (
        <>
          Point a build tool at your cache{" "}
          <HelpPopover label="What does connecting do?">
            Connecting drops a one-line config into your build tool (Bazel,
            Turborepo, sccache, npm, pip) so it reads and writes artifacts from
            your CoreLink cache instead of rebuilding from scratch.
          </HelpPopover>
        </>
      ),
      cta: (
        <Button href={`${base}/connect`} variant="ghost" size="sm">
          Connect a tool
        </Button>
      ),
    },
    {
      // ④ First cache hit — the activation event. No metering signal today
      // (BE-1/BE-2), so it stays honestly pending with a teaching hint. We do
      // NOT fabricate completion from a zero.
      id: "first-hit",
      done: false,
      label: (
        <>
          See your first cache hit{" "}
          <HelpPopover label="What is a cache hit?">
            A cache hit is when a build asks for an artifact and CoreLink already
            has it — so it&rsquo;s served in milliseconds instead of rebuilt. Your
            first hit is the activation moment: it means the cache is working.
          </HelpPopover>
        </>
      ),
    },
  ];

  return (
    <div data-testid="home-shell">
      {/* Onboarding funnel — DPA → token → connect → first hit. */}
      <Card title="Get set up" meta="Four steps to your first faster build">
        <div data-testid="home-checklist">
          <Checklist items={checklistItems} />
        </div>
      </Card>

      {/* Snapshot — plan / storage / BYOK, all [live] from getOverview.
          Deep-links out to the full usage breakdown. */}
      <Card
        title="Your tenant at a glance"
        meta={overview.tenant_name}
        className="lin-mt-lg"
        actions={
          <Button href={`${base}/usage`} variant="ghost" size="sm">
            View usage
          </Button>
        }
      >
        <div data-testid="home-snapshot">
          <div data-testid="home-plan">
            <span className="lin-card__meta">Plan</span>{" "}
            <span data-testid="overview-plan">
              <Badge tone="neutral">{PLAN_LABELS[overview.plan]}</Badge>
            </span>
          </div>

          <div data-testid="home-storage-gauge" className="lin-mt">
            <Gauge
              label="Storage"
              value={gib(overview.usage.cas_bytes)}
              max={gib(overview.usage.quota_bytes)}
              unit="GiB"
              hint={`${storagePctLabel} of your storage cap — the content-addressed store (CAS) dedups every build artifact by its hash.`}
            />
            <span data-testid="overview-cas-bytes" hidden>
              {gib(overview.usage.cas_bytes)} GiB
            </span>
          </div>

          <div data-testid="home-byok" className="lin-mt">
            <span className="lin-card__meta">
              BYOK{" "}
              <HelpPopover label="What is BYOK?">
                BYOK (Bring Your Own Key) lets you encrypt your cached artifacts
                with a key from your own KMS (AWS, GCP, Azure, or Vault), so you
                hold the kill-switch. Status here is read-only; self-serve config
                arrives with the BYOK backend.
              </HelpPopover>
            </span>{" "}
            <span data-testid="overview-byok-status">
              <Badge tone={BYOK_TONE[overview.byok.status]} dot>
                {BYOK_LABEL[overview.byok.status]}
              </Badge>
            </span>
          </div>
        </div>
      </Card>

      {/* Billing snapshot — [live] plan status + next invoice from
          getOverview().billing. Deep-links to the full billing screen. */}
      <Card
        title="Billing"
        meta="Your subscription and next invoice"
        className="lin-mt-lg"
        actions={
          <Button href={`${base}/billing`} variant="ghost" size="sm">
            Manage plan
          </Button>
        }
      >
        <div data-testid="home-billing">
          <div data-testid="home-billing-status">
            <span className="lin-card__meta">Subscription</span>{" "}
            <Badge tone={billingMeta.tone} dot>
              {billingMeta.label}
            </Badge>
          </div>

          {hasUpcomingInvoice ? (
            <div className="lin-mt" data-testid="home-next-invoice">
              <Stat
                label="Next invoice"
                value={money(billing.amount_due_cents, billing.currency)}
                sub={`Due ${formatDate(billing.next_invoice_at)}`}
              />
            </div>
          ) : (
            <div className="lin-mt" data-testid="home-no-invoice">
              <span className="lin-card__meta">
                No upcoming invoice on file — manage or start a plan from the
                billing screen.
              </span>
            </div>
          )}
        </div>
      </Card>

      {/* ROI hero — [live] via BE-2 (getUsage). Show the real hit-rate + modeled
          $ saved once reads exist; when usage is unavailable or hit_rate is null
          (cold start) teach what will appear and WHEN — NEVER a fabricated zero. */}
      <Card title="Your cache ROI" className="lin-mt-lg">
        <div data-testid="home-roi">
          {usage != null && usage.hit_rate != null ? (
            <div data-testid="home-roi-metrics" className="lin-checklist">
              <div data-testid="home-roi-hit-rate">
                <Stat
                  label="Cache-hit rate"
                  value={`${Math.round(usage.hit_rate * 100)}%`}
                  sub="Share of cache lookups served from the cache instead of rebuilt this period."
                />
              </div>
              <div data-testid="home-roi-dollars-saved">
                <Stat
                  label="Saved (estimated)"
                  value={dollarsSaved(usage.dollars_saved_cents)}
                  sub="A modeled estimate of compute cost avoided — not a billed figure."
                />
                <span className="lin-card__meta">
                  <HelpPopover label="How is this estimated?">
                    A modeled estimate, not a billed amount: we credit roughly 15
                    seconds of compute saved per cache hit, priced at typical CI
                    compute rates. A directional savings signal, not an invoice
                    line. See Usage for the full breakdown.
                  </HelpPopover>
                </span>
              </div>
            </div>
          ) : (
            <div data-testid="home-roi-coldstart">
              <EmptyState
                title="Your savings appear here after your first builds"
                body="Cache-hit rate, build time saved, and dollars saved appear here once your cache serves its first reads. A cold start is normal — hits climb from D+1 to D+7 as your cache fills."
              />
            </div>
          )}
          <Callout tone="info">
            <strong>The shared moat works for you.</strong> Deterministic public
            dependencies are warmed for free from the shared <code>_public</code>{" "}
            mirror, so part of your hit-rate arrives before you&rsquo;ve built
            anything — while your private artifacts stay isolated to your tenant.
          </Callout>
        </div>
      </Card>

      {/* Recent activity — [stub] prod=[] (BE-3). Teaching empty, not a blank list. */}
      <Card title="Recent activity" className="lin-mt-lg">
        {activity.length > 0 ? (
          <ul data-testid="overview-activity-list">
            {activity.map((e, i) => (
              <li
                key={e.event_id}
                data-testid={`overview-activity-${e.event_id}`}
                className={i > 0 ? "lin-mt" : undefined}
              >
                <span className="lin-card__meta">{e.ts.slice(0, 10)}</span>{" "}
                <strong>{e.event_type}</strong> — {e.summary}
              </li>
            ))}
          </ul>
        ) : (
          <div data-testid="home-activity-empty">
            <EmptyState
              title="Activity shows up as you use CoreLink"
              body="Token creations, connections, and cache events will appear here once the activity feed ships (BE-3). Until then this stays empty rather than showing a fabricated event."
            />
          </div>
        )}
      </Card>
    </div>
  );
}

export default HomeClient;
