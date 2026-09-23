---
schema: corelink-ownership/1.1
document: reference
package: corelink-slo
manifest: crates/corelink-slo/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: draft
evidence_set: slo-source-static-20260920
---

# corelink-slo — reference

Static source reference. SOURCE != executed, runtime, deployed, observed, Prometheus, or PagerDuty evidence. Verified canonical OKF is routed as reference only; this artifact neither revalidates nor restates its policy.

[Identity](#r01) · [Exports](#r02) · [Definitions](#r03) · [Windows](#r04) · [Calculator](#r05) · [Alert seam](#r06) · [Fakes](#r07) · [Unknowns](#r08).

<a id="r01"></a>
## R01 — Package identity

`Cargo.toml` names `corelink-slo`; `src/lib.rs` describes a multi-burn decision and dispatch-primitive package. The manifest has `thiserror`, test-only `proptest`, `rand`, and `rand_chacha`; it declares no HTTP, Prometheus, or PagerDuty client dependency. Falsifiable invariant: adding such a dependency changes this static inventory. Evidence: `Cargo.toml:1-25`, `src/lib.rs:1-162`.

<a id="r02"></a>
## R02 — Root export contract

The root exposes eight local modules and re-exports definition, window, decision, calculator, alert, audit, pagerduty, and error surfaces. It forbids unsafe code and denies missing docs. Falsifiable invariant: a root re-export removal, addition, or module rename changes consumer paths. Evidence: `src/lib.rs:127-162`.

<a id="r03"></a>
## R03 — Definition and SLI contract

`SloDefinition` retains an SLI, target, error budget, and window days; constructors reject invalid targets or zero window days. `Sli` supplies stable slugs and a Prometheus-compatible metric-base string. Falsifiable invariant: invalid input must return `SloError::InvalidSloDefinition`, not construct a definition. The metric-base method is a string contract, not proof of metric export. Evidence: `definition.rs:18-253`.

<a id="r04"></a>
## R04 — Window and decision taxonomy

`BurnRateWindow` defines four static windows with multipliers 14.4, 6, 3, and 1; `AlertDecision` partitions five variants into quiet, ticket, and page predicates. Falsifiable invariant: each canonical decision belongs to exactly one predicate class, and a listed window retains its associated multiplier. Evidence: `window.rs:26-128`, `decision.rs:23-105,177-186`.

<a id="r05"></a>
## R05 — Pure burn calculation

`error_rate_in_window` returns zero for a zero total and otherwise divides errors by total. `decide` returns `Quiet` for non-positive, non-finite, or below-threshold rates; at or above threshold it maps the selected window to its decision. Falsifiable invariant: changing the comparison or window match changes the decision relation. Evidence: `calculator.rs:32-127`.

<a id="r06"></a>
## R06 — Alert orchestration boundary

`MultiBurnRateAlert` composes a calculator, `SloAuditSink`, `PagerDutyDispatcher`, service key, and per-tenant in-process ledger. Page decisions form a `PagerDutyEvent` then call the injected dispatcher; quiet and ticket paths do not. Falsifiable invariant: moving an audit call after ledger mutation or dispatch changes the source ordering. Evidence: `alert.rs:99-343`.

**Contract index:** [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [INV-005](#inv-005).

<a id="r07"></a>
## R07 — Fake and failure seams

`InMemorySloAuditSink` and `InMemoryPagerDutyDispatcher` keep mutex-protected fixture state; `FailingSloAuditSink` and `FailingPagerDutyDispatcher` return induced errors. Falsifiable invariant: their behavior is local and trait-conforming. They are not a PagerDuty or audit adapter. Evidence: `audit.rs:139-225`, `pagerduty.rs:144-274`.

<a id="api-001"></a>
### API-001 — Definition and canonical taxonomy
**Symbols:** `SloDefinition::{new,with_window_days}`, `Sli`, `canonical_slis`, `BurnRateWindow`, `canonical_burn_rate_windows`. **Input/precondition:** supplied SLI, target percentage, and optional positive window days. **Output/errors/effects:** validated definition or `SloError`; static canonical arrays. **Compatibility:** SLI slugs/metric bases and window multipliers are public values. **Links/evidence:** INV-001; REL-001/002; `definition.rs`, `window.rs`.

[Contract index](#r01)

<a id="api-002"></a>
### API-002 — Sample calculation and decision
**Symbols:** `BurnRateSample`, `BurnRateCalculator::{error_rate_in_window,decide}`, `AlertDecision::{canonical_alert_decisions,is_page,is_ticket,is_quiet}`. **Input/precondition:** sample and definition; zero total is allowed. **Output/errors/effects:** floating rate and one decision. **Compatibility:** four window-to-decision arms are source contract. **Links/evidence:** INV-002/003; REL-001/002; `calculator.rs`, `decision.rs`.

[Contract index](#r01)

<a id="api-003"></a>
### API-003 — Alert evaluation and injected ports
**Symbols:** `AlertEvaluation::{tenant_id,sli,window,sample,now_ms}`, `AlertEvaluationOutcome`, `MultiBurnRateAlert::{new,evaluate,evaluate_dispatcher_fail_open,tenant_evaluation_count}`. **Input/precondition:** caller supplies an evaluation and constructor-injected audit/dispatcher/service key. **Output/errors/effects:** typed outcome plus local per-tenant ledger; audit and page dispatch occur only on source branches. **Compatibility:** ordering and page-only dispatch are material. **Links/evidence:** INV-004/005; REL-003/004/005; `alert.rs`.

[Contract index](#r01)

<a id="api-004"></a>
### API-004 — Audit record, taxonomy, and sink
**Symbols:** `SloAuditEventType::{as_str}`, `SloAuditRecord`, `SloAuditEmitError::Store`, `SloAuditSink::emit`, `canonical_slo_audit_event_strings`, `InMemorySloAuditSink::{new,snapshot,len,is_empty,snapshot_of}`, `FailingSloAuditSink`. **Inputs:** SLI/window/decision slugs, tenant, dedup key, and epoch-ms timestamp in records. **Output/effects:** injected sink result; fixture stores locally. **Compatibility:** five event literals and fields are source contracts. **Links/evidence:** INV-004; REL-018/021; `audit.rs`.

[Contract index](#r01)

<a id="api-005"></a>
### API-005 — PagerDuty event and dispatch port
**Symbols:** `PagerDutyEventAction::{as_str}`, `canonical_pagerduty_actions`, `PagerDutyServiceKey::service_name`, `PagerDutyEvent`, `PagerDutyDispatcher::dispatch`, `InMemoryPagerDutyDispatcher::{new,snapshot_attempts,snapshot_open_incidents,attempt_count,open_incident_count}`, `FailingPagerDutyDispatcher`. **Inputs:** typed service/action, dedup key, severity, summary, source, runbook URL. **Output/errors/effects:** injected `Result<(), SloPagerDutyDispatchError>`; fixture records attempts locally. **Compatibility:** action strings, service labels, event fields. **Links/evidence:** INV-005; REL-019; `pagerduty.rs`, `error.rs`.

[Contract index](#r01)

<a id="api-006"></a>
### API-006 — Schema and error exports
**Symbols:** `slo_schema_version()`, `SloError::{InvalidSloDefinition,Audit,Dispatcher,Internal}`, `SloPagerDutyDispatchError::Transport`. **Inputs:** schema query or typed failure details. **Output/effects:** schema integer `1`; non-exhaustive typed errors, with dispatcher transport convertible into `SloError::Dispatcher`. **Compatibility:** version and public error variants/display forms are caller-visible. **Links/evidence:** REL-020/022; `lib.rs`, `error.rs`.

[Contract index](#r01)

<a id="inv-001"></a>
### INV-001 — Definition validates target and window
**Predicate:** constructor rejects target outside declared range and zero days. **Enforcement:** `SloDefinition::{new,with_window_days}`. **Violation:** invalid target/window returns `Ok`. **Verification:** inspect branch and definition tests; execution unknown.

[Contract index](#r01)

<a id="inv-002"></a>
### INV-002 — Zero-total sample has zero rate
**Predicate:** `error_rate_in_window` returns `0.0` when total is zero. **Enforcement:** first branch in `calculator.rs`. **Violation:** zero-total sample divides or returns nonzero. **Verification:** inspect branch and test source; execution unknown.

[Contract index](#r01)

<a id="inv-003"></a>
### INV-003 — Window threshold maps to one decision arm
**Predicate:** selected window multiplier and comparison choose the documented window-specific decision; non-positive/non-finite/below-threshold rate is Quiet. **Enforcement:** `calculator.rs::decide`. **Violation:** altered threshold or window arm. **Verification:** source match and tests; execution unknown.

[Contract index](#r01)

<a id="inv-004"></a>
### INV-004 — Audit precedes local mutation and dispatch
**Predicate:** evaluation and page audit emits occur before corresponding local state mutation or dispatcher call. **Enforcement:** branch ordering in `alert.rs`. **Violation:** mutation/dispatch precedes required emit. **Verification:** inspect every branch/order; no durable atomicity implied.

[Contract index](#r01)

<a id="inv-005"></a>
### INV-005 — Only page decisions dispatch
**Predicate:** only `AlertDecision::is_page()` reaches `dispatch_page`. **Enforcement:** branch in `alert.rs::evaluate`. **Violation:** quiet/ticket reaches dispatcher. **Verification:** trace call graph and branch; runtime unknown.

[Contract index](#r01)

<a id="r08"></a>
## R08 — Evidence limits and unknowns

Source does not establish test execution, feature resolution, full consumer reachability, metric emission or scrape, alert-rule loading/evaluation, PagerDuty HTTPS/API delivery, incident state, service keys, credentials, retry/fallback, audit durability, deployment, or live page/ticket behavior. These remain unknown pending separately recorded evidence.

[Ownership guide](../../../../.claude/skills/own-corelink-slo/SKILL.md#s01) · [Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01).
