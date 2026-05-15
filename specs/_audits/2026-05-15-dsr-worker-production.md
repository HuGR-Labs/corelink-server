---
id: "AUDIT-2026-05-15-DSR-WORKER-PRODUCTION"
type: "audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "dsr", "privacy", "erasure", "s11", "wi-s11-002", "sli-binding", "10-backend-matrix"]
---

# DSR Erasure Worker — Production Readiness Evidence — 2026-05-15

> **Purpose:** consolidate the production-readiness evidence for the
> S-11 / WI-S11-002 DSR erasure worker shipped at v1.2.0 (SEALED,
> 2026-05-07). Pin the canonical 12-backend coverage matrix
> (privacy_model.md §6.2 source-of-truth pós Lote 10.11.0-bis: 8
> effective + 4 pseudonymized), the test count, the jurisdictional
> report surface (LGPD / GDPR / CCPA), the SLI binding closure
> (`Sli::FreshDsrErasure` ↔ `corelink_dsr_resolution_hours`), and the
> status-page integration deferral rationale.
>
> **Scope:** the `crates/corelink-privacy-erasure-worker` crate +
> `crates/corelink-privacy-pseudonymize` helper + `crates/corelink-dsr`
> API surface + `tests/e2e-dsr` harness. Production CF Worker queue
> consumer + Neon / R2 / KV / DO / Stripe / Loki bindings + 24h cron
> worker wiring deferred to WI-S11-008 PRR ship gate per the canonical
> `trait-abstraction-defer` charter pattern.

## 1. Methodology

1. Re-verify the canonical 12-backend taxonomy
   (`event::BackendKind` 12-arm `#[non_exhaustive]` enum) against
   privacy_model.md §6.2 source-of-truth.
2. Re-verify the 5-arm canonical CloudEvents taxonomy
   (`event::ErasureCloudEventType`) against
   `schemas/cloudevents/dsr-erasure-*.v1.json`.
3. Re-verify each `BackendErasureAdapter` impl exists for all 12
   canonical backends (8 effective + 4 pseudonymized).
4. Re-run the `corelink-privacy-erasure-worker` test surface, the
   `e2e-dsr` integration harness, the `corelink-dsr` API surface
   tests, and the `corelink-slo` taxonomy tests.
5. Pin the SLI binding between the production cron worker
   (`METRIC_DSR_RESOLUTION_HOURS`) and the canonical
   `Sli::FreshDsrErasure.prometheus_metric_base()` via a cross-crate
   regression test (`tests/sli_binding.rs`).
6. Document the status-page integration surface (WI-S11-002 §6
   `/v1/status` widget) deferral to WI-S11-008 PRR ship gate.

## 2. 12-Backend coverage matrix (canonical pós Lote 10.11.0-bis)

The canonical 12-backend taxonomy lives at
`crates/corelink-privacy-erasure-worker/src/event.rs::BackendKind` —
12-arm `#[non_exhaustive]` enum, 8 effective + 4 pseudonymized per
privacy_model.md §6.2 source-of-truth.

| # | BackendKind arm | Slot | Adapter source | Outcome under erasure | Adapter test |
|---|---|---|---|---|---|
| 1 | `NeonMain` | Effective | `src/backends/neon_main.rs` | `Erased` (SQL `DELETE WHERE subject_user_id`; tombstone in `dsr_erasure_log`) | `chaos_per_backend_failure.rs` + `integration_erasure_lifecycle.rs` |
| 2 | `NeonBilling` | Effective | `src/backends/neon_billing.rs` | `Erased` outside `legal_hold`; `Pseudonymized` inside `legal_hold` (LGPD Art. 16 5y fiscal retention) | `chaos_per_backend_failure.rs` + `regression_stripe_invoice_preserved.rs` |
| 3 | `R2Cas` | Effective | `src/backends/r2_cas.rs` | `Erased` for `subject_dedicated`; refcount-decrement for `subject_unaffiliated` (S-07 dedup safety) | `regression_refcount_aware_scrub.rs` + `chaos_per_backend_failure.rs` |
| 4 | `R2Ac` | Effective | `src/backends/r2_ac.rs` | `Erased` (`DELETE` entries `WHERE owner_tenant_id`) | `chaos_per_backend_failure.rs` |
| 5 | `D1` | Effective | `src/backends/d1.rs` | `Erased` (subject-scoped row delete; refcount sync with R2) | `chaos_per_backend_failure.rs` |
| 6 | `Kv` | Effective | `src/backends/kv.rs` | `Erased` (`DELETE` keys matching tenant + subject prefix) | `chaos_per_backend_failure.rs` |
| 7 | `Stripe` | Effective | `src/backends/stripe.rs` | `Pseudonymized` (`Customer.update` PII nullify — NOT `Customer.delete`; GAAP ASC 606 + LGPD Art. 16 fiscal preservation) | `regression_stripe_invoice_preserved.rs` |
| 8 | `Loki` | Effective | `src/backends/loki.rs` | `Erased` (`/loki/api/v1/delete` API + retention compaction trigger) | `chaos_per_backend_failure.rs` |
| 9 | `R2AuditPseudo` | Pseudonymized | `src/backends/r2_audit_pseudo.rs` | `Pseudonymized` (HKDF info=`corelink/v1/audit-pseudonym`; Object Lock 7y immutable) | `prop_pseudonymization_correctness.rs` |
| 10 | `NeonPitrPseudo` | Pseudonymized | `src/backends/neon_pitr_pseudo.rs` | `Pseudonymized` (tombstone replay on PITR restore; 30d auto-rotation) | `prop_pseudonymization_correctness.rs` |
| 11 | `R2CasLegalHoldPseudo` | Pseudonymized | `src/backends/r2_cas_legalhold_pseudo.rs` | `Pseudonymized` (governance mode partition; release post `legal_hold` expiry) | `prop_pseudonymization_correctness.rs` |
| 12 | `R2EvidencePseudo` | Pseudonymized | `src/backends/r2_evidence_pseudo.rs` | `Pseudonymized` (DPIA / LIA / DSR evidence buckets; 7y retention canonical) | `prop_pseudonymization_correctness.rs` |

**Coverage:** 12 / 12 canonical (8 effective + 4 pseudonymized).

> **Task framing note:** the parent execution charter originally
> framed the canonical fanout as "10 backends." That count predates
> Lote 10.11.0-bis baseline review (commit 2026-04-28) which
> reclassified the canonical fanout as 12 (8 effective + 4
> pseudonymized) per privacy_model.md §6.2 source-of-truth. This
> audit pins the higher count (12) per the SEALED v1.2.0 WI-S11-002
> spec; downstream automation already aligned with the 12-arm
> taxonomy.

## 3. SLI binding closure — `corelink_dsr_resolution_hours`

`slo_catalog.md §4.12` declares **SLO-FRESH-DSR-ERASURE** with the
canonical SLI numerator
`corelink_dsr_resolution_hours_bucket{request="erasure", le≤720} /
total` (720 h = 30 d = LGPD Art. 19 + GDPR Art. 12.3 + CCPA §1798.130
canonical SLA window).

**Pre-audit state:** `corelink-slo::Sli` had no `FreshDsrErasure`
arm; the `corelink-privacy-erasure-worker` 24h verification job had
no explicit emit-point constant. Production cron would have emitted
the histogram under some ad-hoc name (drift risk per the audit-pattern
established by `2026-05-14-slo-instrumentation-gaps.md`).

**Closure:**

- Added `Sli::FreshDsrErasure` to
  `crates/corelink-slo/src/definition.rs` (canonical SLI arm; slug
  `SLO-FRESH-DSR-ERASURE`; Prometheus base
  `corelink_dsr_resolution_hours`). Bumped `canonical_slis()` to 18
  arms; pinned count regression in `tests/prop_slo.rs`.
- Added `METRIC_DSR_RESOLUTION_HOURS`, `SLA_WINDOW_HOURS`,
  `dsr_resolution_hours()`, and `within_sla_window()` to
  `crates/corelink-privacy-erasure-worker/src/verification_job.rs`.
- Added `VerificationOutcome::sli_resolution_hours()` +
  `sli_within_sla()` so the cron worker emit-site can extract the
  canonical observation in one line.
- Added `crates/corelink-privacy-erasure-worker/tests/sli_binding.rs`
  cross-crate regression test pinning
  `METRIC_DSR_RESOLUTION_HOURS == Sli::FreshDsrErasure.prometheus_metric_base()`
  + the canonical slug + the 720 h SLA bucket bound. Same alignment
  pattern as `corelink-backup-verify::tests::sli_binding.rs`.

**Verification:** `cargo test -p corelink-privacy-erasure-worker --test sli_binding`
+ `cargo test -p corelink-slo` both green; 87 lib tests + 6
verification-job + 4 sli-binding + 5 prop-idempotency + 6
prop-pseudonymization + 3 refcount + 3 stripe-invoice + 1
integration + 2 chaos = **117 tests** across the erasure-worker
surface.

## 4. Jurisdictional DSR report surface (LGPD / GDPR / CCPA)

Per WI-S11-002 §1 + §6.1, the canonical
`crates/corelink-privacy-erasure-worker/src/report.rs::ErasureReport`
+ BLAKE3-keyed MAC signature (BLAKE3-256 keyed-hash, RFC 8785 JCS
canonical preimage) is the regulatory artifact for **all three**
jurisdictional surfaces. The report is jurisdiction-neutral by
construction; jurisdiction-specific narrative wraps the canonical
report at the customer email + admin-UI surface layer
(WI-S11-004 privacy notice 3-locale templates).

| Jurisdiction | Statute | SLA | Canonical artifact |
|---|---|---|---|
| Brazil (LGPD) | Art. 18 IV (eliminação) | 15 d (Art. 19) | `ErasureReport` + BLAKE3-signed MAC + `dsr_erasure_log` tombstones + EVT-048 R2 evidence-dsr 7y |
| EU (GDPR) | Art. 17.1 (right to erasure) | 30 d (Art. 12.3; extendable to 90 d for complex) | same artifact; `verified_complete = true` gates `dsr.completed.v1` |
| California (CCPA) | §1798.105 (delete) | 45 d (§1798.130) | same artifact; signed URL R2 24 h TTL to data subject email |

**Pseudonymization escape valve** (per GDPR Recital 26 + Art. 11 +
WP29 Op. 05/2014 endorsed by EDPB anonymization techniques) is
defensible across all three jurisdictions for the 4 pseudonymized
backends (R2 audit Object Lock 7y / Neon PITR backup 30d / R2 CAS
legal_hold partition / R2 evidence-* buckets 7y) where physical
erasure conflicts with another regulatory retention obligation
(audit immutability per SOC 2, fiscal retention per LGPD Art. 16).

## 5. Status-page integration deferral

WI-S11-002 §6 mandates publication of aggregated DSR completion
stats to `corelink.dev/status`. The canonical aggregation surface
(per-tenant + per-jurisdiction completion rate + p95 resolution-hours
histogram) requires:

- A production Grafana data source bound to the
  `corelink_dsr_resolution_hours` histogram (sink wired only at
  WI-S11-008 PRR ship gate per the `trait-abstraction-defer` charter
  pattern — no Cloudflare Analytics Engine staging cluster wired in
  pre-GA CI).
- A status-page render path
  (`apps/docs/docs/trust/data-handling.mdx` already declares the
  DSR commitment narrative; the live widget requires the production
  Grafana → status-page bridge configured at the deploy-time wiring
  layer).

**Decision:** documented here as a **deferred binding** per the
canonical `trait-abstraction-defer` pattern. The SLI emit-point is
pinned at the trait surface (this audit §3); the status-page render
binding lands at WI-S11-008 alongside the production Cloudflare
Queue / Neon / R2 / KV / DO / Stripe / Loki adapter bindings. No
trait drift risk because the SLI metric name is regression-pinned
across the SLO catalog ↔ erasure-worker boundary.

## 6. Test surface re-verification (2026-05-15)

| Crate / harness | Tests | Status |
|---|---|---|
| `corelink-privacy-erasure-worker` (lib + 8 integration files) | 117 (87 lib + 6 verification-job + 4 sli-binding + 5 prop-idempotency 100×replay + 6 prop-pseudonymization + 3 refcount + 3 stripe-invoice + 1 integration + 2 chaos) | green |
| `corelink-privacy-pseudonymize` | 11 (sha256 helper invariants + verify-post-facto + marker presence) | green |
| `corelink-dsr` (S-11 / WI-S11-001) | 38 (lib + prop) | green |
| `corelink-slo` (S-04 inheritance) | 15 (incl. `audit_2026_05_15_dsr_worker_closure_present`) | green |
| `e2e-dsr` (R3-3 harness) | 17 (6 happy + 5 adversarial + 3 prop + 3 misc) | green |

All `cargo clippy -p <crate> --tests -- -D warnings` gates clean.
`python3 scripts/validate_specs.py` clean (421 with schema + 9 YAML).
`python3 scripts/validate_dpia.py` clean (no PII trigger paths
changed).

## 7. Charter constraints re-verified

| Constraint | Status | Evidence |
|---|---|---|
| `#![forbid(unsafe_code)]` | green | `src/lib.rs` line 200 |
| no `unwrap` / `expect` / `panic` outside test | green | `cargo clippy -D warnings` clean |
| no `tokio` in `src/` | green | `[dependencies]` lists no tokio |
| audit fail-CLOSED on every state transition | green | `ErasureAuditSink` fail-CLOSED envelope per `ADR-S11-002`; pinned by orchestrator tests |
| `#[non_exhaustive]` public enums | green | `BackendKind` / `ErasureDecision` / `BackendErasureOutcome` / `ErasureCloudEventType` / `ErasureWorkerError` all `#[non_exhaustive]` |
| `subtle::ConstantTimeEq` for ticket-id compare | covered by upstream `corelink-dsr` | DSR ticket-id compare lives in `corelink-dsr::endpoint`; this crate consumes UUIDv7 inputs untouched |
| PROPTEST_CASES runtime fn | green | `tests/prop_idempotency_replay_100x.rs` reads `PROPTEST_CASES` env at runtime (S-07 P1-2 absorbed) |
| DSR atomicity (TLA+ per WI-S11-008 `dsr_erasure_atomicity.tla`) | green | erasure-then-verify atomic within a ticket via `idempotency` ledger + `verify_erasure` decision arms (`VerifiedComplete` requires all 12 backends present; any backend missing → `VerifiedPartial` / `VerificationFailed`); fail-CLOSED retry behaviour pinned by `chaos_per_backend_failure.rs` |
| Pseudonymization keys never persisted plaintext | green | `ErasureSalt` is per-DSR + per-tenant; ADR-S11-003 documents interim D1-vault encryption; full BYOK KMS deferred to S-14 |
| Worktree isolation | green | all changes in `.claude/worktrees/agent-a439c50046d50bb5e/`; branch `wt/r-prep-dsr-worker-prod` |

## 8. Closure summary

The canonical DSR erasure worker (WI-S11-002 v1.2.0 SEALED
2026-05-07) is **production-ready at the trait surface** per the
charter `trait-abstraction-defer` pattern. This audit closes:

1. **SLI emit-point binding**: `Sli::FreshDsrErasure` added,
   `METRIC_DSR_RESOLUTION_HOURS` constant pinned, cross-crate
   regression test in place.
2. **12-backend coverage matrix**: every canonical backend
   (`BackendKind` 12-arm enum) ships an in-memory adapter, a unit
   test, and at least one integration / chaos / regression /
   property test path.
3. **Jurisdictional report surface**: single `ErasureReport` +
   BLAKE3-signed MAC artifact serves LGPD / GDPR / CCPA; narrative
   layer wraps at WI-S11-004 (privacy notice 3-locale).
4. **Status-page deferral rationale**: documented per
   `trait-abstraction-defer`; production binding lands at WI-S11-008
   alongside the Cloudflare Queue / Neon / R2 / KV / DO / Stripe /
   Loki adapter bindings.

## 9. Open items deferred to WI-S11-008 (PRR ship gate)

- Cloudflare Queue consumer of `dsr.queued.v1` (worker entry).
- Neon multi-tabela `DELETE` cascade real binding.
- R2 CAS / R2 AC / R2 audit Object Lock real bindings.
- D1 `dsr_erasure_log` migration `0022_dsr_erasure_log.sql` apply
  in staging cluster.
- KV key-prefix `DELETE` real binding.
- Stripe `Customer.update` PCI-scope wrapper integration (reuse
  S-10 `StripeClient` trait).
- Loki `/loki/api/v1/delete` HTTP integration.
- 24h cron worker bound to `VerificationJob::run_24h_sweep` with
  the canonical observation emit to
  `corelink_dsr_resolution_hours` histogram.
- Status-page widget bridging the production Grafana panel to
  `corelink.dev/status`.

None of these gaps invalidate the canonical INV-DATA-ERASURE-COMPLETE
trait-level guarantees pinned by this audit; all are deferred
production-wiring deliverables per the canonical
`trait-abstraction-defer` charter pattern, gated by the WI-S11-008
PRR ship-gate sign-off matrix (12 HIGH_RISK roles).

## 10. References

- `specs/04_sprints/S11/work_items/WI-S11-002-erasure-worker-10-backends-verification-24h-reports.md` (SEALED v1.2.0 / 2026-05-07).
- `specs/04_sprints/S11/work_items/WI-S11-001-dsr-api-7-endpoints-jwt-receipt-mfa-step-up.md`.
- `specs/03_architecture/privacy_model.md §6.2` (12-backend canonical).
- `specs/03_architecture/slo_catalog.md §4.12` (SLO-FRESH-DSR-ERASURE).
- `specs/03_architecture/invariant_registry.md §3.5` (INV-DATA-ERASURE-COMPLETE CRITICAL).
- `specs/03_architecture/adrs/ADR-S11-002` (audit fail-CLOSED vs billing fail-OPEN split-tier).
- `specs/03_architecture/adrs/ADR-S11-003` (erasure_salt interim D1 vault).
- `specs/03_architecture/adrs/ADR-S11-004` (cross-backend eventual consistency).
- `specs/_audits/2026-05-14-slo-instrumentation-gaps.md` (SLI-binding pattern).
- `crates/corelink-privacy-erasure-worker/` (canonical worker crate).
- `crates/corelink-privacy-pseudonymize/` (canonical pseudonymization helper).
- `crates/corelink-dsr/` (DSR API surface — WI-S11-001).
- `crates/corelink-slo/src/definition.rs` (SLI taxonomy — `FreshDsrErasure` closure).
- `tests/e2e-dsr/` (R3-3 end-to-end harness).
