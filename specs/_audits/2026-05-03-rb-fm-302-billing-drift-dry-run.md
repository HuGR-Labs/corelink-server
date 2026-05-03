---
id: "AUDIT-2026-05-03-RB-FM-302-DRY-RUN"
type: "audit"
doc_status: "FROZEN"
audit_status: "AUDITED"
version: "1.0.0"
created: "2026-05-03"
updated: "2026-05-03"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "rb-dry-run", "rb-fm-302", "billing-drift", "reconciliation", "s10", "wi-s10-007"]
---

# RB-FM-302 dry-run — 2026-05-03 (WI-S10-007)

> **Sprint:** S-10 · **WI:** WI-S10-007 §6.1.6 · **Runbook:** [RB-FM-302](../05_quality/runbooks/RB-FM-302-billing-leak.md) · **Harness:** `scripts/rb_fm_302_billing_drift_dry_run.sh`
> **Mode:** host-side (full staging dry-run with chaos PR introducing
> 0.5% counter drift + on-call engineer execution + PagerDuty
> synthetic + audit chain capture deferred until staging account
> provisioned)

---

## 0. Identification

| Field | Value |
|---|---|
| Runbook | RB-FM-302 (Billing Counter Não Incrementa — Silent Revenue Leak) |
| FM | FM-302 (RPN=30, P1) |
| Invariants tested | INV-BILLING-NO-LOSS (HIGH; registry §3.9 line 136) + INV-BILLING-NO-DUP (HIGH; registry §3.9 line 137) + INV-BILLING-RECONCILE-3-LAYER (HIGH; registry §3.12 line 166) + INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH; lift from S-07 P1-1 fix) |
| Chaos magnitude | counter drift = 0.5% per-tenant (above SEV-2 threshold 0.1%, below SEV-1 threshold 1%) per WI-S10-007 §6.1.6 |
| Detection target | ≤ 24h p95 via reconcile daily cron 02:00 UTC (sprint contract §5.4 R-S10-8) |
| Mitigation target | ≤ 1h p95 (Stripe submission pause idempotent on (tenant, billing_period); rebuild counter from R2 events source-of-truth) |
| Resolution target | ≤ 24h p95 (rebuild + dry-run reconcile + customer notification draft) |
| Pass criteria | (a) 4-tier drift ladder fires SEV-2 at 0.5% (NoDrift/AutoFixed/Sev3/Sev2/Sev1 boundaries) + (b) auto-fix dual-condition gate refuses (count > 5 OR pct > 0.0001) + (c) replay reconstruction byte-equivalent to invoice + (d) re-reconcile green |
| Independent runs | ≥ 3 host-side iterations green; staging dry-run with seed variance documented forward |

## 1. Run summary

| Step | Description | Outcome | Evidence |
|---|---|---|---|
| 1 | Detection — corelink-billing-reconcile 4-tier drift ladder + dual-condition auto-fix gate | PASS | `cargo test -p corelink-billing-reconcile --test prop_billing_reconcile` 10 prop tests green @ 10k iter |
| 2 | Communication — SEV-2 page synthesis (host-side stub) | SIMULATED | Synthetic SEV-2 payload emitted; PagerDuty wiring deferred to live observability stack (`corelink-finance` service key WI-S10-004 production binding) |
| 3 | Mitigação imediata — corelink-billing-aggregator chain integrity + idempotent re-run | PASS | `cargo test -p corelink-billing-aggregator --test prop_billing_aggregator` 9 prop tests green @ 10k iter |
| 4 | Diagnóstico — corelink-billing-replay determinism + layer-diverged classification | PASS | `cargo test -p corelink-billing-replay --test prop_billing_replay` 10 prop tests green @ 10k iter |
| 5 | Mitigação completa — corelink-billing-emit idempotency + INV-BILLING-NO-LOSS | PASS | `cargo test -p corelink-billing-emit --test prop_billing_emit` 10 prop tests green @ 10k iter |
| 6 | Forensics + Notificação | SIMULATED | Post-mortem template scaffold + customer notification draft ready (specs/04_sprints/S10/finance-walkthrough.md template) |
| 7 | Post-mortem hooks — TLA+ regression coverage discipline | SIMULATED | TLA+ billing_atomicity.tla covers INV-BILLING-NO-LOSS / INV-BILLING-NO-DUP / Layer-1 sub-property; regression-test harness ready |

## 2. Drift detection

PASS — every expected runbook header present (Detecção / Comunicação /
Mitigação imediata / Mitigação completa / Forensics / Notificação ao
customer / Post-mortem / Prevenção). Runbook + harness MUST be updated
together on any structural change.

## 3. Findings

- **4-tier drift ladder canonical** (per WI-S10-004 SEAL): NoDrift /
  AutoFixed (Quiet tier; count ≤ 5 AND pct ≤ 0.0001) / TicketSev3 /
  PageSev2 / PageSev1AutoPaused. At the 0.5% drift magnitude (above
  SEV-2 threshold 0.1%, below SEV-1 threshold 1%) the auto-fix gate
  refuses to fire (count > 5 OR pct > 0.0001) and the SEV-2 page-arm is
  selected — exactly the runbook escalation path expected by an SRE
  on-call following RB-FM-302. `prop_drift_threshold_boundaries` 10k
  iter pins the boundary semantics; `prop_auto_fix_dual_condition_gate`
  pins the dual-condition gate.
- **SEV-1 primary-layer routing**: Layer 3 (invoice_line_item ↔ Stripe)
  drift always escalates SEV-1 + Stripe-pause idempotent flag (per WI-S10-004
  `prop_layer3_drift_pages_sev1_pauses_stripe`). Customer-facing invoice
  is legal exposure (sprint contract §15 R-Stripe-pricing-model-migration).
- **Replay endpoint cooperation**: `prop_replay_deterministic` +
  `prop_layer_diverged_flagged` (corelink-billing-replay) cross-validate
  the WI-S10-006 replay endpoint reconstruction is byte-equivalent to
  Stripe invoice (modulo Stripe metadata) so the runbook §3 "rebuild
  counters from R2 events source-of-truth" step is enforceable at the
  type system layer.
- **Aggregator chain integrity**: `prop_billing_aggregator`
  cross-validates that the rebuild-from-R2 path produces deterministic
  counter assignment — no UPSERT divergence on the 4-tuple PK
  (tenant_id, region, sku, hour) per Lote 10.10-quaters R5 P0-A fix.
- **Emit append-only**: `prop_billing_emit` cross-validates that R2
  events are append-only via Object Lock 7y retention (production
  binding WI-S10-001); idempotency by construction via
  (tenant_id, request_id) UNIQUE staging table.

## 4. Deferred items

- Full staging dry-run with chaos PR introducing real 0.5% per-tenant
  counter drift in staging + on-call engineer execution + ≥ 3
  independent runs with seed variance documented (p50 / p95 / max
  latency) deferred until staging account provisioned (sprint contract
  §13 post-sprint observation period). Revalidation trigger: staging
  account provisioned + S-19 onboarding starts.
- Real PagerDuty service key `corelink-finance` Events API v2 dispatch
  + Stripe API failure simulation (5xx mock); host-side simulated by
  `InMemoryStripeBillingAdapter` per charter `trait-abstraction-defer`
  pattern.
- Real R2 events bucket Object Lock 7y retention configuration via
  Terraform IaC + Cloudflare R2 lifecycle policy; host-side simulated
  by `InMemoryR2UsageSink`.
- 30d sustained reconciliation 0 drift > 0.1% (post-sprint observation
  period concurrent with S-11/S-13 sprints per spec contract §13
  timeline).

## 5. Sign-off

| Role | Status | Date |
|---|---|---|
| Owner | ✅ ACKNOWLEDGED | 2026-05-03 |
| SRE Lead | ⚠️ WAIVED (ADR-0034) | 2026-05-03 |
| Finance Officer | ⚠️ WAIVED (ADR-0034) — INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP cross-validated via prop_billing_reconcile + prop_billing_replay 10k iter | 2026-05-03 |

## 6. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-03 | Gustavo Schneiter (via Claude Opus 4.7 1M) | Initial RB-FM-302 host-side dry-run audit trace (WI-S10-007 SEAL Lote). |

---

**End RB-FM-302 dry-run audit v1.0.0.**
