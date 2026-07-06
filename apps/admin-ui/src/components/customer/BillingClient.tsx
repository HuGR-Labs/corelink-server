// Customer-side Plan & billing — the two-axis plan surface (cache tier + runner
// SKU), one Stripe portal button, one upgrade path, honest empty states.
//
// W7 (customer-dashboard build wave). Kit-only: every surface is a
// `@/components/ui/linear` primitive. Enums are humanized (never raw). Exactly
// ONE "Manage subscription" button (the Stripe portal via `startBillingPortal`
// — the redundant raw-URL PortalLauncher flow is gone) and ONE upgrade path
// (the shared <UpgradeButton /> for free-tier tenants). Data-truth: status/plan
// + portal + checkout are [live]; `invoices`/`payment_method` are [stub] — they
// render a TEACHING empty state, never a fabricated card or line item.

"use client";

import React from "react";
import { useAuth } from "@clerk/nextjs";
import { CustomerClient } from "@/lib/customer-client";
import type { CustomerBilling } from "@/lib/customer-types";
import { UpgradeButton } from "@/components/UpgradeButton";
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

// ── Humanizers (no raw enum ever reaches the screen) ────────────────────────

/** Human label for a cache tier id. */
const PLAN_LABELS: Record<CustomerBilling["plan"], string> = {
  free: "Free",
  solo: "Solo",
  starter: "Starter",
  team: "Team",
  pro: "Pro",
  max: "Max",
  enterprise: "Enterprise",
};

/** Human label + Badge tone for a subscription status. */
const STATUS_META: Record<
  CustomerBilling["status"],
  { label: string; tone: "neutral" | "success" | "warn" | "danger" }
> = {
  trialing: { label: "Trialing", tone: "success" },
  active: { label: "Active", tone: "success" },
  past_due: { label: "Past due", tone: "danger" },
  canceled: { label: "Canceled", tone: "warn" },
  inactive: { label: "No subscription", tone: "neutral" },
};

const INVOICE_STATUS_LABEL: Record<"paid" | "open" | "void", string> = {
  paid: "Paid",
  open: "Open",
  void: "Void",
};

function money(cents: number, currency: CustomerBilling["currency"]): string {
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

export function BillingClient(): React.ReactElement {
  const { getToken } = useAuth();
  const client = React.useMemo(() => new CustomerClient({ getToken }), [getToken]);
  const [data, setData] = React.useState<CustomerBilling | null>(null);
  const [err, setErr] = React.useState<unknown>(null);
  const [portalBusy, setPortalBusy] = React.useState(false);
  const [portalErr, setPortalErr] = React.useState<string | null>(null);

  const load = React.useCallback(() => {
    let alive = true;
    setErr(null);
    setData(null);
    client
      .getBilling()
      .then((d) => alive && setData(d))
      .catch((e: unknown) => alive && setErr(e));
    return () => {
      alive = false;
    };
  }, [client]);

  React.useEffect(() => load(), [load]);

  async function openPortal(): Promise<void> {
    setPortalErr(null);
    setPortalBusy(true);
    try {
      const { portal_url } = await client.startBillingPortal();
      if (!portal_url.startsWith("https://")) {
        throw new Error("Billing portal is temporarily unavailable. Try again shortly.");
      }
      if (typeof window !== "undefined") {
        window.location.assign(portal_url);
      }
    } catch (e) {
      setPortalErr(e instanceof Error ? e.message : "Could not open the billing portal.");
      setPortalBusy(false);
    }
  }

  if (err != null) {
    return (
      <div data-testid="billing-error">
        <InlineError error={err} onRetry={load} />
      </div>
    );
  }

  if (data == null) {
    return (
      <div data-testid="billing-loading">
        <Card title="Plan & billing">
          <Skeleton rows={4} />
        </Card>
      </div>
    );
  }

  const isFree = data.plan === "free";
  const isEnterprise = data.plan === "enterprise";
  const status = STATUS_META[data.status];
  const planLabel = PLAN_LABELS[data.plan];

  // ONE portal control — the Stripe Customer Portal. It is the single home for
  // payment method, invoices, plan changes and cancellation for paying tenants.
  const manageButton = (
    <div data-testid="billing-manage">
      <Button
        variant="ghost"
        onClick={openPortal}
        loading={portalBusy}
        data-testid="billing-portal-btn"
      >
        Manage subscription
      </Button>
      {portalErr != null ? (
        <div data-testid="billing-portal-error">
          <InlineError error={portalErr} onRetry={openPortal} />
        </div>
      ) : null}
    </div>
  );

  return (
    <div data-testid="billing-shell">
      {/* ── Axis 1: cache tier ─────────────────────────────────────────── */}
      <Card
        title="Cache plan"
        meta="Your storage & cache-request tier"
        actions={!isFree && !isEnterprise ? manageButton : undefined}
      >
        <div data-testid="billing-subscription">
          <p>
            <strong data-testid="billing-plan">{planLabel}</strong>{" "}
            <span data-testid="billing-status">
              <Badge tone={status.tone} dot>
                {status.label}
              </Badge>
            </span>
          </p>

          {!isFree ? (
            <p className="lin-card__meta">
              Current period {formatDate(data.current_period_start)} →{" "}
              {formatDate(data.current_period_end)} · next charge{" "}
              <strong data-testid="billing-amount-due">
                {money(data.amount_due_cents, data.currency)}
              </strong>
            </p>
          ) : (
            <p className="lin-card__meta" data-testid="billing-free-note">
              You are on the Free tier — no card on file, no charges. Upgrade when
              you outgrow the free ceilings.
            </p>
          )}

          {/* ── ONE upgrade path (free tier only) ── */}
          {isFree ? (
            <div data-testid="billing-upgrade-section" className="lin-mt">
              <Callout tone="info">
                <strong>Upgrade to Starter — {money(3500, "usd")}/mo.</strong> 150 GB
                CAS storage and 6M cache requests a month (vs 10 GB / 500K on Free).{" "}
                Break-even: at a ~90% hit rate, Starter typically pays for itself after
                roughly one saved developer-hour of build time each month.
              </Callout>
              <div className="lin-mt">
                <UpgradeButton tier="starter" locale="en" />
              </div>
            </div>
          ) : null}

          {/* ── Enterprise: locked "contact sales" row (not hidden) ── */}
          {isEnterprise ? (
            <div data-testid="billing-enterprise-row" className="lin-mt">
              <Callout tone="info">
                Your Enterprise subscription is billed via contract, not self-serve.
                Reach your account team to change plan, seats, or terms.
              </Callout>
              <div className="lin-mt">
                <Button
                  variant="ghost"
                  onClick={() => {
                    if (typeof window !== "undefined") {
                      window.location.assign("mailto:gustavo@humangr.com");
                    }
                  }}
                  data-testid="billing-contact-sales"
                >
                  Contact sales
                </Button>
              </div>
            </div>
          ) : null}
        </div>
      </Card>

      {/* ── Axis 2: runner SKU (independent axis) ──────────────────────── */}
      <Card
        title="Runners"
        meta="Cache-accelerated CI — a separate entitlement from your cache plan"
        className="lin-mt"
      >
        <div data-testid="billing-runner-axis">
          <p className="lin-card__meta">
            No runner subscription on this account yet.{" "}
            <HelpPopover label="What are runners?">
              Runners are a separate add-on from your cache plan — cache-accelerated
              CI runners billed on concurrency + monthly vCPU-hours. You can hold a
              cache tier and a runner SKU at the same time.
            </HelpPopover>
          </p>
          <div className="lin-mt">
            <UpgradeButton tier="runner_starter" locale="en" label="Add runners" />
          </div>
        </div>
      </Card>

      {/* ── Payment method [stub] → managed in Stripe, never "no card" ──── */}
      <Card title="Payment method" className="lin-mt">
        <div data-testid="billing-payment-method">
          <p className="lin-card__meta" data-testid="billing-pm-managed">
            Managed in the Stripe portal. Add, update or remove your card there —
            CoreLink never stores card numbers. Use{" "}
            <strong>Manage subscription</strong> above to open it.
          </p>
        </div>
      </Card>

      {/* ── Invoices [stub] → teaching empty, never fabricated ─────────── */}
      <Card title="Invoices" className="lin-mt">
        <div data-testid="billing-invoices">
          {data.invoices.length > 0 ? (
            <table className="lin-table" data-testid="billing-invoice-list">
              <thead>
                <tr>
                  <th>Issued</th>
                  <th>Amount</th>
                  <th>Status</th>
                  <th aria-label="Link" />
                </tr>
              </thead>
              <tbody>
                {data.invoices.map((inv) => (
                  <tr key={inv.invoice_id} data-testid={`billing-invoice-${inv.invoice_id}`}>
                    <td>{formatDate(inv.issued_at)}</td>
                    <td>{money(inv.amount_cents, data.currency)}</td>
                    <td>
                      <Badge tone={inv.status === "paid" ? "success" : "neutral"}>
                        {INVOICE_STATUS_LABEL[inv.status]}
                      </Badge>
                    </td>
                    <td>
                      <a href={inv.hosted_url} rel="noreferrer">
                        View
                      </a>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <div data-testid="billing-invoices-empty">
              <EmptyState
                title="Invoices appear here after your first payment"
                body="Once you have a paid subscription, every invoice will be listed here and in the Stripe portal."
              />
            </div>
          )}
        </div>
      </Card>
    </div>
  );
}

export default BillingClient;
