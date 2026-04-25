---
id: "RB-BILLING-002"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.2.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p2", "billing", "late-events", "reconciliation"]
---

# RB-BILLING-002 — Late-Arriving Billing Events Triage

> **INV:** INV-BILLING-RECONCILE-3-LAYER HIGH | **SLA:** triage ≤ 24h, resolution ≤ 7d

## Pré-condições

- S-10 billing pipeline live com `usage_counter_late` D1 table (R-S10-5).
- Reconciliation worker daily executando.
- 3-layer reconciliation funcional.

## Detecção

- Métrica `corelink.billing.late_events_total` > 5% mensal sustained.
- D1 `usage_counter_late` table size cresce abnormalmente.
- Reconciliation drift detected em **Layer 1** (events R2 ↔ counters D1).
- Customer complaint sobre invoice short OR over-billed.

## Comunicação

- **Severidade**: SEV-3 (cost concern + potential drift); SEV-2 se sustained > 1 month OR > 1% revenue impact.
- **Page**: SRE on-call + Finance lead se SEV-2+.
- **Internal channel**: `#incidents-corelink-billing`.
- **Customer notification**: se affecting specific tenant invoices.

## Triage (≤ 24h)

### Step 1: Identify root cause

Possíveis causas, ordenadas por probabilidade:

1. **Network partition / region failover lag** (40-50%):
   - Recent failover (S-14)? Cross-region eventual consistency window.
   - Cloudflare outage timeline cross-reference.

2. **Bug em event emitter** (20-30%):
   - Recent S-01/S-02 code change touching CAS hot path?
   - Specific tenant pattern (one tenant generating 100% of late events)?

3. **Stripe outage backlog drain** (10-20%):
   - Recent FM-151 incident? Queue backlog took > 6h to drain → events arrive late.

4. **Clock skew em Worker** (5-10%):
   - Worker timestamp drift; events timestamped with future ts.

5. **Customer-side abuse** (rare):
   - Tenant gaming late events for reduced billing? (extremely rare; audit chain prevents).

### Step 2: Assess revenue impact

```sql
SELECT
  tenant_id,
  COUNT(*) as late_events,
  SUM(qty) as bytes_late,
  ts_event_emit,
  ts_event_arrived,
  EXTRACT(EPOCH FROM (ts_event_arrived - ts_event_emit)) / 3600 as hours_late
FROM usage_counter_late
WHERE ts_event_arrived > NOW() - INTERVAL '30 days'
GROUP BY tenant_id, ts_event_emit, ts_event_arrived
ORDER BY hours_late DESC
LIMIT 100;
```

Determine:
- Late events que fall em **billing cycle anterior** (already invoiced)? → corrective billing event needed.
- Late events em **current cycle**? → process normalmente (just delayed).
- Revenue impact em $USD? Aggregate per tenant.

### Step 3: Customer-facing decision

- **Per-tenant impact < $10/month**: process silently; no customer notification.
- **$10-100/month**: customer notification + corrective billing event next cycle.
- **> $100/month**: customer outreach + decision (waive vs bill late vs refund).
- **Cross-tenant systemic**: post-mortem + Finance + Legal review.

## Resolução

### Hot fix (24-72h)

1. **Process late_events_table**: emit corrective billing events em R2 events bucket.
2. **Audit emit**: `corelink.billing.late_events_processed` com per-tenant breakdown.
3. **Reconciliation re-run**: trigger ad-hoc reconciliation post-fix.
4. **Customer notification** se applicable.

### Cold fix (1-4 weeks)

1. **Address root cause**:
   - Network partition: improve cross-region replication retry policy.
   - Event emitter bug: fix code; deploy via S-13 progressive rollout.
   - Stripe outage handling: tighten queue retry policy PAT-BACKOFF-001.
2. **Tighten emit window**: 6h → 4h se needed (R-S10-5 update).
3. **Late events alert threshold**: 5% → 3% se SEV-2 occurrence pattern.
4. **Property test expansion**: simulate late event scenarios em CI.

## Post-incident

- Post-mortem se sustained > 1% mensal por 3 months.
- Engineer-Finance debrief.
- Update `corelink.billing.late_events_total` SLO (Lote 9.5+).
- SOC 2 audit log update se impact > $1k.

## Evidence

- D1 `usage_counter_late` query result snapshot (pre/post).
- Reconciliation report drift breakdown.
- Customer-facing impact summary.
- Network/Stripe outage cross-reference logs.
- Corrective billing events emitted log.

## Escalation

- SEV-2 sustained > 7d sem resolution → Finance lead + Compliance officer.
- SEV-1 (revenue impact > $100k OR systemic): CEO + Legal + Auditor notification.

## References

- `invariant_registry.md` INV-BILLING-RECONCILE-3-LAYER (§3.12), INV-BILLING-NO-LOSS, INV-BILLING-NO-DUP.
- `specs/04_sprints/S10/_spec_contract.md` (R-S10-5 late-arriving events policy).
- `specs/03_architecture/error_taxonomy.md` (`COR_BILLING_*`).
- RB-BILLING-001 (replay forensic — complementar).
- RB-FM-151 (Stripe outage — common upstream cause).
- RB-FM-302 (billing leak — adjacent runbook).
