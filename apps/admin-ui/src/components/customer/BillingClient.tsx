// Customer-side billing — subscription, payment method, invoices, portal link.
// Stream 2.10: free-tier tenants see <UpgradeButton /> to open Stripe Checkout.

"use client";

import React from "react";
import { useAuth } from "@clerk/nextjs";
import { CustomerClient } from "@/lib/customer-client";
import type { CustomerBilling } from "@/lib/customer-types";
import { UpgradeButton } from "@/components/UpgradeButton";

function money(cents: number, currency: string): string {
  const sign = currency === "usd" ? "$" : currency === "eur" ? "€" : "R$";
  return sign + (cents / 100).toFixed(2);
}

export function BillingClient(): React.ReactElement {
  const { getToken } = useAuth();
  const client = React.useMemo(() => new CustomerClient({ getToken }), [getToken]);
  const [data, setData] = React.useState<CustomerBilling | null>(null);
  const [portalUrl, setPortalUrl] = React.useState<string | null>(null);
  const [err, setErr] = React.useState<string | null>(null);

  React.useEffect(() => {
    let alive = true;
    client
      .getBilling()
      .then((d) => alive && setData(d))
      .catch((e: unknown) => alive && setErr(String(e)));
    return () => {
      alive = false;
    };
  }, [client]);

  async function openPortal(): Promise<void> {
    try {
      const r = await client.startBillingPortal();
      setPortalUrl(r.portal_url);
    } catch (e) {
      setErr(String(e));
    }
  }

  if (err) return <p data-testid="billing-error">{err}</p>;
  if (!data) return <p data-testid="billing-loading">loading…</p>;

  return (
    <div data-testid="billing-shell">
      <section data-testid="billing-subscription">
        <h2>Subscription</h2>
        <p>
          plan: <strong data-testid="billing-plan">{data.plan}</strong> · status:{" "}
          <strong data-testid="billing-status">{data.status}</strong>
        </p>
        <p>
          period {data.current_period_start.slice(0, 10)} →{" "}
          {data.current_period_end.slice(0, 10)} · due{" "}
          <strong data-testid="billing-amount-due">
            {money(data.amount_due_cents, data.currency)}
          </strong>
        </p>
        {data.plan === "free" ? (
          <div data-testid="billing-upgrade-section">
            <UpgradeButton tier="starter" locale="en" />
          </div>
        ) : null}
      </section>

      <section data-testid="billing-payment-method">
        <h3>Payment method</h3>
        {data.payment_method ? (
          <p>
            <span data-testid="billing-brand">{data.payment_method.brand}</span> ending in{" "}
            <code data-testid="billing-last4">{data.payment_method.last4}</code> (exp{" "}
            {data.payment_method.exp_month}/{data.payment_method.exp_year})
          </p>
        ) : (
          <p>no payment method on file</p>
        )}
        <button data-testid="billing-portal-btn" type="button" onClick={openPortal}>
          Manage payment method
        </button>
        {portalUrl ? (
          <p data-testid="billing-portal-url">
            redirect: <a href={portalUrl}>{portalUrl}</a>
          </p>
        ) : null}
      </section>

      <section data-testid="billing-invoices">
        <h3>Invoices</h3>
        <ul>
          {data.invoices.map((inv) => (
            <li key={inv.invoice_id} data-testid={`billing-invoice-${inv.invoice_id}`}>
              {inv.issued_at.slice(0, 10)} — {money(inv.amount_cents, data.currency)} —{" "}
              {inv.status} —{" "}
              <a href={inv.hosted_url} rel="noreferrer">
                view
              </a>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}

export default BillingClient;
