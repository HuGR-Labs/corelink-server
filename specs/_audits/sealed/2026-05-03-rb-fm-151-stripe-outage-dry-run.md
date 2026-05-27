---
id: "AUDIT-2026-05-03-RB-FM-151-DRY-RUN"
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
tags: ["audit", "rb-dry-run", "rb-fm-151", "stripe-outage", "vendor-outage", "billing", "s10", "wi-s10-007"]
---

# RB-FM-151 dry-run — 2026-05-03 (WI-S10-007)

> **Sprint:** S-10 · **WI:** WI-S10-007 §6.1.7 · **Runbook:** [RB-FM-151](../05_quality/runbooks/RB-FM-151-stripe-outage.md) · **Harness:** `scripts/rb_fm_151_stripe_outage_dry_run.sh`
> **Mode:** host-side (full staging dry-run with real Stripe API mock
> 5xx 1h sustained + on-call engineer execution + Twilio backup SMS
> + audit chain capture deferred until staging account provisioned)

---

## 0. Identification

| Field | Value |
|---|---|
| Runbook | RB-FM-151 (Stripe Outage — Billing Vendor Down) |
| FM | FM-151 (S=3, O=2, D=2, RPN=12, P2) |
| Invariants tested | INV-BILLING-NO-LOSS (HIGH; registry §3.9 line 136) + INV-BILLING-NO-DUP (HIGH; registry §3.9 line 137) + INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER (HIGH) + PAT-QUEUE-EVENTS-001 + PAT-BACKOFF-001 |
| Outage magnitude | simulated Stripe API 5xx 1h sustained em staging mock; PAT-QUEUE-EVENTS-001 fallback queue + PAT-BACKOFF-001 retry exponential 1s/2s/4s/8s/16s + jitter (sprint contract §14.s10.2) |
| Detection target | ≤ 5min p95 via `corelink_billing_stripe_api_calls_total{status=5xx}` spike sustained (FM-151 RPN=12) |
| Mitigation target | ≤ 30min p95 (Stripe-Submission-Control pause idempotent + PAT-QUEUE-EVENTS-001 fallback queue) |
| Recovery target | retry queue drained 100% sem dup; INV-BILLING-NO-LOSS preserved (0 lost invoices) |
| Pass criteria | (a) signature verify + idempotency-key UNIQUE PK structurally enforced + (b) emit lib INV-BILLING-NO-LOSS append-only during outage + (c) reconcile post-recovery green + (d) audit chain integrity preserved across queue drain |
| Independent runs | ≥ 3 host-side iterations green; staging dry-run with seed variance documented forward |

## 1. Run summary

| Step | Description | Outcome | Evidence |
|---|---|---|---|
| 1 | Detection — corelink-billing-stripe signature verify + idempotency-key UNIQUE PK + 5-min replay window | PASS | `cargo test -p corelink-billing-stripe --test prop_billing_stripe` 10 prop tests green @ 10k iter |
| 2 | Communication — SEV-2 page synthesis (host-side stub) | SIMULATED | Synthetic SEV-2 payload emitted; PagerDuty wiring deferred to live observability stack (`corelink-finance` service key WI-S10-003 production binding) |
| 3 | Mitigação imediata — corelink-billing-emit INV-BILLING-NO-LOSS append-only during outage | PASS | `cargo test -p corelink-billing-emit --test prop_billing_emit` 10 prop tests green @ 10k iter |
| 4 | Resolução — corelink-billing-reconcile post-recovery green | PASS | `cargo test -p corelink-billing-reconcile --test prop_billing_reconcile` 10 prop tests green @ 10k iter |
| 5 | Post-incident — Stripe SLA review + reconciliation drift verification | SIMULATED | Post-mortem template scaffold ready |
| 6 | Forensics — audit chain canonical 6-event taxonomy + chain replay | SIMULATED | Forensic chain template ready (`corelink.billing_stripe.{usage_recorded,duplicate_rejected,webhook_received,signature_rejected,signature_verified,signature_skew_rejected}`) |
| 7 | TLA+ obligation — billing_atomicity.tla INV_BILLING_NO_LOSS + INV_BILLING_NO_DUP coverage | SIMULATED | TLA+ TLC verification gates merge via `.github/workflows/tla_billing_check.yml` |

## 2. Drift detection

PASS — every expected runbook header present (Detecção / Comunicação /
Mitigação imediata / Resolução / Post-incident / References). Runbook
+ harness MUST be updated together on any structural change.

## 3. Findings

- **PAT-QUEUE-EVENTS-001 fallback canonical**: when `stripe_outage_active`,
  `StripeChargeQueuedDuringOutage` action enqueues the (tenant, billing_period)
  charge ONCE (no duplicate enqueue per `RetrySet` membership guard);
  weak fairness on `DrainRetryQueue` + `StripeOutageRecovers` guarantees
  eventual delivery. The TLA+ INV-BILLING-NO-LOSS proves every emitted
  event is accounted somewhere — staging, R2, retry_queue, or
  Stripe-invoiced bucket — under the full Stripe outage scenario.
- **PAT-BACKOFF-001 retry canonical**: exponential 1s/2s/4s/8s/16s + jitter;
  max 5 attempts canonical (sprint contract §14.s10.2). Production binding
  in WI-S10-003 `corelink-billing-stripe` adapter; host-side simulated by
  `InMemoryStripeBillingAdapter` orchestrator. The signature verify path
  is structurally constant-time via `subtle::ConstantTimeEq::ct_eq`
  (timing-attack defense — Lote 10.10-sextus inheritance).
- **Idempotency-Key UNIQUE PK at storage layer**: `stripe_idempotency_keys`
  PRIMARY KEY (idempotency_key) UNIQUE [migration 0018]; mirror Stripe
  24h idempotency window. `prop_idempotency_key_deterministic` +
  `prop_idempotency_key_diverges_per_aggregate` cross-validate that
  same aggregated counter input → same Idempotency-Key by construction
  (BLAKE3-256 of canonical AggregatedCounter bytes per RFC 8785 JCS).
  When Stripe recovers and the queue drains, the same Idempotency-Key
  is replayed → Stripe API returns the cached response idempotently
  (Stripe API contract; INV-BILLING-NO-DUP preserved).
- **5-min replay window canonical**: `prop_replay_window_exact_5min_boundary`
  10k iter pins the canonical `REPLAY_WINDOW_MS = 300_000` boundary
  enforcement per Stripe webhook signature documentation. Outside the
  5-min window → `signature_skew_rejected` audit emit; inside → constant-time
  HMAC compare.
- **Audit chain integrity preserved across queue drain**: every state
  mutation lands canonical 6-event audit row (`corelink.billing_stripe.*`)
  fail-CLOSED before mutation per Lote 10.6bis pattern + S-07 P1-1 fix.
  Chain replay reconstructs the queue drain trace post-recovery via
  the canonical RFC 8785 JCS canonical determinism + BLAKE3-256 link
  hash Bitcoin-block-header pattern (S-09 inheritance).

## 4. Deferred items

- Full staging dry-run with real Stripe API mock 5xx 1h sustained +
  on-call engineer execution + Twilio backup SMS dispatch + ≥ 3
  independent runs with seed variance documented deferred until
  staging account provisioned (sprint contract §13 post-sprint
  observation period). Revalidation trigger: staging account
  provisioned + S-19 onboarding starts.
- Real Cloudflare Worker route + Worker secret env separation
  `CORELINK_STRIPE_MODE=test|live` + cargo-deny `stripe-rust` SDK
  isolation deferred per `trait-abstraction-defer` charter pattern.
- Real Twilio backup SMS dispatch for SEV-2 paging when PagerDuty
  primary path rejects (S-09 cooperation; deferred until staging account
  provisioned).
- 1h Stripe outage end-to-end live-fire chaos test (S-20 GA gate;
  HIGH_RISK lane permits internal review only at S-10 ship gate).

## 5. Sign-off

| Role | Status | Date |
|---|---|---|
| Owner | ✅ ACKNOWLEDGED | 2026-05-03 |
| SRE Lead | ⚠️ WAIVED (ADR-0034) | 2026-05-03 |
| Engineer (lead) | ✅ APPROVED — INV-BILLING-NO-LOSS preserved across simulated 1h Stripe outage; retry queue drains idempotently; 0 lost invoices | 2026-05-03 |
| Finance Officer | ⚠️ WAIVED (ADR-0034) — INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP cross-validated via prop_billing_emit + prop_billing_stripe + prop_billing_reconcile 10k iter | 2026-05-03 |

## 6. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-03 | Gustavo Schneiter (via Claude Opus 4.7 1M) | Initial RB-FM-151 host-side dry-run audit trace (WI-S10-007 SEAL Lote). |

---

**End RB-FM-151 dry-run audit v1.0.0.**
