// Customer-side Plan & billing — the two-axis plan surface (cache tier + runner
// SKU) built as a real Linear tier ladder, one Stripe portal button, honest
// empty states.
//
// W7 / W10 (customer-dashboard build wave — Billing rebuild). Kit-only: every
// surface is a `@/components/ui/linear` primitive. Enums are humanized (never
// raw). The plan ladder is sourced 1:1 from `lib/pricing.ts` TIERS — NO
// hardcoded prices anywhere on this screen.
//
// Change-of-plan doctrine (SOTA / no double-billing): a tenant WITHOUT a live
// subscription (free / inactive / canceled) sees a real self-serve checkout CTA
// per checkout-able tier (the shared <UpgradeButton />). A tenant WITH a live
// subscription changes plan (up, down, or cancel) in the Stripe Customer Portal
// — the only Stripe-correct place for proration on an existing subscription;
// spinning a fresh Checkout session for them would mint a SECOND subscription.
//
// Data-truth: status/plan + portal + checkout are [live]; `invoices`/
// `payment_method` are [stub] — they render a TEACHING empty state, never a
// fabricated card or line item. Exactly ONE "Manage subscription" button (the
// Stripe portal via `startBillingPortal`).

"use client";

import React from "react";
import { useCustomerClient } from "@/lib/use-customer-client";
import type { CustomerBilling } from "@/lib/customer-types";
import { UpgradeButton } from "@/components/UpgradeButton";
import { TIERS, isCheckoutTierId, type Tier } from "@/lib/pricing";
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

/** Ordinal position of each plan on the ladder — drives upgrade/downgrade copy. */
const PLAN_RANK: Record<CustomerBilling["plan"], number> = {
  free: 0,
  solo: 1,
  starter: 2,
  team: 2,
  pro: 3,
  max: 4,
  enterprise: 5,
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

// The two product axes, sourced 1:1 from the frozen rate card in lib/pricing.ts.
const CACHE_TIERS: Tier[] = TIERS.filter((t) => (t.group ?? "cache") === "cache");
const RUNNER_TIERS: Tier[] = TIERS.filter((t) => t.group === "runner");
// The self-serve cache ladder (Free…Max); Enterprise is rendered as its own row.
const LADDER_TIERS: Tier[] = CACHE_TIERS.filter((t) => t.id !== "enterprise");
const ENTERPRISE_TIER: Tier | undefined = CACHE_TIERS.find((t) => t.id === "enterprise");

const CONTACT_SALES_HREF = "mailto:gustavo@humangr.com";

export function BillingClient(): React.ReactElement {
  const client = useCustomerClient();
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

  const currentPlan = data.plan;
  const isFree = currentPlan === "free";
  const isEnterprise = currentPlan === "enterprise";
  const status = STATUS_META[data.status];
  const planLabel = PLAN_LABELS[currentPlan];
  const currentRank = PLAN_RANK[currentPlan];

  // A tenant with a live subscription changes plan in the Stripe portal (the
  // only Stripe-correct home for proration on an existing sub). A tenant WITHOUT
  // one may self-serve checkout directly into any paid tier.
  const hasLiveSub =
    data.status === "active" || data.status === "trialing" || data.status === "past_due";
  const canCheckout = !hasLiveSub;

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

  // One ladder row for a checkout-able cache tier.
  function renderTierRow(tier: Tier): React.ReactElement {
    const isCurrent = tier.id === currentPlan;
    const rowClasses = [
      "flex items-center justify-between gap-4 rounded-[8px] border px-4 py-3",
      isCurrent ? "border-[var(--line-2)] bg-[var(--panel)]" : "border-[var(--line)]",
    ].join(" ");

    // CTA: current → badge; no live sub + checkout-able → real checkout; live
    // sub → nothing (plan changes route through the portal, see the note below).
    let cta: React.ReactElement | null = null;
    if (isCurrent) {
      cta = (
        <Badge tone="success" dot>
          Current plan
        </Badge>
      );
    } else if (canCheckout && isCheckoutTierId(tier.id)) {
      const goingUp = PLAN_RANK[tier.id as CustomerBilling["plan"]] > currentRank;
      cta = (
        <UpgradeButton
          tier={tier.id}
          locale="en"
          label={goingUp ? `Choose ${tier.name}` : `Switch to ${tier.name}`}
        />
      );
    }

    return (
      <div
        key={tier.id}
        data-testid={`billing-tier-${tier.id}`}
        className={rowClasses}
      >
        <div className="min-w-0">
          <div className="flex items-baseline gap-2">
            <span className="text-[14px] font-[510] text-[var(--t1)]">{tier.name}</span>
            <span className="text-[13px] text-[var(--t3)]">
              {tier.price ? `${tier.price}${tier.cadence}` : "Custom"}
            </span>
          </div>
          <div className="mt-1 truncate text-[13px] text-[var(--t3)]">
            {tier.features.slice(0, 2).join(" · ")}
          </div>
        </div>
        <div className="shrink-0">{cta}</div>
      </div>
    );
  }

  return (
    <div data-testid="billing-shell">
      {/* ── Current plan (cache axis) ──────────────────────────────────── */}
      <Card
        title="Current plan"
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

          {/* Enterprise: locked "contact sales" row (not hidden) */}
          {isEnterprise ? (
            <div data-testid="billing-enterprise-row" className="lin-mt">
              <Callout tone="info">
                Your Enterprise subscription is billed via contract, not self-serve.
                Reach your account team to change plan, seats, or terms.
              </Callout>
              <div className="lin-mt">
                <Button
                  variant="ghost"
                  href={CONTACT_SALES_HREF}
                  data-testid="billing-contact-sales"
                >
                  Contact sales
                </Button>
              </div>
            </div>
          ) : null}
        </div>
      </Card>

      {/* ── Cache plan ladder (sourced 1:1 from lib/pricing.ts TIERS) ───── */}
      <Card
        title="Cache plans"
        meta="Compare tiers — storage, cache requests, and support"
        className="lin-mt"
      >
        <div data-testid="billing-upgrade-section" className="flex flex-col gap-2">
          {LADDER_TIERS.map(renderTierRow)}

          {/* Enterprise = contact-sales row (never checkout-able) */}
          {ENTERPRISE_TIER != null ? (
            <div
              data-testid="billing-tier-enterprise"
              className="flex items-center justify-between gap-4 rounded-[8px] border border-[var(--line)] px-4 py-3"
            >
              <div className="min-w-0">
                <div className="flex items-baseline gap-2">
                  <span className="text-[14px] font-[510] text-[var(--t1)]">
                    {ENTERPRISE_TIER.name}
                  </span>
                  <span className="text-[13px] text-[var(--t3)]">Custom</span>
                </div>
                <div className="mt-1 truncate text-[13px] text-[var(--t3)]">
                  {ENTERPRISE_TIER.features.slice(0, 2).join(" · ")}
                </div>
              </div>
              <div className="shrink-0">
                <Button
                  variant="ghost"
                  size="sm"
                  href={CONTACT_SALES_HREF}
                  data-testid="billing-tier-enterprise-cta"
                >
                  {ENTERPRISE_TIER.cta}
                </Button>
              </div>
            </div>
          ) : null}
        </div>

        {/* Paying tenants change plan in the portal, never a fresh Checkout. */}
        {hasLiveSub && !isEnterprise ? (
          <div className="lin-mt" data-testid="billing-change-note">
            <Callout tone="info">
              You are on <strong>{planLabel}</strong>. To upgrade, downgrade, or
              cancel, open <strong>Manage subscription</strong> above — plan changes
              on an active subscription are handled in the billing portal.
            </Callout>
          </div>
        ) : null}
      </Card>

      {/* ── Runner add-on (independent axis — its own SKU ladder) ───────── */}
      <Card
        title="Runners"
        meta="Cache-accelerated CI — a separate entitlement from your cache plan"
        className="lin-mt"
      >
        <div data-testid="billing-runner-axis">
          <p className="lin-t2 text-[13px]">
            Runners are a separate add-on from your cache plan.{" "}
            <HelpPopover label="What are runners?">
              Cache-accelerated CI runners billed on concurrency + monthly vCPU-hours.
              You can hold a cache tier and a runner SKU at the same time.
            </HelpPopover>
          </p>
          <div className="lin-mt flex flex-col gap-2">
            {RUNNER_TIERS.map((rt) => (
              <div
                key={rt.id}
                data-testid={`billing-runner-tier-${rt.id}`}
                className="flex items-center justify-between gap-4 rounded-[8px] border border-[var(--line)] px-4 py-3"
              >
                <div className="min-w-0">
                  <div className="flex items-baseline gap-2">
                    <span className="text-[14px] font-[510] text-[var(--t1)]">
                      {rt.name}
                    </span>
                    <span className="text-[13px] text-[var(--t3)]">
                      {rt.price}
                      {rt.cadence}
                    </span>
                  </div>
                  <div className="mt-1 truncate text-[13px] text-[var(--t3)]">
                    {rt.features.slice(0, 2).join(" · ")}
                  </div>
                </div>
                <div className="shrink-0">
                  {isCheckoutTierId(rt.id) ? (
                    <UpgradeButton tier={rt.id} locale="en" label="Get" />
                  ) : null}
                </div>
              </div>
            ))}
          </div>
        </div>
      </Card>

      {/* ── Payment method [stub] → managed in Stripe, never fabricated ──── */}
      <Card title="Payment method" className="lin-mt">
        <div data-testid="billing-payment-method">
          <Callout tone="info">
            {isFree ? (
              <span data-testid="billing-pm-managed" className="lin-t2">
                No card on file — the Free tier has no charges. Your card is added
                in the Stripe portal when you upgrade. CoreLink never stores card
                numbers.
              </span>
            ) : (
              <span data-testid="billing-pm-managed" className="lin-t2">
                Managed in the Stripe portal — add, update, or remove your card
                there via <strong>Manage subscription</strong> above. CoreLink never
                stores card numbers.
              </span>
            )}
          </Callout>
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
                      <Button
                        variant="ghost"
                        size="sm"
                        href={inv.hosted_url}
                        target="_blank"
                        rel="noopener noreferrer"
                      >
                        View
                      </Button>
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
