---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-region
manifest: crates/corelink-region/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: candidate
evidence_set: corelink-region-static-source-20260920
---

# corelink-region — blast radius

These are atomic, source-evidenced relations between declarations. They bound
the effect of editing a named contract; they do not identify live consumers or
prove runtime, provider, migration, region, replica, or operational impact.

[Vocabulary](#b01) · [Naming](#b02) · [Jurisdiction](#b03) · [Audit](#b04) · [Metrics](#b05) · [Migration](#b06) · [Consumers and unknowns](#b07) · [Probe interfaces](#b08).

Relation index: [REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011) · [REL-012](#rel-012) · [REL-013](#rel-013) · [REL-014](#rel-014) · [REL-015](#rel-015).

[Reference contracts](REFERENCE.md#r01) · [Maintenance procedures](MAINTENANCE.md#m01) · [Ownership skill](../../../../.claude/skills/own-corelink-region/SKILL.md#s01).

<a id="b01"></a>
## B01 — Region vocabulary relation

<a id="rel-001"></a>
### REL-001 — Region variants to declared vocabulary
**Type/endpoints:** local data; `Region` → `Region::ALL` → iterator/tests. **Surface/activation:** callers iterate the static list. **Contract/effect:** variants and six-member list define source vocabulary. **Failure/propagation:** a mismatch can omit a code value from consumers. **Boundary:** no deployed region inventory. **Validation/coordination:** compare enum, `ALL`, and iterator call sites; coordinate API-001. **Evidence:** `region.rs`.
[Index](#b02)

<a id="b02"></a>
## B02 — Region naming relation

<a id="rel-002"></a>
### REL-002 — Region identifier to constructed names
**Type/endpoints:** local data; `Region::as_str` → four name helpers. **Surface/activation:** helper call with a region. **Contract/effect:** returns constructed R2/D1/KV/domain-shaped strings. **Failure/propagation:** identifier change alters those strings. **Boundary:** no provider resources are proven. **Validation/coordination:** trace each helper to `as_str`; coordinate API-001 and any located consumer. **Evidence:** `region.rs`.
[Index](#b02)

<a id="b03"></a>
## B03 — Jurisdiction relation

<a id="rel-003"></a>
### REL-003 — Region mapping to jurisdiction predicate
**Type/endpoints:** local call; `DoJurisdiction::is_valid_for_region` → `expected_for_region`. **Surface/activation:** explicit predicate call. **Contract/effect:** compares supplied jurisdiction with computed mapping. **Failure/propagation:** changed mapping changes local boolean; error variant is only declared. **Boundary:** no live policy/provider enforcement. **Validation/coordination:** inspect both functions and INV-002; coordinate region API owner. **Evidence:** `region.rs`, `error.rs`.
[Index](#b03)

<a id="b04"></a>
## B04 — Event-to-audit relation

<a id="rel-004"></a>
### REL-004 — Provisioning event constructor to audit sink
**Type/endpoints:** event/interface; `RegionAuditRecord::provisioning` → `RegionAuditSink::emit`. **Surface/activation:** caller constructs and submits a record. **Contract/effect:** constructor assigns `EVT_REGION_PROVISIONED`; sink accepts the record. **Failure/propagation:** changed type/event can break local sink callers. **Boundary:** no provision action or external delivery. **Validation/coordination:** inspect constructor and trait; coordinate API-003. **Evidence:** `event.rs`, `audit.rs`.
[Index](#b04)

<a id="b05"></a>
## B05 — Region vocabulary test-enforcement relation

<a id="rel-005"></a>
### REL-005 — Region test predicate
**Type/endpoints:** test relation; `test_region_all_coverage` → `Region::ALL` and naming helpers. **Surface/activation:** test target execution. **Contract/effect:** source test asserts length six and nonempty strings/name containing identifier. **Failure/propagation:** that predicate fails on changed length/output; it does not compare every enum variant. **Boundary:** test not executed; no live region evidence. **Validation/coordination:** inspect test body and matching helpers. **Evidence:** `region.rs` test module.
[Index](#b05)

<a id="b06"></a>
## B06 — Migration-report relation

<a id="rel-006"></a>
### REL-006 — Migration result to report counters
**Type/endpoints:** local data; `TenantMigrationResult` → `MigrationReport::add_tenant_result` → `has_failures`. **Surface/activation:** supplied result added to report. **Contract/effect:** counters update; `Failed` increments error count. **Failure/propagation:** taxonomy/counter changes alter report outcome. **Boundary:** no tenant migration or rollback is established. **Validation/coordination:** inspect branch and INV-004; coordinate migration API owner. **Evidence:** `migration.rs`.
[Index](#b06)

<a id="b07"></a>
## B07 — Declared consumers and SLO test edge

[Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Migration application dependency
**Type/endpoints:** dependency/reverse consumer; `apps/migrate-single-to-multi-region` → `corelink-region`. **Surface/activation:** manifest direct dependency; application source may import crate symbols. **Contract/effect:** API changes can affect compile-time application compatibility. **Failure/propagation:** source/build break; selected build unknown. **Validation/coordination:** inspect application manifest/imports before change. **Evidence:** both manifests/source; no execution asserted.
[Index](#b07)

<a id="rel-008"></a>
### REL-008 — Replication facade re-export
**Shared key:** `repo:1232040291:boundary:corelink-region-replication-region-facade`. **Type/endpoints:** re-export; `corelink-replication::region` → `corelink-region::*`. **Surface/activation:** consumer imports through facade. **Contract/effect:** renamed public symbol can break facade import path; implementation remains in this package. **Failure/propagation:** source compatibility break. **Validation/coordination:** compare facade `pub use` and manifest; peer reconciliation unknown. **Evidence:** `crates/corelink-replication/{Cargo.toml,src/region.rs}`.
[Index](#b07)

<a id="rel-009"></a>
### REL-009 — SLO test-only dependency
**Type/endpoints:** dev dependency; `corelink-region/Cargo.toml` → package `corelink-slo`. **Surface/activation:** region test target dependency resolution. **Contract/effect:** test-only code can resolve the SLO package; no production edge follows. **Failure/propagation:** package or feature change can affect test-target resolution. **Validation/coordination:** inspect the dev-dependency declaration; the separate metric-binding source assertion is REL-010. **Evidence:** `Cargo.toml`.
[Index](#b07)

<a id="rel-010"></a>
### REL-010 — Region metric/SLI binding assertion
**Shared key:** `repo:1232040291:boundary:corelink-region-slo-sli-test-binding`. **Type/endpoints:** test assertion; `tests/sli_binding.rs` → region metric constants and `corelink_slo::Sli` values. **Surface/activation:** named integration test target is selected. **Contract/effect:** source assertion compares metric-name taxonomy. **Failure/propagation:** either string change can fail the assertion if executed. **Boundary:** test not executed; no metric emission is proven. **Validation/coordination:** inspect the test mapping and coordinate with the SLO owner; matching SLO-side record: [corelink-slo REL-008](../corelink-slo/BLAST_RADIUS.md#rel-008). **Evidence:** `tests/sli_binding.rs`; `Cargo.toml`.
[Index](#b07)

[Relation index](#b01)

<a id="b08"></a>
## B08 — Independent probe interface relations

These are separate typed ports. Their fixture behavior does not establish a
provider query, scheduled worker, metric scrape, or probe execution.
[Relation index](#b08)

<a id="rel-011"></a>
### REL-011 — D1 lag sample and probe port
**Type/endpoints:** local API/data; `D1ReplicaLagProbe::probe(primary, replica, timestamp_ms)` → `D1LagSample`. **Activation:** caller invokes the trait method. **Contract/effect:** sample carries region pair, lag seconds, and timestamp; default metric name is D1-specific. **Failure:** implementation returns `String`; in-memory fixture supplies local values and failing fixture returns its canned error. **Boundary:** no D1 query or caller proven. **Validation/coordination:** inspect trait and fixtures; API-005. **Evidence:** `replica_lag.rs`, `lib.rs`.
[Index](#b08)

<a id="rel-012"></a>
### REL-012 — KV write/read propagation sample and probe port
**Type/endpoints:** local API/data; `KvPropagationProbe::probe(write_region, read_region, timestamp_ms)` → `KvPropagationSample`. **Activation:** explicit trait call. **Contract/effect:** carries directed region pair and lag seconds; helper computes the fraction under a supplied ceiling. **Failure:** `String`; local and failing fixtures are distinct implementations. **Boundary:** no KV write/read or caller proven. **Validation/coordination:** inspect trait, helper, and fixtures; API-006. **Evidence:** `kv_propagation.rs`, `lib.rs`.
[Index](#b08)

<a id="rel-013"></a>
### REL-013 — DO class/region sync-age sample and probe port
**Type/endpoints:** local API/data; `DoSyncAgeProbe::probe(do_class, region, timestamp_ms)` → `DoSyncAgeSample`. **Activation:** explicit trait call. **Contract/effect:** sample carries age seconds and class-specific `within_budget`; the excluded class has no budget. **Failure:** `String`; local fixture rejects non-finite/negative values, while the failing fixture returns its canned error. **Boundary:** no Durable Object or D1 synchronization read proven. **Validation/coordination:** inspect trait/sample/fixtures; API-007. **Evidence:** `do_sync_age.rs`, `lib.rs`.
[Index](#b08)

<a id="rel-014"></a>
### REL-014 — R2 lag and object-presence sample and probe port
**Type/endpoints:** local API/data; `R2CrrProbe::probe(primary, replica, timestamp_ms)` → `R2CrrSample`. **Activation:** explicit trait call. **Contract/effect:** sample combines lag seconds with object presence; separate predicates classify ceiling and missing-object incident. **Failure:** `String`; in-memory fixture validates lag and failing fixture returns its canned error. **Boundary:** no bucket access or copied object proven. **Validation/coordination:** inspect trait/sample/fixtures; API-008. **Evidence:** `r2_crr.rs`, `lib.rs`.
[Index](#b08)

<a id="rel-015"></a>
### REL-015 — Neon lag sample and probe port
**Type/endpoints:** local API/data; `NeonReplicaLagProbe::probe(primary_region, replica_region, timestamp_ms)` → `NeonReplicaLagSample`. **Activation:** explicit trait call. **Contract/effect:** sample carries lag seconds and timestamp; soft ceiling is separately marked informational. **Failure:** `String`; local and failing fixtures are distinct implementations. **Boundary:** no Neon query, caller, or alert proven. **Validation/coordination:** inspect trait/sample/fixtures; API-009. **Evidence:** `neon_replica_lag.rs`, `lib.rs`.
[Index](#b08)

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership skill](../../../../.claude/skills/own-corelink-region/SKILL.md#s01).
