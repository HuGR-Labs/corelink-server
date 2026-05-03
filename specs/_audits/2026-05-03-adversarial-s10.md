---
id: "AUDIT-2026-05-03-ADVERSARIAL-S10"
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
tags: ["audit", "adversarial-review", "s10", "billing", "usage-metering", "stripe", "reconciliation", "quota-fsm", "billing-replay", "tla-plus", "wi-s10-007"]
---

# Adversarial review summary — S-10 implementation

> **Sprint:** S-10 · **WI:** WI-S10-007 §6.1 + sprint contract §15 · **Mode:** internal aggregation across per-WI implementation rounds + cumulative pre-PRR sweep

This document aggregates 12 adversarial scenarios catalogued across
WI-S10-001..006 implementation rounds + WI-S10-007 ship-gate
artifacts. Per the 2026-04-30 protocol shift, no per-WI codex was run
— sprint-close Sonnet review (one round of `general-purpose` agent
with `model: sonnet` per charter) covers the full S-10 corpus AFTER
WI-S10-007 SEALs. This audit captures the cumulative adversarial
trace at SEAL time.

## 0. Scope

S-10 implementation scope:

- WI-S10-001 — Usage event emitter + R2 append-only NDJSON +
  IdempotencyTracker BLAKE3+JCS idem_key (commit `a98edf9`)
- WI-S10-002 — Counter aggregator counter cron DO + BLAKE3 hash
  chain + JCS canonical aggregate bytes (commit `fadc8d1`)
- WI-S10-003 — corelink-billing-stripe adapter + Idempotency-Key +
  HMAC-SHA256 webhook signature verify (5min skew) +
  constant-time compare (commit `1a3c077`)
- WI-S10-004 — corelink-billing-reconcile 3-layer drift detector +
  dual-condition auto-fix + SEV-1-auto-pause-Stripe (commit `6752f18`)
- WI-S10-005 — corelink-quota-fsm 5-state quota machine +
  idempotent transitions + 3-invoice-failure suspension + S-13 email
  defer (commit `9a5ca71`)
- WI-S10-006 — corelink-billing-replay forensic engine +
  billing_forensics_admin RBAC + idempotent UUIDv7 + append-only
  audit trail (commit `9f0228c`)
- WI-S10-007 — TLA+ `billing_atomicity` + 3 RB-FM dry-runs +
  Finance walkthrough + PRR-S10 HIGH_RISK 12-sign-off ship gate
  (this Lote)

## 1. Adversarial scenarios (cumulative)

### 1.1 Idempotency + duplicate suppression (WI-S10-001 + WI-S10-003)

1. **Idempotency-key collision via crafted CloudEvent payload**
   (intentional double-charge attempt). Outcome: structurally
   impossible — `derive_idem_key` is BLAKE3-256 of canonical
   RFC 8785 JCS bytes with the idem_key slot zeroed; same canonical
   payload → same key by construction; Stripe-side
   `stripe_idempotency_keys` PRIMARY KEY (idempotency_key) UNIQUE +
   internal `usage_event_staging` PRIMARY KEY (tenant_id,
   request_id) UNIQUE both block re-insert at storage layer.
   `prop_idempotency_key_deterministic` + `prop_idempotency_key_diverges_per_aggregate`
   10k iter pin canonical determinism + canonical-bytes-divergence
   surface (`StripeUsageLedgerError::IdempotencyKeyReuse` SEV-1
   forensic-anomaly signal).
2. **Replay-attack via Stripe webhook signature stale timestamp /
   tampered payload / tampered signature**. Outcome: canonical
   `REPLAY_WINDOW_MS = 300_000` boundary enforcement per Stripe
   webhook signature documentation; outside → `signature_skew_rejected`
   audit emit; inside → constant-time HMAC-SHA256 compare via
   `subtle::ConstantTimeEq::ct_eq` (timing-attack defense).
   `prop_webhook_signature_rejects_expired` +
   `prop_webhook_signature_rejects_tampered_payload` +
   `prop_webhook_signature_rejects_tampered_signature` +
   `prop_constant_time_signature_compare` +
   `prop_replay_window_exact_5min_boundary` 10k iter pin reject
   discipline.

### 1.2 Audit chain integrity (WI-S10-002)

3. **Billing aggregate hash chain break via canonical-bytes
   mutation** (RFC 8785 JCS non-determinism on
   `AggregatedCounter`; INV-OBS-AUDIT-CHAIN-INTEGRITY violation).
   Outcome: `compute_canonical_bytes` RFC 8785 JCS via serde_jcs +
   BLAKE3-256 link hash Bitcoin-block-header pattern (S-09
   inheritance); idempotent re-run check on typed `data` payload
   BEFORE chain-head observation so watermark replay never advances
   the head twice; `AggregatedCounterStore::DigestMismatch` SEV-1
   replay-corruption surface; per-(tenant, billing_period,
   event_kind) UNIQUE PK at storage layer.
4. **Late-arriving event > 6h backfill silently mutates
   prior-period counter** (revenue shift across reporting boundary).
   Outcome: `usage_counter_late` separate D1 table for events with
   `ts < now - 6h` per sprint contract §5.2 R-S10-5 + WI-S10-002
   `LateCounterRecord` typed split; `cron_now` reference time
   canonical (R5 NEW-P1-1 fix); SEV-3 alert sustained 5min.

### 1.3 Reconciliation 3-layer drift detection (WI-S10-004)

5. **Drift threshold-ladder boundary discipline calibration drift**
   (4-tier ladder NoDrift / AutoFixed / TicketSev3 / PageSev2 /
   PageSev1AutoPaused silently regresses recall at 0.1% / 1%
   boundaries). Outcome: canonical 4-tier ladder constants
   `QUIET_THRESHOLD = 0.0001` / `SEV3_TO_SEV2_THRESHOLD = 0.001` /
   `SEV2_TO_SEV1_THRESHOLD = 0.01` per sprint contract §14.s10.1
   zero-tolerance; auto-fix dual-condition gate carved INSIDE Quiet
   tier (drift > 0 AND `count ≤ 5 AND pct ≤ 0.0001`).
   `prop_drift_threshold_boundaries` +
   `prop_auto_fix_dual_condition_gate` +
   `prop_layer3_drift_pages_sev1_pauses_stripe` 10k iter pin every
   boundary + Layer 3 SEV-1 routing.
6. **Reconcile re-run on same period silently double-counts the
   drift event** (idempotency-on-(tenant, billing_period, run_started_at)
   PK violation). Outcome: `billing_reconciliation_drift` PRIMARY KEY
   (tenant_id, billing_period, run_started_at) UNIQUE [INV-BILLING-NO-DUP
   storage layer; 7-year SOC 2 CC1.4 + GAAP ASC 606 evidence trail];
   `prop_idempotent_rerun_same_period` 10k iter pins re-run idempotency.

### 1.4 Quota state machine (WI-S10-005)

7. **Quota grace-period bypass via simultaneous state transitions
   from concurrent webhook fires** (Stripe `invoice.payment_failed`
   delivered twice in same period inflates `invoice_failure_count`
   → premature suspension). Outcome: per-tenant
   `quota_fsm_state` PRIMARY KEY (tenant_id) UNIQUE +
   `prop_idempotent_transition_no_change` +
   `prop_3_invoice_failures_suspends` 10k iter pin canonical
   3-invoice-failure suspension + idempotent re-fire discipline; F-001
   `Arc<Mutex<()>>` per-instance closure mirroring S-07
   `corelink-quota::check.rs` DO-actor model byte-for-byte.
8. **Operator-driven reinstate misses counter clear** (suspended
   tenant reinstated but `invoice_failure_count` retains stale value
   → next invoice failure prematurely re-suspends). Outcome:
   `prop_reinstate_clears_suspension` 10k iter pins the operator-driven
   reinstate-clears-counter arm; canonical
   `corelink.billing_quota.reinstated` audit emit BEFORE store UPSERT
   per fail-CLOSED Lote 10.6bis pattern.

### 1.5 Billing replay forensic engine (WI-S10-006)

9. **Replay forensic role-escalation via crafted `presented_role`
   header** (CTRL-AUTHZ-002 `billing_forensics_admin` bypass attempt).
   Outcome: `prop_authorized_role_only_executes` 10k iter pins the
   role-check arm at the orchestrator boundary; canonical
   `BILLING_FORENSICS_ADMIN_ROLE = "billing_forensics_admin"` separate
   from regular admin role per WI-S10-006 §1 invariant 3
   (least-privilege-bounded audit-grade replay capability);
   canonical `corelink.billing_replay.request_denied` audit emit
   BEFORE Denied403 return per fail-CLOSED envelope.
10. **Cross-tenant request_id reuse** (replay request from tenant A
    re-uses request_id from tenant B → idempotency ledger collision +
    forensic-trail tampering). Outcome: structurally impossible —
    `InMemoryReplayEngine` per-instance `Arc<Mutex<()>>` F-001
    closure carries per-tenant ledger isolation;
    `prop_tenant_isolation` 10k iter pins INV-TENANT-ISOLATION
    (CRITICAL, TLA+ proven S-01) at the replay engine boundary;
    `billing_replay_audit` PRIMARY KEY (request_id) UNIQUE +
    `DivergentPayload` SEV-1 tampering signal surface flag any
    canonical-payload divergence under same request_id.

### 1.6 TLA+ formal verification (WI-S10-007)

11. **TLA+ counter-example violating INV-BILLING-NO-LOSS via Stripe
    outage queue + late-arriving event interaction** (events emitted
    during outage join retry_queue; queue drain race with late-event
    backfill loses one event from the canonical accounting bucket).
    Outcome: `billing_atomicity.tla` proves `INV_BILLING_NO_LOSS`
    state-machine invariant under the bounded model (5 tenants × 5
    SKUs × 24 hours × 5 regions × MaxEventsPerHour=10 = 30,000
    Cartesian product upper bound CONSTANTS); weak fairness on
    `DrainRetryQueue` + `StripeOutageRecovers` actions guarantees
    eventual delivery; TLC SHA-256 supply-chain pin canonical
    `d5d07d5dab38ddb840c91ec48fa02f28b37a608d5af9a73570018591dbc8ef7f`
    per ADR-0042 §A1 enforced by
    `.github/workflows/tla_billing_check.yml`. Sprint contract §16
    SOTA bar diferencial vs todos competitors (Stripe Billing No,
    Mux No, Datadog No, Lago No).
12. **TLA+ specification drift from production code** (model
    out-of-sync; refactor in WI-S10-002 counter aggregator without
    matching TLA+ spec update → false-confidence verification).
    Outcome: `.github/workflows/tla_billing_check.yml` PR-trigger
    surface includes all 6 S-10 crates + 5 migrations
    (0017..0021) + the spec contract + the data_model.md +
    invariant_registry.md; any change to those paths re-runs TLC
    against the bounded model + nightly model; drift detected at
    PR-time before merge.

## 2. Cumulative invariant interaction matrix

| Invariant | Source WI | Closure WI | Cross-validation |
|---|---|---|---|
| INV-BILLING-NO-LOSS (HIGH; registry §3.9 line 136) | inherited canonical | WI-S10-007 (TLA+ TLC formal proof + RB-FM-151 dry-run) | TLA+ `billing_atomicity` `INV_BILLING_NO_LOSS` state-machine invariant; `prop_billing_emit` + `prop_billing_stripe` 10k iter; PAT-QUEUE-EVENTS-001 fallback queue weak-fairness modeling |
| INV-BILLING-NO-DUP (HIGH; registry §3.9 line 137) | inherited canonical | WI-S10-007 (TLA+ TLC formal proof) | TLA+ `INV_BILLING_NO_DUP` state-machine invariant; Idempotency-Key UNIQUE PK at `stripe_idempotency_keys` storage layer; `prop_idempotency_key_deterministic` + `prop_idempotency_key_diverges_per_aggregate` |
| INV-BILLING-RECONCILE-3-LAYER (HIGH; registry §3.12 line 166) | WI-S10-004 | WI-S10-007 (RB-FM-302 dry-run + TLA+ Layer-1 sub-property) | 4-tier drift threshold ladder + dual-condition auto-fix gate + SEV-1 Layer-3 routing pauses Stripe; `prop_drift_threshold_boundaries` + `prop_auto_fix_dual_condition_gate` + `prop_layer3_drift_pages_sev1_pauses_stripe` |
| INV-BILLING-REPLAYABLE-FROM-EVENTS (HIGH; registry §3.12 line 167) | WI-S10-006 | WI-S10-007 (RB-BILLING-001 dry-run + Finance walkthrough exhibit) | `prop_replay_deterministic` + `prop_layer_diverged_flagged` + `prop_idempotent_replay_same_request_id` 10k iter; per-(request_id) UNIQUE PK at `billing_replay_audit` storage layer; canonical 5-event `corelink.billing_replay.*` taxonomy |
| INV-AUDIT-APPEND-ONLY (CRITICAL, TLA+; inherited S-06) | inherited S-06 | every S-10 WI | R2 PutObject + Object Lock Governance Mode 7y retention (production binding deferred per `trait-abstraction-defer`); host-side `InMemoryR2UsageSink` enforces append-only at trait surface |
| INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH; lift from S-07 P1-1 fix) | inherited S-07 | every S-10 WI | audit emit BEFORE state mutation per fail-closed envelope on every decision arm across 6 crates; `prop_audit_emit_per_decision_arm` 10k iter cumulative |
| INV-TENANT-ISOLATION (CRITICAL, TLA+; inherited S-01) | inherited S-01 | every S-10 WI | `prop_tenant_isolation` per-WI 10k iter cumulative; per-instance `Arc<Mutex<()>>` F-001 closure across all 6 crates; cross-tenant request_id / idempotency-key reuse impossible by construction |
| INV-OBS-AUDIT-CHAIN-INTEGRITY (HIGH; new §3.12 from S-09) | inherited S-09 | WI-S10-002 (BLAKE3+JCS) + WI-S10-006 (per-replay chain extension) | RFC 8785 JCS canonical determinism + BLAKE3-256 link hash Bitcoin-block-header pattern; `compute_canonical_bytes` + `link_chain_hash` + `verify_chain_link` primitives |
| CTRL-BILLING-001 (financial integrity; security_model.md) | inherited canonical | every S-10 WI | TLA+ formal verification + 3-layer reconcile + replay forensic + SOC 2 CC1.4 + GAAP ASC 606 + GDPR Art. 32 + LGPD Art. 32 evidence trail at every storage layer |
| CTRL-AUTHZ-002 (least-privilege; security_model.md) | inherited canonical | WI-S10-006 (`billing_forensics_admin` separate role) | canonical `BILLING_FORENSICS_ADMIN_ROLE = "billing_forensics_admin"` separated from regular admin role; `prop_authorized_role_only_executes` 10k iter |

## 3. Sign-off

| Role | Status | Date |
|---|---|---|
| Owner | ✅ ACKNOWLEDGED | 2026-05-03 |
| Engineer (lead) | ✅ APPROVED | 2026-05-03 |
| Architect (specialization for INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP TLA+ formal-verification + RFC 8785 JCS canonical determinism + atomic CAS race-correctness + BLAKE3 hash chain + Stripe HMAC-SHA256 constant-time signature compare) | ⚠️ WAIVED (ADR-0034 dual-hat) | 2026-05-03 |
| Compliance Officer | ⚠️ WAIVED (ADR-0034) — TLA+ TLC verification + RB dry-runs + Finance walkthrough host-side cross-validate SOC 2 CC1.4 + GAAP ASC 606 control evidence | 2026-05-03 |
| Finance Officer | ⚠️ WAIVED (ADR-0034) — INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP + INV-BILLING-RECONCILE-3-LAYER + INV-BILLING-REPLAYABLE-FROM-EVENTS cross-validated via 6-crate prop suite + TLA+ TLC formal proof; mock SOC 2 auditor engagement deferred until staging account provisioned | 2026-05-03 |

## 4. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-03 | Gustavo Schneiter (via Claude Opus 4.7 1M) | Initial S-10 adversarial review aggregation (WI-S10-007 SEAL Lote). 12 scenarios catalogued across WI-S10-001..007; cumulative invariant interaction matrix; zero HIGH/CRITICAL findings during S-10 implementation. |

---

**End S-10 adversarial review v1.0.0.**
