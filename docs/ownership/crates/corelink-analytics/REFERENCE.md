---
schema: corelink-ownership/1.1
document: reference
package: corelink-analytics
manifest: crates/corelink-analytics/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: S
state: draft
evidence_set: analytics-static-source-20260920
---

# corelink-analytics — ownership reference

Static-source reference for the analytics crate. It records declared and source-visible relationships; it does not prove runtime export, D1 execution, Workers Analytics Engine, Prom remote-write, or deployment.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Contracts](#r04) · [Invariants](#r05) · [Configuration](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity

| Field | Static evidence |
|---|---|
| Package | `corelink-analytics`, `crates/corelink-analytics/Cargo.toml` |
| Role | RED metrics observer, typed-label cardinality validator, and analytics audit-envelope surface |
| Direct internal dependency | `corelink-eviction` in the manifest |
| Tests declared | `prop_analytics` and `migration_canonical_0015` |

<a id="r02"></a>
## R02 — Boundary

The crate owns its `src/{audit,canonical,config,error,labels,lib,observer,validator}.rs` modules and embeds `migrations/d1/0015_analytics_cardinality_budgets.sql`. The manifest description defers production Workers Analytics Engine and Prom remote-write; that deferment is source intent, not a runtime assertion.

<a id="r03"></a>
## R03 — Implementation map

| Module | Source-visible responsibility |
|---|---|
| `canonical` / `labels` | Metric kinds and typed label vocabulary |
| `config` / `error` | Budget and histogram configuration; typed errors |
| `observer` / `validator` | Observer trait/fakes and tuple-admission decision surface |
| `audit` | Typed audit sink, records, taxonomy, and in-memory/failing fixtures |
| `lib` | Public re-exports, embedded migration, schema-version function |

<a id="r04"></a>
## R04 — Public contracts

[Metric and labels](#api-001) · [Observer](#api-002) · [Validator](#api-003) · [Audit](#api-004).

<a id="api-001"></a>
### API-001 — Canonical metric and label vocabulary
`RedMetricKind`, `MetricLabelTuple`, `Tier`, `Region`, and `FORBIDDEN_LABEL_NAMES` are re-exported by `src/lib.rs`; `canonical.rs` and `labels.rs` define their source-visible vocabulary. Unknown: every downstream match or serialization consumer. [Return](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Observation surface
`RedMetricsObserver`, `InMemoryRedMetrics`, and `FailingRedMetrics` are re-exported; `observer.rs` is the local trait/fake implementation boundary. Unknown: a production observer implementation. [Return](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Cardinality admission
`CardinalityValidator`, `ValidatorDecision`, and `ValidatorOutcome` are re-exported from `validator.rs`; config exposes per-metric and global budget values. Unknown: durable ledger hydration. [Return](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Audit envelope
`AnalyticsAuditSink`, record, event taxonomy, and in-memory/failing sinks are re-exported from `audit.rs`. The static source defines `metric_emitted`, `cardinality_rejected`, and `budget_exceeded`. Unknown: an outbox adapter or handler response mapping. [Return](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — State and invariants

Static source describes per-metric and global tuple budgets, typed labels that exclude four named high-cardinality labels, and audit-before-mutation behavior. The property-test file is evidence of intended assertions, not an execution result. No runtime persistence, alert, or tenant-isolation result is asserted here.

<a id="r06"></a>
## R06 — Configuration and schema

`AnalyticsConfig` exposes canonical per-metric/global budgets and histogram boundaries in `config.rs`. `lib.rs` includes migration 0015 and returns schema version 15. The migration declares budget and observed-tuple tables. Static evidence cannot show that either table is present in an environment.

<a id="r07"></a>
## R07 — Errors and limits

`AnalyticsError` is re-exported from `error.rs`; `AnalyticsAuditSinkError` is defined in `audit.rs`. `FailingAnalyticsAuditSink` and `FailingRedMetrics` are source fixtures. Their presence does not establish production failure handling, retry, HTTP status, or alert delivery.

<a id="r08"></a>
## R08 — Evidence and unknowns

Evidence set: manifest, eight crate source modules, two declared tests, migration 0015, and static manifests/imports for known consumers. Unknowns: actual Cargo resolution/features, reverse graph completeness, consumer runtime paths, migration application, production bindings, remote-write, operational ownership, and review status.

[Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
