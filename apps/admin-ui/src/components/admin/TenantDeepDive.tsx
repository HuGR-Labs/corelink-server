// WI-S16-005 — tenant deep-dive: 5 configurable cards (usage, billing,
// consents, DSR queue, active PATs). Read-only; sensitive actions route to
// /admin/ops dual-approval queue.
//
// Linear kit: glass `lin-card` panels, BYOK Badge, EmptyState for missing data,
// a Callout for the dual-approval note. Testids + last4 redaction preserved.

import React from "react";
import Link from "next/link";
import type { Tenant } from "@/lib/types";
import { Badge, Callout, EmptyState } from "@/components/ui/linear";
import { byokTone } from "./tones";

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
      <div className="lin-card lin-card--pad">
        <h2 className="lin-card__title">{tenant.name}</h2>
        <p className="lin-card__meta">
          <code>{tenant.tenant_id}</code> · {tenant.plan} · {tenant.region} ·{" "}
          <Badge tone={byokTone(tenant.byok_status)} dot>
            BYOK {tenant.byok_status}
          </Badge>
        </p>
      </div>

      <div role="list" data-testid="tenant-cards" className="lin-mt">
        <article
          role="listitem"
          data-testid="card-usage"
          className="lin-card lin-card--pad"
        >
          <h3 className="lin-card__title">Usage</h3>
          {usage ? (
            <dl>
              <dt className="lin-card__meta">CAS hit ratio</dt>
              <dd>{(usage.cas_hit_ratio * 100).toFixed(2)}%</dd>
              <dt className="lin-card__meta">GB stored</dt>
              <dd>{usage.gb_stored.toFixed(2)}</dd>
              <dt className="lin-card__meta">GB egress</dt>
              <dd>{usage.gb_egress.toFixed(2)}</dd>
            </dl>
          ) : (
            <EmptyState title="No usage data" />
          )}
        </article>

        <article
          role="listitem"
          data-testid="card-billing"
          className="lin-card lin-card--pad lin-mt"
        >
          <h3 className="lin-card__title">Billing</h3>
          {billing ? (
            <dl>
              <dt className="lin-card__meta">Plan</dt>
              <dd>{billing.plan}</dd>
              <dt className="lin-card__meta">MRR</dt>
              <dd>${billing.mrr_usd.toFixed(2)}</dd>
              <dt className="lin-card__meta">Next invoice</dt>
              <dd>{billing.next_invoice}</dd>
              <dt className="lin-card__meta">Payment method</dt>
              <dd data-testid="payment-method">{paymentDisplay}</dd>
            </dl>
          ) : (
            <EmptyState title="No billing data" />
          )}
        </article>

        <article
          role="listitem"
          data-testid="card-consents"
          className="lin-card lin-card--pad lin-mt"
        >
          <h3 className="lin-card__title">Consents</h3>
          {consents ? (
            <dl>
              <dt className="lin-card__meta">Granted</dt>
              <dd>{consents.granted}</dd>
              <dt className="lin-card__meta">Revoked</dt>
              <dd>{consents.revoked}</dd>
              <dt className="lin-card__meta">Last capture</dt>
              <dd>{consents.last_capture}</dd>
            </dl>
          ) : (
            <EmptyState title="No consent data" />
          )}
        </article>

        <article
          role="listitem"
          data-testid="card-dsr"
          className="lin-card lin-card--pad lin-mt"
        >
          <h3 className="lin-card__title">DSR queue</h3>
          {dsr ? (
            <dl>
              <dt className="lin-card__meta">Pending</dt>
              <dd>{dsr.pending}</dd>
              <dt className="lin-card__meta">In progress</dt>
              <dd>{dsr.in_progress}</dd>
              <dt className="lin-card__meta">Completed</dt>
              <dd>{dsr.completed}</dd>
            </dl>
          ) : (
            <EmptyState title="No DSR data" />
          )}
        </article>

        <article
          role="listitem"
          data-testid="card-pats"
          className="lin-card lin-card--pad lin-mt"
        >
          <h3 className="lin-card__title">Active PATs</h3>
          {pats && pats.length > 0 ? (
            <ul className="lin-checklist">
              {pats.map((p) => (
                <li key={p.pat_id}>
                  <code>{p.pat_id}</code>{" "}
                  <span className="lin-card__meta">
                    {p.scope} · {p.created_at}
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <EmptyState title="No active PATs" />
          )}
        </article>
      </div>

      <div className="lin-mt">
        <Callout tone="warn">
          Sensitive actions (delete, BYOK rotate, residency change) must be queued in{" "}
          <Link href="../ops">/admin/ops</Link> for dual approval.
        </Callout>
      </div>
    </section>
  );
}

export default TenantDeepDive;
