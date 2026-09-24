---
schema: corelink-ownership/1.1
document: reference
package: corelink-region
manifest: crates/corelink-region/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: candidate
evidence_set: corelink-region-static-source-20260920
---

# corelink-region — ownership reference

This is a static-source reference for typed region, provisioning-event,
migration, audit, metric, and probe contracts. It records declarations and
local fixtures only; it is not evidence of actual regions, replicas, provider
resources, migrations, audit delivery, metrics export, or runtime execution.

[Identity](#r01) · [Boundary](#r02) · [Manifest](#r03) · [Region types](#r04) · [Events and audit](#r05) · [Metrics and probes](#r06) · [Migration](#r07) · [Evidence limits](#r08).

[Blast-radius relations](BLAST_RADIUS.md#b01) · [Maintenance procedures](MAINTENANCE.md#m01) · [Ownership skill](../../../../.claude/skills/own-corelink-region/SKILL.md#s01).

Contract index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006) · [API-007](#api-007) · [API-008](#api-008) · [API-009](#api-009) · [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004).

<a id="r01"></a>
## R01 — Identity

The manifest names package `corelink-region`. `src/lib.rs` declares modules
`audit`, `do_sync_age`, `error`, `event`, `kv_propagation`, `metrics`,
`migration`, `neon_replica_lag`, `r2_crr`, `region`, and `replica_lag`, and
re-exports selected `Region` and probe surfaces. This is the checked package
surface, not a statement about its consumers.

<a id="r02"></a>
## R02 — Source boundary

`metrics::RegionMetrics` is explicitly an in-memory accumulator; audit and
probe modules expose traits plus in-memory and failing fixtures. Source comments
mention provider and worker contexts, but no source inspection here proves a
provider API call, actual replica, active region, probe worker, emitted metric,
or completed migration.

<a id="r03"></a>
## R03 — Manifest declaration

`Cargo.toml` declares `thiserror`, `serde`, `serde_json`, and workspace `uuid`
with `v7`, `serde`, and `js` features. It declares `proptest`, `rand`,
`rand_chacha`, and path development dependency `corelink-slo`, plus named test
targets `prop_region_invariants` and `sli_binding`. This documents manifest
text; no build or test was executed.

<a id="r04"></a>
## R04 — Region and jurisdiction contracts

`region::Region` is a non-exhaustive enum whose `ALL` constant lists `Wnam`,
`Enam`, `Weur`, `Sam`, `Apac`, and `Afr`. Its methods produce identifiers,
location-hint strings, display names, and resource-name-shaped strings.
`DoJurisdiction::expected_for_region` maps the same type to `Us`, `Eu`, or
`None`; `is_valid_for_region` compares against that computed value.

`RegionHealthStatus` declares `Down=0`, `Degraded=1`, and `Healthy=2`, with
`gauge_value` and `as_label`. These are type and method contracts, not evidence
that a health gauge is collected or that any location has that status.

<a id="api-001"></a>
### API-001 — Region identifiers and jurisdiction
**Symbols:** `Region::{ALL,as_str,r2_location_hint,display_name,r2_bucket_name,d1_db_name,kv_namespace_title,custom_domain,from_str}`, `DoJurisdiction::{expected_for_region,is_valid_for_region}`. **Input/precondition:** enum or string input. **Output/errors/effects:** static labels, constructed name strings, or `RegionError`; no resource lookup. **Compatibility:** variants, mappings, and string forms are public source contracts. **Links/evidence:** INV-001/002; REL-001/002; `region.rs`.
[Index](#r04)

<a id="api-005"></a>
### API-005 — D1 replica-lag probe
**Symbols:** `D1LagSample::{primary_region,replica_region,lag_seconds,probe_timestamp_ms}`, `D1ReplicaLagProbe::{probe,metric_name}`, `InMemoryD1ReplicaLagProbe::{new,set_lag}`, `FailingD1ReplicaLagProbe::{new}`, `METRIC_D1_REPLICA_LAG_SECONDS`, `D1_PROBE_CADENCE_SECONDS`, `D1_REPLICA_LAG_P99_CEILING_SECONDS`. **Inputs/units:** primary/replica `Region`, timestamp ms; sample lag seconds. **Output/errors/effects:** sample or `String`; fixture uses local injected values. **Compatibility:** fields, metric literal, cadence and ceiling. **Links/evidence:** REL-011; `replica_lag.rs`.
[Index](#r06)

<a id="api-006"></a>
### API-006 — KV propagation probe
**Symbols:** `KvPropagationSample::{write_region,read_region,lag_seconds,probe_timestamp_ms}`, `KvPropagationProbe::{probe,metric_name}`, `InMemoryKvPropagationProbe::{new,set_lag}`, `FailingKvPropagationProbe::{new}`, `fraction_within_typical`, `METRIC_KV_PROPAGATION_LAG_SECONDS`, `KV_PROBE_CADENCE_SECONDS`, `KV_PROPAGATION_TYPICAL_P99_CEILING_SECONDS`, `KV_PROPAGATION_PESSIMISTIC_P99_CEILING_SECONDS`, `KV_PROPAGATION_TYPICAL_SAMPLE_FRACTION`. **Inputs/units:** write/read `Region`, timestamp ms, lag/ceiling seconds. **Output/errors/effects:** sample or `String`; fraction is `1.0` for empty input. **Compatibility:** sample fields, metric, constants, and empty-set behavior. **Links/evidence:** REL-012; `kv_propagation.rs`.
[Index](#r06)

<a id="api-007"></a>
### API-007 — Durable Object sync-age probe
**Symbols:** `DoClass::{as_str,budget_seconds}`, `DoSyncAgeSample::{within_budget}`, `DoSyncAgeProbe::{probe,metric_name}`, `InMemoryDoSyncAgeProbe::{new,set_age}`, `FailingDoSyncAgeProbe::{new}`, `METRIC_DO_SYNC_AGE_SECONDS`, `DO_SYNC_AGE_TENANT_QUOTA_P99_CEILING_SECONDS`, `DO_SYNC_AGE_CONFIG_SINGLETON_P99_CEILING_SECONDS`, `DO_SYNC_AGE_RATE_LIMITER_BUDGET_SECONDS`. **Inputs/units:** class, `Region`, timestamp ms, age seconds. **Output/errors/effects:** sample or `String`; excluded class has no budget and is treated within budget by the sample method. **Compatibility:** class labels, optional budgets, constants, fields. **Links/evidence:** REL-013; `do_sync_age.rs`.
[Index](#r06)

<a id="api-008"></a>
### API-008 — R2 cross-region replication probe
**Symbols:** `R2CrrSample::{primary_region,replica_region,lag_seconds,object_present,probe_timestamp_ms,within_ceiling,is_missing_incident}`, `R2CrrProbe::{probe,metric_name}`, `InMemoryR2CrrProbe::{new,set_lag}`, `FailingR2CrrProbe::{new}`, `METRIC_R2_CRR_LAG_SECONDS`, `R2_CRR_PROBE_CADENCE_SECONDS`, `R2_CRR_LAG_P99_CEILING_SECONDS`, `R2_CRR_OBJECT_MISSING_INCIDENT_SECONDS`. **Inputs/units:** primary/replica `Region`, timestamp ms, injected lag seconds and object-presence bool. **Output/errors/effects:** sample or `String`; incident predicate combines absence with its threshold. **Compatibility:** fields, predicate, metric and constants. **Links/evidence:** REL-014; `r2_crr.rs`.
[Index](#r06)

<a id="api-009"></a>
### API-009 — Neon replica-lag probe
**Symbols:** `NeonReplicaLagSample::{primary_region,replica_region,lag_seconds,sample_timestamp_ms,within_soft_ceiling}`, `NeonReplicaLagProbe::{probe,metric_name}`, `InMemoryNeonReplicaLagProbe::{new,set_lag}`, `FailingNeonReplicaLagProbe::{new}`, `METRIC_NEON_REPLICA_LAG_SECONDS`, `NEON_PROBE_CADENCE_SECONDS`, `NEON_REPLICA_LAG_P99_SOFT_CEILING_SECONDS`, `NEON_SLO_IS_INFORMATIONAL`. **Inputs/units:** primary/replica `Region`, timestamp ms, lag seconds. **Output/errors/effects:** sample or `String`; fixture uses injected local values. **Compatibility:** fields, metric, cadence, informational flag and soft ceiling. **Links/evidence:** REL-015; `neon_replica_lag.rs`.
[Index](#r06)

<a id="api-002"></a>
### API-002 — Health and in-memory metric accumulator
**Symbols:** `RegionHealthStatus::{gauge_value,as_label}`, `RegionMetrics::{record_provisioning_duration,set_health_status,record_drift_finding,record_migration_progress,record_outage_event}`. **Input/precondition:** supplied region, value, and label. **Output/effects:** mutates local vectors. **Errors/compatibility:** no exporter or validation error is declared here. **Links/evidence:** INV-003; REL-003; `metrics.rs`.
[Index](#r04)

<a id="api-003"></a>
### API-003 — Event and audit interfaces
**Symbols:** `RegionAuditRecord::{provisioning,migration_tenant_completed}`, `RegionAuditSink::emit`, `InMemoryRegionAuditSink`, `FailingRegionAuditSink`. **Input/precondition:** supplied event fields/record. **Output/errors/effects:** record value or sink result; fixture stores only in memory. **Compatibility:** event strings and record fields are source contracts. **Links/evidence:** REL-004; `event.rs`, `audit.rs`.
[Index](#r04)

<a id="api-004"></a>
### API-004 — Migration report
**Symbols:** `MigrationReport::{new_dry_run,new_execute,add_tenant_result,complete,tenant_count,count_by_decision,has_failures}`. **Input/precondition:** supplied mode, time, and `TenantMigrationResult`. **Output/effects:** updates local counters and completion timestamp. **Errors/compatibility:** increments error count for `Failed` only. **Links/evidence:** INV-004; REL-006; `migration.rs`.
[Index](#r04)

<a id="r05"></a>
## R05 — Provisioning-event and audit contracts

`event.rs` declares seven `corelink.region.*` event constants and
`RegionAuditEventType`; `RegionAuditRecord` has CloudEvent-shaped fields and
constructors for provisioning and tenant-completed migration records.
`audit::RegionAuditSink` requires `emit`, while `InMemoryRegionAuditSink` saves
records in a vector and `FailingRegionAuditSink` returns `RegionError`.

The constructors and sinks are source-visible contracts. They do not prove
Terraform application, resource provisioning, audit transport, or an event
emitted outside process memory.

<a id="r06"></a>
## R06 — Metrics and probe contracts

`metrics.rs` declares five canonical region metric-name constants and
`RegionMetrics` vectors for duration, health, drift, migration progress, and
outage data. Its methods append or update those vectors; it contains no
exporter implementation. Migration progress accepts a `tenant_id_hash` string,
but source inspection cannot prove the caller supplied a hash.

The five probe families expose distinct typed sample/trait contracts: D1
replica lag, KV propagation lag, DO sync age, R2 CRR lag/object presence, and
Neon replica lag. Their separate inputs, outputs, thresholds, and failure
contracts are recorded as API-005 through API-009 above. Each has in-memory
and failing fixtures. These are interface and fixture evidence only; no actual
replica, provider query, synthetic write, alert, or metric series is proven.

<a id="r07"></a>
## R07 — Migration and errors

`migration::MigrationDecision` declares `Migrate`, two skip variants, and
`Failed`. `MigrationReport` constructors create dry-run or execute report
values; `add_tenant_result` adds counts and increments `error_count` only for
`Failed`; `complete` sets completion time. `RegionError` is non-exhaustive and
declares unknown-region, jurisdiction, audit, migration, rollback, R2-location,
and serialization categories.

These values model a migration result. They do not show data copied, hashes
verified, a rollback initiated, or a migration command being invoked.

<a id="inv-001"></a>
### INV-001 — Region parsers accept only declared variants
**Predicate:** `Region::from_str` returns a variant only for one of the six declared identifiers. **Enforcement:** match arms in `region.rs`. **Violation:** an undeclared identifier succeeds or declared spelling maps to another variant. **Verification:** compare parser arms to `Region::ALL` and parser tests; execution unknown.
[Index](#r07)

<a id="inv-002"></a>
### INV-002 — Jurisdiction derives from region mapping
**Predicate:** `is_valid_for_region(r)` equals comparison with `expected_for_region(r)`. **Enforcement:** `region.rs`. **Violation:** validity accepts a value unequal to the computed mapping. **Verification:** inspect mapping/comparison and tests; execution unknown.
[Index](#r07)

<a id="inv-003"></a>
### INV-003 — Metrics mutation stays local
**Predicate:** each `RegionMetrics` recorder changes only its corresponding in-memory vector or status slot. **Enforcement:** methods in `metrics.rs`. **Violation:** a method writes external state or updates another vector. **Verification:** inspect each method body; no exporter is declared in this package.
[Index](#r07)

<a id="inv-004"></a>
### INV-004 — Failed migration result increments error count
**Predicate:** `add_tenant_result` increments `error_count` exactly for `MigrationDecision::Failed`. **Enforcement:** `migration.rs`. **Violation:** another decision increments it or `Failed` does not. **Verification:** inspect branch and tests; execution unknown.
[Index](#r07)

<a id="r08"></a>
## R08 — Evidence limits and unknowns

Evidence is limited to `Cargo.toml` and `src/*.rs` at `source_commit`. Unknown:
which regions or provider resources exist; whether routing, Durable Objects,
D1, R2, KV, Neon, or replicas are configured; whether probes run; whether
metrics or audit records leave memory; and whether any migration completed.
`WAVE_007_PLAN` and the verified canonical OKF are routing/reference inputs,
not evidence revalidated, copied, or redefined by this record.
