// Customer overview — usage / audit / billing / BYOK snapshot.

"use client";

import React from "react";
import { CustomerClient } from "@/lib/customer-client";
import type { CustomerOverview } from "@/lib/customer-types";

const client = new CustomerClient();

function gibibytes(b: number): string {
  return (b / 1024 ** 3).toFixed(2) + " GiB";
}

function dollars(cents: number, currency: string): string {
  const sign = currency === "usd" ? "$" : currency === "eur" ? "€" : "R$";
  return sign + (cents / 100).toFixed(2);
}

export function OverviewClient(): React.ReactElement {
  const [data, setData] = React.useState<CustomerOverview | null>(null);
  const [error, setError] = React.useState<string | null>(null);

  React.useEffect(() => {
    let alive = true;
    client
      .getOverview()
      .then((d) => alive && setData(d))
      .catch((e: unknown) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, []);

  if (error) return <p data-testid="overview-error">{error}</p>;
  if (!data) return <p data-testid="overview-loading">loading…</p>;

  return (
    <div data-testid="overview-grid" style={{ display: "grid", gap: "1rem" }}>
      <section data-testid="overview-tenant-card">
        <h2>{data.tenant_name}</h2>
        <p>
          tenant: <code data-testid="overview-tenant-id">{data.tenant_id}</code> · plan:{" "}
          <strong data-testid="overview-plan">{data.plan}</strong>
        </p>
      </section>

      <section data-testid="overview-usage-card">
        <h3>Usage — {data.usage.period}</h3>
        <ul>
          <li>
            CAS storage:{" "}
            <span data-testid="overview-cas-bytes">{gibibytes(data.usage.cas_bytes)}</span> /{" "}
            {gibibytes(data.usage.quota_bytes)}
          </li>
          <li>
            Reads: <span data-testid="overview-reads">{data.usage.reads.toLocaleString()}</span>
          </li>
          <li>
            Writes:{" "}
            <span data-testid="overview-writes">{data.usage.writes.toLocaleString()}</span>
          </li>
        </ul>
      </section>

      <section data-testid="overview-billing-card">
        <h3>Billing</h3>
        <p>
          status: <span data-testid="overview-billing-status">{data.billing.status}</span> ·
          next invoice {data.billing.next_invoice_at.slice(0, 10)} for{" "}
          <strong data-testid="overview-amount-due">
            {dollars(data.billing.amount_due_cents, data.billing.currency)}
          </strong>
        </p>
      </section>

      <section data-testid="overview-byok-card">
        <h3>BYOK</h3>
        <p>
          status: <span data-testid="overview-byok-status">{data.byok.status}</span>
          {data.byok.cmk_id ? (
            <>
              {" "}
              · CMK <code>{data.byok.cmk_id}</code>
            </>
          ) : null}
        </p>
      </section>

      <section data-testid="overview-activity-card">
        <h3>Recent activity</h3>
        <ul data-testid="overview-activity-list">
          {data.recent_activity.map((e) => (
            <li key={e.event_id} data-testid={`overview-activity-${e.event_id}`}>
              <time>{e.ts}</time> — <code>{e.event_type}</code> — {e.summary}
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
}

export default OverviewClient;
