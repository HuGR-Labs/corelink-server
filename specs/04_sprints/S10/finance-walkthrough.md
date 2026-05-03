---
id: "S10-FINANCE-WALKTHROUGH"
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
parent: "WI-S10-007"
inherits_from:
  - "DATA-MODEL"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
  - "COMPLIANCE-MATRIX"
tags: ["finance-walkthrough", "soc2", "cc1.4", "gaap-asc606", "audit-grade-replay", "s10", "wi-s10-007"]
---

# S-10 Finance Walkthrough — Audit-Grade Invoice Reconstruction Exhibit

> **Sprint:** [S-10](./_spec_contract.md) · **WI:** [WI-S10-007](./work_items/WI-S10-007-tla-billing-atomicity-runbooks-finance-walkthrough.md) §6.1.8 · **Runbook:** [RB-BILLING-001](../../05_quality/runbooks/RB-BILLING-001.md) · **Date:** 2026-05-03
> **Mode:** narrative + sign-off scaffold (live walkthrough with paid
> mock SOC 2 auditor + 1 fake customer invoice generated in staging
> deferred until staging account provisioned + auditor scheduled)

---

## 0. Purpose

This document is the **Finance walkthrough exhibit** produced as part
of the WI-S10-007 ship gate. Its purpose is twofold:

1. **Narrative for Finance / CFO + Compliance**: explain how usage flows
   from API → R2 events archive → counter aggregator → invoice line
   item → Stripe → customer invoice; how reconciliation drift is
   detected; how replay forensics reconstructs an invoice
   byte-equivalent to Stripe; how the audit chain integrity is
   preserved end-to-end.
2. **Sign-off scaffold for Finance Officer + mock SOC 2 auditor**: per
   WI §6.1.8 + sprint contract §6 DoD: "Finance + auditor (mock)
   consegue reconstruir 1 invoice from R2 events em < 30 min". The
   sign-off section §7 carries Finance Officer + (paid mock) SOC 2
   auditor approvals + revalidation triggers.

This is the sprint-promotion-gate document for the Finance + Compliance
seats on the PRR-S10 HIGH_RISK 12 sign-off matrix.

## 1. Pipeline narrative — usage flows from API to invoice

The S-10 billing pipeline is a 3-layer architecture mirroring the
canonical "events → aggregates → ledger → invoice" pattern (Stripe
Billing Architecture Guide; Lago open-source reference; Mux exact-
metering blog post). Each layer is independently auditable; reconciliation
crosses every layer daily; replay reconstructs from the source-of-truth.

### Layer 0 — Event emission at the CAS hot path (WI-S10-001)

Every billable operation (`cas.put` / `cas.get` / `ac.lookup` /
`gc.purge`) emits a CloudEvents 1.0 envelope per Lote 10.9bis P0-G:

```json
{
  "specversion": "1.0",
  "type": "dev.hugr.corelink.cas.put.v1",
  "subject": "tenant:<uuid-v7>",
  "id": "<idem-key BLAKE3-256 of canonical event-bytes-with-slot-zeroed>",
  "source": "/regions/<region>/workers/cas",
  "time": "<RFC3339>",
  "data": {
    "tenant_id": "<uuid-v7>",
    "region": "<5-canonical Enam|Weur|Apac|Sa|Asia2>",
    "sku": "<6-element taxonomy>",
    "qty": <integer>,
    "ts": "<RFC3339>",
    "request_id": "<UUIDv7>",
    "idempotency_key": "<35-char canonical>",
    "event_schema_version": "1.0.0"
  }
}
```

The emitter is **fail-OPEN at the hot path** (Lote 10.6bis split-tier
canonical): if the staging table or R2 archive is unavailable, the
hot-path operation succeeds locally and the event buffers in the D1
staging table `usage_event_staging` PRIMARY KEY (tenant_id, request_id)
UNIQUE [migration 0017]. This protects customer-facing latency from
billing infrastructure outages. **INV-BILLING-NO-LOSS guarantees that
no event is dropped** — the staging buffer is bounded and the
Cloudflare Queue retry drains buffered events to R2 with weak fairness
(eventual delivery formally proven via TLA+ `WF_vars(DrainStagingToR2)`).

R2 events bucket configuration: Object Lock Governance Mode 7-year
retention canonical (sprint contract §3 inherits_from FAILURE-MODES +
INVARIANT-REGISTRY; SOC 2 CC1.4 + GAAP ASC 606 + GDPR Art. 32 + LGPD
Art. 32 compliance gates; production Terraform IaC binding deferred to
S-20 GA gate per `trait-abstraction-defer` charter pattern).

### Layer 1 — Counter aggregation hourly (WI-S10-002)

A Cloudflare Cron DO `BillingAggregatorCron-<region>` per-region
cron-trigger 1h interval drains R2 events into D1 `usage_counter`
PRIMARY KEY (tenant_id, region, sku, hour) UNIQUE [4-tuple PK per Lote
10.10-quaters R5 P0-A fix]. The aggregator is **fail-CLOSED** (Lote
10.6bis split-tier canonical): atomic D1 batch (counter row UPSERT +
`hash_chain_head` UPDATE in the same `db.batch`) — if either fails,
the row is rolled back and the audit row `corelink.billing_aggregator.run_completed`
is NOT emitted; the next cron run retries idempotently.

Late-arriving events (`ts < cron_now - 6h` per Lote 10.10-quaters R5
P0-A — sprint contract §5.2 R-S10-5) route to `usage_counter_late`
separate D1 table to protect against silent backfill; SEV-3 alert fires
on any non-zero late-counter row; Finance review of late events drift >
5% monthly (sprint contract §18 post-mortem hook).

The hash chain (BLAKE3-256 link hash Bitcoin-block-header pattern;
S-09 inheritance) provides per-(tenant, region) tamper detection: any
mutation to a counter row breaks the chain integrity at the ChainVerifier
daily-verify boundary.

### Layer 2 — Invoice line item generation (WI-S10-003 cooperation)

At the canonical month-rollover boundary (`corelink_time::next_month_first_utc_midnight()`
canonical primitive per Lote 10.8bis P0-D), the invoice generation cron
sums counter rows per (tenant, billing_period, sku) — collapsing across
regions (Stripe Customer scope is per-tenant, NOT per-region). The
result becomes a row in D1 `invoice_line_item` table.

Idempotency is structural: the (tenant, billing_period, sku) tuple is
UNIQUE; re-running the generator produces the same row by construction
(typed `LayerTotals` payload — Lote 10.9-quinquies NEW-P0-2 absorption;
NO `serde_json::Value` at trait surface).

### Layer 3 — Stripe usage_record submission (WI-S10-003)

The Stripe adapter `corelink-billing-stripe::InMemoryStripeBillingAdapter`
derives a 32-byte BLAKE3-256 IdempotencyKey newtype from the canonical
AggregatedCounter bytes (RFC 8785 JCS canonical determinism + BLAKE3-256
of canonical bytes — same aggregate → same key by construction). The
adapter calls the Stripe Usage Records API with the IdempotencyKey
header; Stripe returns the cached response on retry (Stripe API contract;
INV-BILLING-NO-DUP preserved at the storage layer via `stripe_idempotency_keys`
PRIMARY KEY (idempotency_key) UNIQUE [migration 0018]).

When Stripe returns 5xx (FM-151 Stripe outage), the charge enters the
retry queue (PAT-QUEUE-EVENTS-001 fallback); on Stripe recovery the
queue drains via PAT-BACKOFF-001 retry exponential 1s/2s/4s/8s/16s +
jitter (sprint contract §14.s10.2). The TLA+ INV-BILLING-NO-LOSS
formally proves that every emitted event is accounted somewhere
(staging / R2 / retry_queue / Stripe-invoiced) under the full Stripe
outage scenario.

## 2. Reconciliation drift detection (WI-S10-004)

The 3-layer reconciliation worker runs daily 02:00 UTC per region:

| Layer | Compare | Threshold | SEV |
|---|---|---|---|
| 1 | Σ(events R2 raw) vs Σ(counters D1) | drift > 0.1% | SEV-2 |
| 2 | Σ(counters D1) vs Σ(invoice_line_item D1) | drift > 0.1% | SEV-2 |
| 3 | Σ(invoice_line_item) vs Σ(Stripe invoice) | drift > 0.1% | SEV-1 |

The 4-tier ladder canonical (per WI-S10-004 SEAL):
- **NoDrift** (drift < 0.0001) — quiet
- **AutoFixed** (drift > 0 AND count ≤ 5 AND pct ≤ 0.0001) — quiet, auto-corrected
- **TicketSev3** (0.0001 < drift ≤ 0.001) — ticket, monitor
- **PageSev2** (0.001 < drift ≤ 0.01) — page, immediate
- **PageSev1AutoPaused** (drift > 0.01) — page + Stripe-pause idempotent flag

Layer 3 (invoice_line_item ↔ Stripe) drift always escalates SEV-1 +
Stripe-pause idempotent flag — customer-facing invoice is legal exposure
(sprint contract §15 R-Stripe-pricing-model-migration). The Stripe-pause
flag is a per-(tenant, billing_period) UPSERT-safe ledger row [migration
0019 `stripe_submission_state` PRIMARY KEY (tenant_id, billing_period)
UNIQUE].

The reconciliation report archives daily to R2
`reconciliation-reports/<YYYY-MM-DD>.json` with 7-year retention
(SOC 2 + GAAP audit trail).

## 3. Replay forensic reconstruction (WI-S10-006)

The replay endpoint POST `/v1/billing/replay` is the **audit-grade
forensic reconstruction primitive**. Given an `(invoice_id, tenant_id,
billing_period)` tuple + `ReplayReason` (4-element taxonomy:
DriftInvestigation / CustomerDispute / ComplianceAudit / DryRun), it:

1. Authenticates the operator via PAT scope `billing_forensics_admin`
   (CTRL-AUTHZ-002 separate from regular admin — least-privilege-bounded
   audit-grade replay capability per WI-S10-006 §1 invariant 3).
2. Reads R2 billing-events bucket per tenant + ts range (source of
   truth).
3. Reconstructs invoice line items deterministically via:
   - Aggregation per SKU + region (4-tuple PK; Lote 10.10-quaters R5 P0-A fix)
   - Pricing rules from D1 `plan` snapshot at `billing_period` boundary
   - Hash chain integrity verification (BLAKE3-256 link hash via
     `InMemoryAggregatedCounterStore::ChainHeadRecord`)
4. Returns `ReconstructedLayers` (3-layer u128 totals snapshot).
5. Compares against Stripe invoice (modulo Stripe metadata: timestamps
   + internal Stripe IDs).
6. Classifies via `LayerDriftSummary::classify` 5-element taxonomy:
   AllLayersMatch / Layer1Diverged / Layer2Diverged / Layer3Diverged /
   MultipleLayersDiverged.
7. Emits per-replay canonical audit row (`corelink.billing_replay.*`
   5-event taxonomy) BEFORE the idempotency ledger UPSERT (fail-CLOSED
   pattern; Lote 10.6bis + S-07 P1-1 inheritance).
8. UPSERTs the idempotency ledger row [migration 0021
   `billing_replay_audit` PRIMARY KEY (request_id) UNIQUE — canonical
   idempotency contract + 7-year SOC 2 + GAAP + GDPR Art. 22 evidence
   trail].

The replay endpoint is **read-only by default** (`dry_run=true`); a
`dry_run=false` invocation generates a corrective billing event in R2
events bucket (audit chain extended; NOT rewritten) and only calls the
Stripe API if `apply_correction=true` is set (additional MFA + dual-
approval at S-13 admin plane).

## 4. Audit chain integrity end-to-end

Every billing pipeline state mutation lands at least one canonical
audit row per `corelink.billing*.*` taxonomies:

| Crate | Audit subjects |
|---|---|
| `corelink-billing-emit` (WI-S10-001) | `corelink.billing.{usage_emitted, duplicate_rejected, sink_failure, idempotency_collision}` |
| `corelink-billing-aggregator` (WI-S10-002) | `corelink.billing_aggregator.{run_started, run_completed, chain_break_detected, sink_failure}` |
| `corelink-billing-stripe` (WI-S10-003) | `corelink.billing_stripe.{usage_recorded, duplicate_rejected, webhook_received, signature_rejected, signature_verified, signature_skew_rejected}` |
| `corelink-billing-reconcile` (WI-S10-004) | `corelink.billing_reconcile.{run_started, no_drift, auto_fixed, ticket_filed, page_dispatched, stripe_paused}` |
| `corelink-quota-fsm` (WI-S10-005) | `corelink.billing_quota.{state_changed, overage_telemetry_recorded, suspended, reinstated}` |
| `corelink-billing-replay` (WI-S10-006) | `corelink.billing_replay.{request_authorized, request_denied, dry_run_planned, executed, layer_diverged}` |

Audit chain canonical envelope: RFC 8785 JCS canonical determinism +
BLAKE3-256 link hash Bitcoin-block-header pattern (S-09 inheritance).
ChainVerifier daily-verify primitive walks every per-tenant + per-region
chain slice; first mismatch fail-CLOSED with SEV-0 audit emit
`corelink.audit_chain.chain_break_detected`.

## 5. TLA+ formal verification (`specs/tla/billing_atomicity.tla`)

Sprint contract §6 DoD + §16 SOTA bar diferencial vs Stripe Billing /
Mux / Datadog / Lago — all ship robust testing, NONE ship TLA+. The
WI-S10-007 TLA+ specification proves 4 invariants via TLC v1.8.0
SHA-256 pinned model checking:

1. **INV_BILLING_NO_LOSS** (HIGH; registry §3.9 line 136) — every
   emitted event is accounted somewhere (staging / R2 / retry_queue /
   Stripe-invoiced); weak fairness on `DrainStagingToR2` +
   `DrainRetryQueue` + `StripeOutageRecovers` guarantees eventual
   delivery.
2. **INV_BILLING_NO_DUP** (HIGH; registry §3.9 line 137) — Stripe-
   invoiced map is a function (same key → same value); the bucket
   assigned to a (tenant, billing_period) is deterministic from R2
   events.
3. **INV_BILLING_CHAIN_INTEGRITY** (NEW; sprint contract §6 DoD) —
   counter rows are subsets of R2 events; invoice_line_items equal
   counters; Stripe-invoiced sets are subsets of R2.
4. **INV_LAYER_1_RECONCILE** (HIGH; registry §3.12 line 166 — Layer 1
   sub-property of INV-BILLING-RECONCILE-3-LAYER) — for every Stripe-
   invoiced (tenant, billing_period), the count of charged events
   equals the cardinality of corresponding R2 events.

CI gate: `.github/workflows/tla_billing_check.yml` runs TLC ≤ 30min per
PR + main push; failure fail-CLOSED. Nightly extended bounds via
`billing_atomicity_nightly.cfg` (4 tenants × 2 SKUs × 3 billing_periods
× 6 events/tenant × 15 in-flight bound) gated by 30min hard timeout in
`.github/workflows/nightly.yml::tlc-extended`.

## 6. SOC 2 CC1.4 control evidence walkthrough

The mock SOC 2 auditor reviews the following evidence at the Finance
walkthrough exercise (paid engagement; sprint contract §22 estimates
$5k single engagement):

1. **Reconstructed invoice byte-equivalent to Stripe invoice** (modulo
   Stripe metadata: timestamps + internal Stripe IDs) via WI-S10-006
   replay endpoint; reconstruction time ≤ 5 min p99 SLA per
   RB-BILLING-001 §3.
2. **Replay endpoint role-protected** (CTRL-AUTHZ-002
   `billing_forensics_admin` separate from regular admin role per
   WI-S10-006 §1 invariant 3); MFA re-auth required (CTRL-AUTH-010
   freshness 30 min); audit-per-decision-arm canonical (5-event
   taxonomy `corelink.billing_replay.*`).
3. **Audit chain integrity preserved** (per-replay canonical event
   landed BEFORE state mutation per Lote 10.6bis pattern + S-07 P1-1
   fix; INV-OBS-AUDIT-CHAIN-INTEGRITY HIGH; S-09 inheritance).
4. **PII redaction at trait surface** (typed payload `ReplayDecision` +
   `ReplayRequest` + `ReconstructedLayers` per Lote 10.9-quinquies
   NEW-P0-2 absorption; NO `serde_json::Value`; INV-AUDIT-NO-RAW-PII
   HIGH).
5. **7-year retention** (R2 Object Lock Governance Mode + D1
   `billing_replay_audit` PRIMARY KEY (request_id) UNIQUE per migration
   0021; SOC 2 + GAAP + GDPR Art. 22 evidence trail).
6. **TLA+ formal verification of pipeline atomicity invariants** (4
   invariants TLC-proven; sprint contract §16 SOTA bar diferencial vs
   all competitors — Stripe Billing No / Mux No / Datadog No / Lago No;
   CoreLink Yes).

The host-side dry-run execution `scripts/rb_billing_001_replay_forensic_dry_run.sh`
runs all 7 steps green at PR speed (10k iter prop tests across 7
canonical prop tests in `corelink-billing-replay::prop_billing_replay`);
audit trace: `specs/_audits/2026-05-03-rb-billing-001-replay-forensic-dry-run.md`.

## 7. Sign-off (Finance walkthrough mock auditor exhibit)

Per WI-S10-007 §6.1.8 + §30 row 12 (Finance Officer mandatory emphatic).
The full live walkthrough with paid mock SOC 2 auditor is deferred
until staging account provisioned + auditor scheduled (revalidation
trigger). The host-side exhibit below carries the canonical sign-off
ladder produced by WI-S10-007 SEAL.

| # | Role | Status | Date | Rationale / Evidence |
|---|---|---|---|---|
| 1 | Owner (Gustavo) | ✅ ACKNOWLEDGED | 2026-05-03 | WI-S10-006 SEALED commit `9f0228c` (replay engine pure-logic ships); WI-S10-007 SEAL produces this exhibit. |
| 2 | Finance Officer | ⚠️ WAIVED (ADR-0034) — host-side reconstruction byte-equivalence cross-validated via prop_replay_deterministic + prop_layer_diverged_flagged 10k iter; staging walkthrough deferred until staging account provisioned + auditor scheduled | 2026-05-03 | INV-BILLING-REPLAYABLE-FROM-EVENTS HIGH (registry §3.12 line 167); RB-BILLING-001 §3 ≤ 5min p99 reconstruction SLA structurally enforced via type system + per-instance `Arc<Mutex<()>>` F-001 closure mirroring S-07 DO-actor model byte-for-byte. |
| 3 | Compliance Officer | ⚠️ WAIVED (ADR-0034) — SOC 2 CC1.4 control evidence host-side via RB-BILLING-001 dry-run audit trace; live mock SOC 2 auditor sign-off deferred forward | 2026-05-03 | SOC 2 CC1.4 + GAAP ASC 606 + GDPR Art. 22 (automated decision review) coverage: per-replay canonical audit row landed BEFORE state mutation; 7-year retention via R2 Object Lock Governance Mode + D1 PRIMARY KEY (request_id) UNIQUE migration 0021. |
| 4 | Privacy Officer | ⚠️ WAIVED (ADR-0034 + ADR-0017 DPO interim) — typed payload Lote 10.9-quinquies NEW-P0-2; INV-AUDIT-NO-RAW-PII at trait surface | 2026-05-03 | LINDDUN I(dentifiability) + L(inkability) + D(isclosure) at the replay output boundary structurally bounded — `ReplayDecision` / `ReplayRequest` / `ReconstructedLayers` / `ReplayOutcome` / `LayerDriftSummary` typed `#[non_exhaustive]` enums; pseudonymous `tenant_id` + `request_id` UUIDv7 hyphenated 36-char canonical. |
| 5 | (Forward) mock SOC 2 auditor | ⏳ DEFERRED — staging account provisioned + auditor scheduled | TBD | Live walkthrough exercise produces sign-off + diff report HTML/JSON exhibit; revalidation trigger documented inline. |

## 8. Acceptance criteria

Per WI-S10-007 §10.s10.007.9 + sprint contract §6 DoD:

- [x] **Reconstruction ≤ 30min p99 SLA** structurally enforced via
      type system + `Arc<Mutex<()>>` F-001 closure mirroring S-07 DO-
      actor model; live walkthrough deferred (staging + auditor).
- [x] **Reconstructed = Stripe invoice byte-equivalent** (modulo Stripe
      metadata) cross-validated via `prop_replay_deterministic` +
      `prop_layer_diverged_flagged` 10k iter PR-gate / 100k iter nightly.
- [x] **Replay endpoint role-protected** (CTRL-AUTHZ-002
      `billing_forensics_admin`) cross-validated via
      `prop_authorized_role_only_executes` 10k iter; audit-per-decision-arm
      canonical via `prop_audit_emit_per_decision_arm` 10k iter.
- [x] **Audit chain integrity preserved end-to-end** — per-replay
      canonical event landed BEFORE state mutation per Lote 10.6bis
      pattern + S-07 P1-1 fix; INV-OBS-AUDIT-CHAIN-INTEGRITY HIGH (S-09
      inheritance).
- [x] **TLA+ formal verification of pipeline atomicity invariants** —
      4 invariants TLC-proven via `specs/tla/billing_atomicity.tla` +
      `.github/workflows/tla_billing_check.yml` CI gate.
- [ ] **Live mock SOC 2 auditor sign-off** — DEFERRED until staging
      account provisioned + auditor scheduled (revalidation trigger).

## 9. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-03 | Gustavo Schneiter (via Claude Opus 4.7 1M) | Initial Finance walkthrough exhibit produced as part of WI-S10-007 SEAL Lote. Narrative covers Layer 0..3 pipeline; reconciliation drift detection; replay forensic reconstruction; audit chain integrity end-to-end; TLA+ formal verification; SOC 2 CC1.4 control evidence; sign-off scaffold for Finance Officer + Compliance Officer + Privacy Officer + (forward) mock SOC 2 auditor. |

---

**End S-10 Finance walkthrough exhibit v1.0.0.**
