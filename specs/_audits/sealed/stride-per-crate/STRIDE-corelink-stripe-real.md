# STRIDE deep dive — `corelink-stripe-real` (Stripe webhook ingestion + reconcile)

- **Crate:** `crates/corelink-stripe-real` (+ `corelink-billing-stripe`, `corelink-billing-reconcile`, `corelink-billing-replay`)
- **Date:** 2026-05-15
- **Owner:** Finance Engineering + Security Lead
- **Pentest scope:** Yes — engagement 2026-06-15 (P0 surface — money flow)
- **Coarse references:** matrix-stride-ctrl.csv THR-T-005 (billing tamper), THR-R-003 (CF dispute), and pentest §3.2
- **Pentest doc cross-ref:** §3.2 Billing (Stripe boundary)
- **SOC 2 cross-ref:** CC3.2 (financial risk), CC6.1 (logical access), CC7.1/7.2 (monitoring), A1.2 (commitments — billing accuracy)

## 1. Trust boundaries

| Boundary | Caller | Callee | Auth/authz | Output |
|---|---|---|---|---|
| **TB-stripe-1** | Stripe (external) → CF edge → Worker | `corelink-stripe-real::handle_webhook` | `Stripe-Signature` HMAC-SHA256 constant-time + timestamp window ±5 min | Idempotent event recorded in D1 + R2 NDJSON append-only |
| **TB-stripe-2** | Reconciler cron → Stripe API | `corelink-billing-reconcile` | Stripe API key (live mode, restricted scope); rotated quarterly | 3-layer diff: Stripe events ↔ D1 events ↔ Neon invoice ledger |
| **TB-stripe-3** | Replay endpoint (audit-grade) | `corelink-billing-replay::POST /v1/billing/replay` | PAT scope `billing:replay` (role-protected) + admin scope for cross-tenant | Reconstructed invoice byte-equal to original |
| **TB-stripe-4** | Outbound: CoreLink emits usage to Stripe (metered) | Stripe API (`v1/subscription_items/.../usage_records`) | API key + idempotency-key header | Stripe-side aggregation |

## 2. STRIDE per boundary

### 2.1 TB-stripe-1 (inbound webhook)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Webhook signature forge | HMAC-SHA256 constant-time compare; signing secret in CF Secrets per-environment | `crates/corelink-stripe-real/tests/webhook_signature.rs` — FM-BILLING-001 |
| **T** | Replay of valid webhook (replay-after-process) | Idempotent D1 dedup table (`stripe_event_id` PRIMARY KEY); nonce window 5 min; INV-BILLING-NO-DUP | `crates/corelink-stripe-real/tests/prop_webhook.rs` + `specs/tla/billing_atomicity.tla` — FM-BILLING-002 |
| **R** | Charge dispute "we never sent webhook" | INV-BILLING-RECONCILE-3-LAYER daily; CF Logpush retains request log; audit chain entry on receive | INV-OBS-AUDIT-CHAIN-INTEGRITY |
| **I** | Customer billing state leak across tenants | Tenant-scoped routes; INV-TENANT-ISOLATION; RLS-analogue at Worker | `specs/tla/tenant_isolation.tla` — FM-BILLING-004 |
| **D** | Webhook flood | CF rate limit + Stripe-side delivery throttle; idempotent dedup absorbs retries; DLQ for failed events | `crates/corelink-stripe-real/tests/prop_dlq.rs` + k6 — FM-BILLING-005 |
| **E** | Webhook is read-only ingress — no API mutation surface exposed | n/a (design) | n/a |

### 2.2 TB-stripe-2 (reconciler)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Reconciler impersonated to inject false-positive "all reconciled" | Reconciler runs in CF Worker with scoped IAM; result signed and chain-anchored | INV-OBS-AUDIT-CHAIN-INTEGRITY |
| **T** | Drift threshold table mutated to mask fraud | Threshold in code (CTRL-SUPPLY-002 signed deploy); INV-BILLING-RECONCILE-3-LAYER per spec_contract S-10 §14.s10.1 (drift > 0.1% SEV-2, > 1% SEV-1 + auto-pause Stripe) | `crates/corelink-billing-reconcile/tests/` |
| **R** | "Reconciler never ran on day X" | Cron heartbeat dead-man switch; missed run = SEV-1 page | RB-FM-302-billing-drift-dry-run |
| **I** | Stripe API key leaks via reconciler logs | CTRL-CRED-001 no-secret-in-log lint; key in CF Secrets; never serialized | `tools/pat_plaintext_lint/` |
| **D** | Stripe API rate limit during reconcile blocks close-of-month | Pagination + backoff (CTRL-BACKOFF-001); pre-flight estimate; SEV-2 if not done in budget | FM-151 (Stripe outage) + RB-FM-151-stripe-outage-dry-run |
| **E** | Reconciler key escalates to non-billing Stripe APIs | Stripe restricted key — billing scope only; quarterly key audit (CTRL-CRYPTO-003) | quarterly review |

### 2.3 TB-stripe-3 (replay)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Unauthorized invoice reconstruction | PAT scope `billing:replay` (role-protected); admin scope for cross-tenant | `crates/corelink-billing-replay/tests/` |
| **T** | Replay returns mutated invoice (cover up fraud) | INV-BILLING-REPLAYABLE-FROM-EVENTS — byte-equal to original event stream; monthly CI test | INV-BILLING-APPEND-ONLY |
| **R** | "Replay output was wrong" — auditor dispute | Replay deterministic from R2 NDJSON event stream + JCS canonicalization | property test |
| **I** | Replay exposes other tenants' line items | Tenant-scoped query; admin scope subject to dual-approval audit | INV-TENANT-ISOLATION |
| **D** | Mass replay storm | Per-PAT rate limit + cost-meter | k6 |
| **E** | Replay endpoint used to forge new invoice (write path) | Read-only contract enforced; no mutation API on this surface | API surface review |

### 2.4 TB-stripe-4 (outbound usage)

| STRIDE | Attacker scenario | Control / invariant | Test coverage |
|---|---|---|---|
| **S** | Compromised worker emits usage for wrong customer | tenant_id → Stripe `customer_id` mapping derived from D1 (single SoT); INV-TENANT-ISOLATION | `crates/corelink-billing-emit/tests/` |
| **T** | Usage record tampered (inflate revenue) | Idempotency-key = BLAKE3 of canonical `(tenant, period, seq)`; INV-BILLING-NO-DUP; INV-BILLING-APPEND-ONLY at R2 sink | `specs/tla/billing_atomicity.tla` |
| **R** | "We were billed for usage we didn't generate" | R2 event log byte-replayable to invoice; audit chain entry | INV-BILLING-REPLAYABLE-FROM-EVENTS |
| **I** | Customer usage leaks via emit logs | CTRL-PRIV-001 + per-tenant log isolation | LINDDUN review |
| **D** | Stripe API rate limit blocks emit | Queue with durable + retry (PAT-QUEUE-EVENTS-001) per FM-151 | RB-FM-151 dry-run |
| **E** | Outbound emit escalates to billing-config mutate | Restricted API key (usage scope only) | quarterly key audit |

## 3. Residual risks

| RR-id | Description | Severity | Mitigation status |
|---|---|---|---|
| RR-STRIPE-01 | Live integration test (`live_integration.rs`) gated behind env var, runs only in staging | LOW | Documented; CI matrix covers prop_webhook + prop_dlq |
| RR-STRIPE-02 | Stripe API outage > 24h forces queue-and-deferred emit (FM-151) | LOW | RB-FM-151 dry-run validated; queue capacity sized |
| RR-STRIPE-03 | Webhook DLQ items require operator action (manual triage) | MEDIUM | Documented in oncall runbook; dashboard SLI on DLQ depth |

## 4. Adversarial test pointers

- `crates/corelink-stripe-real/tests/webhook_signature.rs` — sig forge (FM-BILLING-001)
- `crates/corelink-stripe-real/tests/prop_webhook.rs` — replay/idempotency (FM-BILLING-002)
- `crates/corelink-stripe-real/tests/prop_dlq.rs` — DLQ behavior (FM-BILLING-005)
- `crates/corelink-stripe-real/tests/live_integration.rs` — staging-only
- `specs/tla/billing_atomicity.tla` — formal billing model
- `specs/_audits/2026-05-03-rb-billing-001-replay-forensic-dry-run.md`
- `specs/_audits/2026-05-03-rb-fm-151-stripe-outage-dry-run.md`
- `specs/_audits/2026-05-03-rb-fm-302-billing-drift-dry-run.md`

## 5. Cross-references

- Invariants: INV-BILLING-NO-LOSS, INV-BILLING-NO-DUP, INV-BILLING-APPEND-ONLY, INV-BILLING-RECONCILE-3-LAYER, INV-BILLING-REPLAYABLE-FROM-EVENTS, INV-TENANT-ISOLATION, INV-AUDIT-APPEND-ONLY
- Controls: CTRL-BILLING-001, CTRL-AUTH-001/004, CTRL-CRED-001, CTRL-BACKOFF-001, CTRL-RATE-001
- Failure modes: FM-BILLING-001..005 (pentest §3.2), FM-151 (Stripe outage), FM-302 (billing drift)
- SOC 2: CC3.2, CC6.1, CC7.1/7.2, A1.2
