---
schema: corelink-ownership/1.1
document: reference
package: corelink-eviction
manifest: crates/corelink-eviction/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: draft
evidence_set: eviction-source-static-20260920
---

# corelink-eviction — ownership reference

Static source reference for eviction decisions, TTL/LRU logic, quota-trigger calculations, state traits, and test fakes. SOURCE means files inspected at the named commit; execution/runtime is unobserved. Canonical verified OKF material is only routed through [its profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md), not copied, revalidated, or redefined.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Contracts](#r04) · [Invariants](#r05) · [Evidence](#r06) · [Unknowns](#r07) · [Done](#r08).

<a id="r01"></a>
## R01 — Identity and scope

`corelink-eviction` declares `thiserror` and `uuid`, plus `proptest` for development, in [its manifest](../../../../crates/corelink-eviction/Cargo.toml). SOURCE scope is the exported pure/source-local eviction surface: `EvictionPhase`, configuration, decisions, storage traits, blob and reference traits, TTL helpers, trigger helpers, audit/metric interfaces, and in-memory implementations. It is not evidence that any production integration invokes those surfaces.

<a id="r02"></a>
## R02 — Ownership boundary

The crate owns decisions over `BlobLruRow` projections and state supplied through traits. [blob_meta.rs](../../../../crates/corelink-eviction/src/blob_meta.rs) defines a soft-delete trait and an in-memory map; [storage_state.rs](../../../../crates/corelink-eviction/src/storage_state.rs) defines state lookup/reclaim/watermark methods; [reachable.rs](../../../../crates/corelink-eviction/src/reachable.rs) defines the reachability probe. Physical deletion, chunk lifecycle, real persistence, counters outside the supplied row, scheduling, and adapter wiring are out of this source-only ownership claim.

<a id="r03"></a>
## R03 — Source map

| Source | Static responsibility | Evidence state |
|---|---|---|
| [lib.rs](../../../../crates/corelink-eviction/src/lib.rs) | exports modules and embeds a migration text artifact | SOURCE |
| [phase.rs](../../../../crates/corelink-eviction/src/phase.rs) | candidate decision pipeline and aggregate result types | SOURCE |
| [tier.rs](../../../../crates/corelink-eviction/src/tier.rs) | tier domain, defaults, slug mapping, override validation | SOURCE |
| [reservation.rs](../../../../crates/corelink-eviction/src/reservation.rs) | saturating, bounded reservation TTL calculation | SOURCE |
| [trigger.rs](../../../../crates/corelink-eviction/src/trigger.rs) | pure threshold/target functions and captured in-memory closure | SOURCE |
| [tests/prop_eviction.rs](../../../../crates/corelink-eviction/tests/prop_eviction.rs) | property assertions expressed in test source | SOURCE; not executed |

<a id="r04"></a>
## R04 — Static contracts

`should_fire_quota_trigger` returns `BelowThreshold` for zero quota or a ratio below 0.95, and `Fire` at or above 0.95; `target_bytes_to_reclaim` subtracts the rounded-up 90% target with saturation. `reservation_ttl_ms` uses integer arithmetic with a 60,000 ms floor and seven-day cap. `ttl_for_tier_with_override` permits an override only for Enterprise and rejects values over 730 days. The in-memory trigger helper captures a closure until `run_now`; this is a fake API contract, not observed asynchronous behavior. Sources: [trigger](../../../../crates/corelink-eviction/src/trigger.rs), [reservation](../../../../crates/corelink-eviction/src/reservation.rs), and [tier](../../../../crates/corelink-eviction/src/tier.rs).

<a id="r05"></a>
## R05 — Invariants

| Invariant | Falsifiable source observation | Source evidence |
|---|---|---|
| Tenant/region state scope | a state lookup/reclaim uses `(tenant_id, region)`; a changed fake key can be tested against another tenant/region | [storage_state.rs](../../../../crates/corelink-eviction/src/storage_state.rs) |
| Reachable candidate protection | a reference with `created_at_ms < evict_started_at_ms` returns a witness and prevents the delete path | [reachable.rs](../../../../crates/corelink-eviction/src/reachable.rs), [property source](../../../../crates/corelink-eviction/tests/prop_eviction.rs) |
| Soft-delete idempotence | a second fake delete of an already resolved row yields `AlreadyResolved`, not another reclaimed size | [blob_meta.rs](../../../../crates/corelink-eviction/src/blob_meta.rs) |
| TTL bounds | every `reservation_ttl_ms` result is within declared minimum and maximum constants | [reservation.rs](../../../../crates/corelink-eviction/src/reservation.rs) |
| Inclusive quota edge | a row at exactly 95/100 returns `Fire`; 94/100 does not | [trigger.rs](../../../../crates/corelink-eviction/src/trigger.rs) |
| Enterprise cap | non-Enterprise override or an Enterprise value above 730 is an error | [tier.rs](../../../../crates/corelink-eviction/src/tier.rs) |

These are algorithm/fake invariants. They do not establish persistence, concurrency, wall-clock scheduling, or backend transaction behavior.

<a id="r06"></a>
## R06 — Evidence classification

SOURCE evidence consists of the manifest, Rust modules, and test source listed above at `6ed297f5b2b64cf97447985111a2ecbbaa9536bb`. The test files show intended assertions, including tenant isolation, idempotent re-run, BLOB-only scope, TTL bounds, and the quota boundary; they are not an execution result. No execution, runtime, network, deployment, D1, Cloudflare, or production evidence was collected for this artifact.

<a id="r07"></a>
## R07 — Unknowns and exclusions

Known direct workspace consumers are analytics and ratelimit (`Tier`), CAS (public re-export), and billing (reservation TTL/region and quota state); their source arrows and change routes are in [B06](BLAST_RADIUS.md#b06). Still unknown: consumers outside this workspace; which adapter implements each trait; whether a scheduler invokes the phase; persistence and transaction semantics; real quota-counter authority; physical deletion; and runtime metrics/audit delivery. Embedded migration text and source comments do not answer these questions. No claim is made about D1, Cloudflare, or runtime eviction behavior.

<a id="r08"></a>
## R08 — Success and completion standard

**Success criteria:** a maintainer can identify the source-owned decision, its invariant, and its evidence class. **Completeness criteria:** every source-local surface in R03 is covered or explicitly excluded, and B01–B06 including the four known consumer arrows are reconciled.

**Quality standards:** factual claims link to source, examples are falsifiable, and SOURCE is never relabeled execution/runtime. **Definition of Done:** update affected exports, invariants, consumer relations, and maintenance route. Send tier changes to analytics/rate-limit owners, re-export changes to CAS, and reservation/region/state changes to billing. An independent reviewer resolves out-of-scope adapter or lifecycle decisions.

[Return to identity](#r01)
