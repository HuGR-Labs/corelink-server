// WI-S16-005 — tenant deep-dive: 5 configurable cards (usage, billing,
// consents, DSR queue, active PATs). Read-only; sensitive actions route to
// /admin/ops dual-approval queue.

import React from "react";
import Link from "next/link";
import type { Tenant } from "@/lib/types";

export interface TenantDeepDiveProps {
  tenant: Tenant;
  usage?: { cas_hit_ratio: number; gb_stored: number; gb_egress: number };
  billing?: { plan: string; mrr_usd: number; next_invoice: string; payment_method_last4?: string };
  consents?: { granted: number; revoked: number; last_capture: string };
  dsr?: { pending: number; in_progress: number; completed: number };
  pats?: Array<{ pat_id: string; scope: string; created_at: string }>;
}

export function TenantDeepDive({
  tenant,
  usage,
  billing,
  consents,
  dsr,
  pats,
}: TenantDeepDiveProps): React.ReactElement {
  // PCI/PII: only ever display last4 — never full PAN, never CVC.
  const paymentDisplay = billing?.payment_method_last4
    ? `••••${billing.payment_method_last4}`
    : "—";

  return (
    <section data-testid="tenant-deep-dive" aria-label={`Tenant ${tenant.tenant_id}`}>
      <header>
        <h2>{tenant.name}</h2>
        <p>
          <code>{tenant.tenant_id}</code> · {tenant.plan} · {tenant.region}
        </p>
      </header>

      <div role="list" data-testid="tenant-cards">
        <article role="listitem" data-testid="card-usage">
          <h3>Usage</h3>
          {usage ? (
            <dl>
              <dt>CAS hit ratio</dt>
              <dd>{(usage.cas_hit_ratio * 100).toFixed(2)}%</dd>
              <dt>GB stored</dt>
              <dd>{usage.gb_stored.toFixed(2)}</dd>
              <dt>GB egress</dt>
              <dd>{usage.gb_egress.toFixed(2)}</dd>
            </dl>
          ) : (
            <p>No usage data.</p>
          )}
        </article>

        <article role="listitem" data-testid="card-billing">
          <h3>Billing</h3>
          {billing ? (
            <dl>
              <dt>Plan</dt>
              <dd>{billing.plan}</dd>
              <dt>MRR</dt>
              <dd>${billing.mrr_usd.toFixed(2)}</dd>
              <dt>Next invoice</dt>
              <dd>{billing.next_invoice}</dd>
              <dt>Payment method</dt>
              <dd data-testid="payment-method">{paymentDisplay}</dd>
            </dl>
          ) : (
            <p>No billing data.</p>
          )}
        </article>

        <article role="listitem" data-testid="card-consents">
          <h3>Consents</h3>
          {consents ? (
            <dl>
              <dt>Granted</dt>
              <dd>{consents.granted}</dd>
              <dt>Revoked</dt>
              <dd>{consents.revoked}</dd>
              <dt>Last capture</dt>
              <dd>{consents.last_capture}</dd>
            </dl>
          ) : (
            <p>No consent data.</p>
          )}
        </article>

        <article role="listitem" data-testid="card-dsr">
          <h3>DSR queue</h3>
          {dsr ? (
            <dl>
              <dt>Pending</dt>
              <dd>{dsr.pending}</dd>
              <dt>In progress</dt>
              <dd>{dsr.in_progress}</dd>
              <dt>Completed</dt>
              <dd>{dsr.completed}</dd>
            </dl>
          ) : (
            <p>No DSR data.</p>
          )}
        </article>

        <article role="listitem" data-testid="card-pats">
          <h3>Active PATs</h3>
          {pats && pats.length > 0 ? (
            <ul>
              {pats.map((p) => (
                <li key={p.pat_id}>
                  <code>{p.pat_id}</code> · {p.scope} · {p.created_at}
                </li>
              ))}
            </ul>
          ) : (
            <p>No active PATs.</p>
          )}
        </article>
      </div>

      <footer>
        <p>
          Sensitive actions (delete, BYOK rotate, residency change) must be queued in{" "}
          <Link href="../ops">/admin/ops</Link> for dual approval.
        </p>
      </footer>
    </section>
  );
}

export default TenantDeepDive;
