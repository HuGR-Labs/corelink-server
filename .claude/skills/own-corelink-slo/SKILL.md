---
name: own-corelink-slo
description: Own the static source contract for corelink-slo multi-burn decisions, audit seams, and PagerDuty-shaped fakes without asserting live alert execution.
metadata:
  evidence-set: slo-source-static-20260920
  source-commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
  package: corelink-slo
  manifest: crates/corelink-slo/Cargo.toml
  profile: S
---

# Own corelink-slo

Static ownership guide at source commit `6ed297f5b2b64cf97447985111a2ecbbaa9536bb`. SOURCE != executed, runtime, deployed, or observed: this guide authorizes source reading only.

[Scope](#s01) · [Surface](#s02) · [Axioms](#s03) · [Decision](#s04) · [Seams](#s05) · [Unknowns](#s06) · [Handoff](#s07).

<a id="s01"></a>
## S01 — Scope and evidence boundary

Own `crates/corelink-slo/` and its manifest only. Read source, manifests, and checked-in tests; do not infer Prometheus ingestion, PagerDuty delivery, incident creation, deployment, or runtime behavior. Route policy questions to the verified canonical OKF; do not copy, revalidate, or redefine it.

Activate for edits to the crate manifest, public SLI/definition/window/decision
contracts, burn calculation, alert state/audit ordering, or dispatcher seam.
Do not activate for generic SLO policy questions, dashboards, Prometheus rule
operations, or PagerDuty configuration that do not change this crate. Route a
crate change through Reference → linked Blast Radius REL → Maintenance PROC.
Stop for secrets, network/provider operation, production paging, or runtime
claims; retain the claim as unknown and escalate to its operator.

<a id="s02"></a>
## S02 — Public surface

Trace root re-exports to `definition`, `window`, `decision`, `calculator`, `alert`, `audit`, `pagerduty`, and `error`. Classify `corelink-telemetry/src/slo.rs` as a source re-export edge, not proof of execution. Evidence: `src/lib.rs:137-162`; telemetry `src/slo.rs:1-8`.

<a id="s03"></a>
## S03 — Five source axioms

1. `BurnRateCalculator::decide` is pure source logic: a non-positive rate is `Quiet`.
2. A nonzero rate below the selected multiplier times error budget is `Quiet`.
3. Fast, medium, slow, and long windows map to the four non-`Quiet` decision variants defined in `decision.rs`; `Quiet` is the separate below-threshold/no-burn outcome.
4. `MultiBurnRateAlert` emits its named audit record before its documented state mutation or page dispatch attempt.
5. Only `AlertDecision::is_page()` reaches the `PagerDutyDispatcher` trait call; in-memory and failing dispatchers are fixtures.

Each axiom is falsifiable by a source diff that changes the named branch, ordering, or trait call. Evidence: `calculator.rs:75-127`, `alert.rs:147-332`, `pagerduty.rs:144-273`.

<a id="s04"></a>
## S04 — Decision review

For a change to targets, samples, windows, or decisions, trace `SloDefinition` → `BurnRateCalculator::error_rate_in_window` → `decide` → `AlertDecision`. Preserve explicit zero-total behavior and compare all four multipliers. A slug or metric-base change requires a separate static consumer sweep; a method name mentioning Prometheus is not exporter evidence.

<a id="s05"></a>
## S05 — Audit and dispatch seams

Review `SloAuditSink::emit` and `PagerDutyDispatcher::dispatch` as injected interfaces. `InMemory*` captures fixture state and `Failing*` induces errors. Their source proves seam shape and local fake behavior only; it does not prove durable audit storage, HTTPS, routing keys, credentials, PagerDuty acceptance, or paging.

<a id="s06"></a>
## S06 — Explicit unknowns

Unknown: resolved features; all callers; real Prometheus scrape/rule evaluation; real PagerDuty API traffic, deduplication, service configuration, and incident state; audit durability; retry/fallback; secrets; deployment; and production alert execution. Obtain separately scoped evidence before stating any of these facts.

<a id="s07"></a>
## S07 — Static handoff

Record baseline SHA, changed paths, public symbols, source relations, source-level invariants, direct static consumers, and unknowns. State checks actually run separately from source evidence. Hand runtime, provider, credential, deployment, and operational claims to their responsible owner.

[Reference](../../../docs/ownership/crates/corelink-slo/REFERENCE.md#r01) · [Blast radius](../../../docs/ownership/crates/corelink-slo/BLAST_RADIUS.md#b01) · [Maintenance](../../../docs/ownership/crates/corelink-slo/MAINTENANCE.md#m01).
