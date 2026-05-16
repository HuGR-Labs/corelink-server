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

## 5. Status-page integration — wave-16 shipped

**Status (2026-05-15 wave-16):** SHIPPED. WI-S11-002 §6 mandate
"DSR completion stats published to corelink.dev/status" is now
satisfied at the trait surface + real HTTP wiring + bridge from the
worker aggregator. The wave-15 deferral rationale below is preserved
for historical context; the closing follow-on is captured immediately
under it.

### 5.1 Wave-15 deferral rationale (historical)

WI-S11-002 §6 mandates publication of aggregated DSR completion
stats to `corelink.dev/status`. The canonical aggregation surface
(per-tenant + per-jurisdiction completion rate + p95 resolution-hours
histogram) required, at the wave-15 cycle, a production Grafana data
source bound to the `corelink_dsr_resolution_hours` histogram + a
status-page render path. Both were deferred under the canonical
`trait-abstraction-defer` charter pattern; the SLI emit-point was
already pinned at the trait surface (this audit §3); the status-page
render binding was scheduled for WI-S11-008 alongside production
Cloudflare Queue / Neon / R2 / KV / DO / Stripe / Loki adapter
bindings.

### 5.2 Wave-16 closure surface

Wave-16 ships the canonical Atlassian Statuspage integration:

| Layer | Crate / module | Notes |
|---|---|---|
| Aggregator (pure, wasm32-clean) | `corelink-privacy-erasure-worker::statuspage_publish` | `DsrCompletionStats` + `aggregate_24h_window(&[VerificationOutcome], window_start_unix_s)` + canonical p95 (nearest-rank) over `dsr_resolution_hours` observations. |
| Publish payload + trait | `corelink-statuspage-real::report` + `::backend` | `DsrCompletionReport` 24h-rolling payload (validated: 24h window invariant + 7_200h p95 clock-skew ceiling) + `StatuspageBackend` trait surface (real HTTP + in-memory fake share). |
| Real HTTP wiring | `corelink-statuspage-real::http::StatuspageHttpClient` | `reqwest::blocking` POST `https://api.statuspage.io/v1/pages/{page_id}/metrics/{metric_id}/data.json` with `Authorization: OAuth <STATUSPAGE_API_KEY>`; body `{"data":{"timestamp":<unix_s>,"value":<p95_hours>}}`. |
| Rate-limiter | `corelink-statuspage-real::rate_limit::StatuspageRateLimiter` | 1 publish per 5 min per `(page_id, metric_id)` (monotonic-clock-driven); denies emit `corelink.privacy.statuspage_rate_limited.v1` + return `RateLimited { retry_after_ms, jitter_ms = retry_after_ms / 8 }`. |
| Audit envelope (fail-CLOSED) | `corelink-statuspage-real::audit::StatuspageAuditSink` | 4 canonical event types: `statuspage_published.v1` / `statuspage_auth_failed.v1` / `statuspage_rate_limited.v1` / `statuspage_failed.v1`. Audit emit fires BEFORE caller-visible outcome (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER). Credential redacted via `redact_api_key` (last-4 only). |
| Retry | `corelink-statuspage-real::retry::RetryPolicy` | 3 retries exp-backoff on 5xx + 429; 401 / 403 distinct `GiveUpAuth` (drives auth audit type); other 4xx fatal. |
| Bridge | `corelink-statuspage-real::dsr_bridge::bridge_to_report` | Pure converter: worker `DsrCompletionStats` → `DsrCompletionReport`; the publish job at WI-S11-008 PRR ship gate composes `aggregate_24h_window → bridge_to_report → publish_dsr_metric`. |

### 5.3 Test surface (wave-16 net-new)

| Crate / harness | Tests | Status |
|---|---|---|
| `corelink-statuspage-real` (lib) | 29 (audit 2 + memory 3 + rate_limit 5 + redact 4 + report 5 + retry 7 + dsr_bridge 3) | green |
| `corelink-statuspage-real::tests::dsr_publish` (integration; WireMock) | 4 (happy 201 + auth 401 + rate-limit deny-with-jitter + worker-bridge end-to-end) | green |
| `corelink-privacy-erasure-worker::statuspage_publish` (worker aggregator) | 8 (p95 algorithm 4 + 24h aggregator 4) | green |

Worker lib total grows from 87 → 95 (wave-16 +8). Integration test
count for the worker is unchanged (8 files; the statuspage publish
path is intentionally NOT a worker integration test — the trait
boundary keeps the worker wasm32-clean).

### 5.4 Charter constraints (re-verified for wave-16 crates)

| Constraint | `corelink-statuspage-real` | Evidence |
|---|---|---|
| `#![forbid(unsafe_code)]` | green | `src/lib.rs` |
| no `unwrap` / `expect` / `panic` outside test | green | `cargo clippy --tests -- -D warnings` clean |
| no `tokio` in `src/` | green | `reqwest::blocking` only; tokio is dev-only (WireMock host) |
| audit fail-CLOSED on every publish path | green | Audit emit BEFORE caller-visible outcome on each of Published / AuthFailed / RateLimited / Failed |
| `#[non_exhaustive]` public enums | green | `StatuspageAuditOutcome` / `StatuspageClientError` / `RateLimitDecision` / `RetryDecision` / `StatuspageAuditError` / `StatuspageAuditEvent` / `DsrCompletionReportError` all `#[non_exhaustive]` |
| Credential never logged plaintext | green | `redact_api_key` last-4 only; pinned by `redact::tests::redact_never_contains_full_key` + `tests::dsr_publish::*` audit-envelope assertion |
| Rate-limit framework | green | `StatuspageRateLimiter` 5-min canonical window; pinned by 5 unit + 1 integration test |
| Worktree isolation | green | wave-16 work-tree `.claude/worktrees/agent-abf9bd4a905788d4a/`; branch `wt/r-prep-dsr-statuspage-wire` |

### 5.5 Production secrets surface

The wave-16 wiring consumes three production secrets, all already in
the canonical matrix (`docs/internal/secrets-checklist.md`):

- Row 42 — `STATUSPAGE_API_KEY` (existing; status-sync path).
- Row 116 — `STATUSPAGE_PAGE_ID` (wave-16 net-new — Atlassian
  Statuspage page identifier; config, not credential).
- Row 117 — `STATUSPAGE_METRIC_DSR_RESOLUTION_HOURS` (wave-16
  net-new — Atlassian Statuspage Public-Metric ID for DSR resolution
  hours p95 publication; config, not credential).

`python3 scripts/validate_secrets_matrix.py` remains green; the 116
matrix rows grow to 118 (drift only on the matrix-only side, never
code-only).

### 5.6 Wave-17 closure — scheduler binding shipped; wave-18 closure — wasm32 real backend wired

**Status (2026-05-15 wave-18):** scheduler real backend wired wave-18.
Wave-17 (commit `49901da` + `dd58b27`) shipped the publish scheduler
composition (`aggregate_24h_window → bridge_to_report →
publish_dsr_metric`) on native + the wasm32 `#[event(scheduled)]`
firing surface. The wasm32 path short-circuited with the canonical
`wasm32_real_binding_deferred` skip event because the wave-16
`StatuspageHttpClient` uses `reqwest::blocking` which does not link
on wasm32. Wave-18 lands the real `worker::Fetch`-backed
[`corelink_statuspage_real::StatuspageWasm32Client`] +
`worker::D1Database`-backed
[`corelink_dsr_statuspage_scheduler::D1Wasm32RowSource`] and wires
them into the cron handler — the skip event is REMOVED; the cron
publishes on wasm32 from wave-18 forward.

Wave-16 shipped the publish-path layers but the scheduler binding
(cron tick + D1 row source + idempotency dedupe + scheduler-level
audit envelope) was deferred under the `trait-abstraction-defer`
charter (see §5.1 historical rationale). Wave-17 closed that gap on
native; wave-18 closes it on wasm32.

| Layer | Crate / module | Notes |
|---|---|---|
| Scheduler (native, trait-driven) | `corelink-dsr-statuspage-scheduler` (NEW) | `DsrStatuspagePublishScheduler::run_once(now_unix_s)` composes the four trait-bound collaborators (`D1RowSource`, `CronRunLog`, `StatuspageBackend`, `SchedulerAuditSink`) into a single 24h-cron-firable orchestration. Native-only (the wave-16 `StatuspageHttpClient` uses `reqwest::blocking` which does not link on wasm32). |
| CF Worker cron entry | `corelink-clerk-cf::dsr_statuspage_cron` | wave-17: `#[event(scheduled)]` handler resolving the three `STATUSPAGE_*` bindings + emitting the canonical `corelink.privacy.statuspage_publish_scheduled.v1` NDJSON audit line BEFORE any work; short-circuited with `wasm32_real_binding_deferred` skip event because wave-16 `StatuspageHttpClient` (reqwest::blocking) does not link on wasm32. **wave-18:** real `worker::Fetch`-backed `StatuspageWasm32Client` + `worker::D1Database`-backed `D1Wasm32RowSource` wired inline; skip event REMOVED. Cron handler hand-composes `D1Wasm32RowSource::fetch_window_async → aggregate_24h_window → bridge_to_report → StatuspageWasm32Client::publish_dsr_metric_async` (the native [`DsrStatuspagePublishScheduler`] uses sync trait surfaces incompatible with the async wasm32 worker::* runtime). New bindings: `STATUSPAGE_TENANT_ID` (var) + `DSR_LOG_DB` (D1 binding). |
| Cron trigger | `crates/corelink-clerk-cf/wrangler.toml` `[triggers] crons = ["0 6 * * *"]` | 06:00 UTC daily — scheduled AFTER the audit-chain daily-verify at 02:00 UTC and BEFORE SF business start (gives 10h buffer for ops to react to `statuspage_publish_failed.v1` before customer business day). Pinned verbatim against `corelink-dsr-statuspage-scheduler::CRON_EXPRESSION` + `corelink-clerk-cf::dsr_statuspage_cron::CRON_EXPRESSION` — both constants assert `"0 6 * * *"` in unit tests so any drift fails CI. |
| Idempotency dedupe | `corelink-dsr-statuspage-scheduler::cron_log::CronRunLog` | Trait surface for a D1-backed `(date_yyyymmdd, metric_id)` PRIMARY KEY ledger. The pre-flight `is_recorded` check + the post-publish `record` call together guarantee one publish per UTC day per metric (the CF runtime retrying a `scheduled` event in the same day short-circuits with the canonical `skipped / already_published_today` audit). `InMemoryCronRunLog` fake covers the algorithmic invariants. |
| Scheduler audit envelope (fail-CLOSED) | `corelink-dsr-statuspage-scheduler::audit::SchedulerAuditSink` | 4 canonical event types: `statuspage_publish_scheduled.v1` / `statuspage_publish_succeeded.v1` / `statuspage_publish_failed.v1` / `statuspage_publish_skipped.v1`. Every transition emits BEFORE the caller-visible outcome (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER); audit emit failure aborts the tick as `SchedulerError::Audit` (fail-CLOSED). |

#### 5.6.1 Test surface (wave-17 net-new)

| Crate / harness | Tests | Status |
|---|---|---|
| `corelink-dsr-statuspage-scheduler` (lib) | wave-17: 14 (audit 3 + cron_log 5 + row_source 2 + scheduler 4); **wave-18: +2** (`canonical_outcome_query_is_pinned` + `canonical_outcome_query_passes_tenant_scope_validator`) = 16 total | green |
| `corelink-dsr-statuspage-scheduler::tests::dsr_statuspage_cron` (integration; WireMock) | 6 (cron-expression pin + happy 201 + empty-window skip + auth 401 + rate-limit 429 retry-exhaustion + d1 read-fail) | green |
| `corelink-clerk-cf` (lib; cron module unit tests) | wave-17: 3 (binding-name pin + cron-expression pin + audit-type pin); **wave-18 closure**: 3 (binding-name pin extends with `STATUSPAGE_TENANT_ID` + `DSR_LOG_DB`; audit-type pin extends with the four canonical `statuspage_publish_{scheduled,succeeded,failed,skipped}.v1` strings) | green |
| `corelink-statuspage-real::tests::wasm32_backend` (integration; native composition equivalence) | **wave-18 net-new**: 2 (`in_memory_backend_published_audit_shape_matches_wasm32_contract` + `rate_limit_audit_shape_pinned_for_wasm32_parity`) | green |

#### 5.6.2 Charter constraints (re-verified for wave-17)

| Constraint | `corelink-dsr-statuspage-scheduler` | Evidence |
|---|---|---|
| `#![forbid(unsafe_code)]` | green | `src/lib.rs` |
| no `unwrap` / `expect` / `panic` outside test | green | `cargo clippy --tests -- -D warnings` clean |
| no `tokio` in `src/` | green | tokio is dev-only (WireMock host); `src/` is sync-blocking (mirrors wave-16 stance) |
| audit fail-CLOSED on every transition | green | Audit emit BEFORE caller-visible outcome on each of Scheduled / Succeeded / Failed / Skipped; pinned by all 6 integration tests |
| `#[non_exhaustive]` public enums | green | `SchedulerAuditOutcome` / `SkipReason` / `SchedulerAuditEvent` / `SchedulerAuditError` / `D1RowSourceError` / `CronRunLogError` / `RecordedRunStatus` / `RunOutcome` / `SchedulerError` all `#[non_exhaustive]` |
| Credential never logged plaintext | green | wave-16 `redact_api_key` bottoms-out at `StatuspageHttpClient`; this crate's audit envelope never sees the plaintext key |
| Idempotent composition (1 publish per UTC day) | green | `CronRunLog::record` enforces `(date_yyyymmdd, metric_id)` PRIMARY KEY; second tick short-circuits with `Skipped / AlreadyPublishedToday`; pinned by `already_recorded_short_circuits_with_already_published_today` lib test |
| Worktree isolation | green | wave-17 work-tree `.claude/worktrees/agent-a6fa9fb8e5a49007f/`; branch `wt/r-prep-dsr-statuspage-cron-scheduler` |

### 5.7 Wave-18 closure — PagerDuty alert wiring for the SEV-1 failed emit

**Status (2026-05-15 wave-18):** SHIPPED. The wave-17 scheduler emits
4 canonical audit event types per tick; wave-18 wires the SEV-1 event
(`corelink.privacy.statuspage_publish_failed.v1`) into PagerDuty +
ships the operator runbook. The other three events
(`statuspage_publish_scheduled.v1` / `_succeeded.v1` / `_skipped.v1`)
remain info-only dashboard-tile surfaces (NOT paged) per the wave-18
scope decision documented in `dashboards/alerts/dash-dsr-statuspage-alerts.yml`
header.

| Layer | Artefact | Notes |
|---|---|---|
| PD alert rule | `dashboards/alerts/dash-dsr-statuspage-alerts.yml` rule `DsrStatuspagePublishFailed` | severity=critical (SEV-1) → `routing_key=PAGERDUTY_ROUTING_KEY` (existing row 11) → escalation_policy `corelink-incident-response`; 5-min burn for early detection (catches all retries inside one cron firing). |
| Recording rules | `corelink_dsr_statuspage_publish_failed_5m` + `corelink_dsr_statuspage_publish_success_rate_5m` | First feeds the alert expr (cheap rollup); second feeds the DSR overview dashboard tile. Both share the metric contract with the alert so the dashboard and the page read the same series. |
| Runbook | `specs/_runbooks/RB-DSR-STATUSPAGE-PUBLISH-FAILED.md` | SEV-1 runbook §1 Detect → §2 Triage (4xx auth / 5xx vendor / network / D1 read) → §3 Containment (vendor outage > 24h customer email) → §4 Mitigation (manual one-shot retry + cron auto-reconcile; do-NOT-thunder backoff discipline) → §5 Compliance impact (GDPR Art. 30 / LGPD Art. 37; outside-counsel notification > 7d) → §6 Resolution (3 consecutive successful publishes) → §7 MTTA ≤ 4h / MTTR ≤ 24h. |
| Pseudonymization | PD payload carries `page_id_hex8` + `metric_id_hex8` ONLY (BLAKE3 first-8-hex per INV-AUTH-AUDIT-PSEUDONYMIZATION + CTRL-PRIV-001 + INV-OBS-CARDINALITY-BUDGET) | The DSR Statuspage publish is system-scoped (no tenant_id in path); the alert pipeline never sees the raw Atlassian identifiers and never sees the `STATUSPAGE_API_KEY` bytes (wave-16 `redact_api_key` last-4 semantics bottom out at `StatuspageHttpClient`). |
| Regulatory posture | GDPR Art. 30 / LGPD Art. 37 records-of-processing transparency | 72h breach-notification clocks (GDPR Art. 33 / LGPD Art. 46) do NOT apply — no personal data is disclosed. Outage ≤ 7d is documented in the post-incident memo; outage > 7d triggers outside-counsel notification per runbook §5.2. |
| MTTA / MTTR target | 4h MTTA / 24h MTTR | Wider than the audit-export SEV-1 (5 min MTTA / 24h MTTR; `dash-audit-export-alerts.yml` rule `AuditExport_CrossTenantAttempt`) because the DSR worker (full erasure processing) is UNAFFECTED by this failure — the cron will reconcile naturally on the next 06:00 UTC tick. |

The wave-18 closure satisfies the WI-S11-002 §6 wave-18 closure-note
mandate "PD wiring for DSR Statuspage cron failures DONE wave-18"
and supersedes the wave-17 audit-doc §5.6 implicit "no PD wiring
yet" status without further code change to the wave-17 scheduler
crate (the audit emit-point + metric counter contract were
pre-staged at wave-17).

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
4. **Status-page integration — shipped wave-16 + scheduler bound
   wave-17**: trait surface + real Atlassian Statuspage HTTP wiring
   + worker aggregator + bridge shipped under `corelink-statuspage-
   real` + `corelink-privacy-erasure-worker::statuspage_publish` at
   wave-16. The publish scheduler that fires the composition once per
   24h (`aggregate_24h_window → bridge_to_report → publish_dsr_metric`
   with `(date, metric_id)` idempotency dedupe) shipped wave-17 under
   `corelink-dsr-statuspage-scheduler` + `corelink-clerk-cf::
   dsr_statuspage_cron` + `wrangler.toml` `[triggers] crons =
   ["0 6 * * *"]`. Rate-limited 1-per-5-min, fail-CLOSED audit envelope
   on both the wave-16 publish layer (`statuspage_published.v1` /
   `_auth_failed.v1` / `_rate_limited.v1` / `_failed.v1`) AND the
   wave-17 scheduler layer (`statuspage_publish_scheduled.v1` /
   `_succeeded.v1` / `_failed.v1` / `_skipped.v1`). Credential never
   logged plaintext. See §5.2 / §5.3 / §5.5 / §5.6 above.

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
- ~~Status-page widget bridging the production Grafana panel to
  `corelink.dev/status`.~~ **CLOSED wave-17 (see §5.6).** Scheduler
  binding shipped under `corelink-dsr-statuspage-scheduler` +
  `corelink-clerk-cf::dsr_statuspage_cron`; `[triggers] crons =
  ["0 6 * * *"]` row added to `crates/corelink-clerk-cf/wrangler.toml`.
  The real D1 row source + `worker::Fetch`-backed Statuspage HTTP
  client remain trait-abstraction-deferred to a follow-up real-binding
  wave (the wave-17 wasm32 handler short-circuits with the canonical
  `wasm32_real_binding_deferred` skip event so the cron firing is
  observable in the audit chain from wave-17 forward).

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
