---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-eviction
manifest: crates/corelink-eviction/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: draft
evidence_set: eviction-source-static-20260920
---

# corelink-eviction — blast radius

This map contains the six required ownership sections, with B05 split into separately reviewable atomic records. SOURCE evidence is static code and test text at the named commit. It does not prove execution, a backend, D1, Cloudflare, scheduling, or production eviction. Canonical verified OKF material remains reachable through [the OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md), without restatement or revalidation here.

[Decision pipeline](#b01) · [State key](#b02) · [Reachability](#b03) · [Soft delete](#b04) · [TTL/quota](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Candidate decision pipeline relation

**Relation:** `InMemoryEvictionPhase` consumes candidate rows, configuration, clock, and trait outputs to produce `EvictionDecision` and `EvictionResult`. **Source endpoints:** [phase.rs](../../../../crates/corelink-eviction/src/phase.rs) and exports in [lib.rs](../../../../crates/corelink-eviction/src/lib.rs).

**Change impact:** changing decision order, result counters, or a decision variant can alter consumers of exported types and static test expectations. **Invariant:** a decision remains explainable as quota skip, TTL skip, reachable skip, or soft-delete result; a source test can falsify a removed branch. **Evidence state:** SOURCE; no pipeline was run. **Unknown:** callers and actual orchestration are untraced.

<a id="b02"></a>
## B02 — Storage-state key and reclaim relation

**Relation:** `TenantStorageStateStore` receives a tenant UUID and `EvictionRegion` for lookup, reclaim, and watermark updates; the fake maps `(Uuid, EvictionRegion)` to a row. **Source endpoints:** [storage_state.rs](../../../../crates/corelink-eviction/src/storage_state.rs).

**Change impact:** changing either key, reclaim arithmetic, or monotonic checks changes the trait contract and fake behavior together. **Invariant:** a reclaim cannot silently underflow `bytes_used`, and a lifecycle watermark cannot regress in the in-memory implementation; tests can supply an underflow or prior higher watermark. **Evidence state:** SOURCE and unexecuted test source only. **Unknown:** real-backend atomicity and schema enforcement are not established.

<a id="b03"></a>
## B03 — Reachability boundary relation

**Relation:** the phase asks `AcReferenceProbe` whether an active reference for the same tenant and digest predates `evict_started_at_ms`; the fake scans only its tenant bucket. **Source endpoints:** [reachable.rs](../../../../crates/corelink-eviction/src/reachable.rs), [phase.rs](../../../../crates/corelink-eviction/src/phase.rs), and [property source](../../../../crates/corelink-eviction/tests/prop_eviction.rs).

**Change impact:** changing `<` to `<=`, dropping tenant scope, or changing the witness contract alters safety decisions. **Invariant:** a matching, non-deleted reference strictly earlier than the anchor is observable as a witness; equality and later timestamps follow the source predicate, not assumed runtime behavior. **Evidence state:** SOURCE. **Unknown:** reference storage, concurrency ordering, and adapter query behavior.

<a id="b04"></a>
## B04 — Blob soft-delete and idempotence relation

**Relation:** the phase calls `BlobMetaSoftDeleteStore::soft_delete_for_eviction` after its checks, and receives either `Deleted { size_bytes }` or `AlreadyResolved`. **Source endpoints:** [blob_meta.rs](../../../../crates/corelink-eviction/src/blob_meta.rs) and [phase.rs](../../../../crates/corelink-eviction/src/phase.rs).

**Change impact:** a change to digest validation, tenant lookup, tombstone handling, or returned size affects LRU result accounting and rerun behavior. **Invariant:** a fake row already tombstoned or absent produces `AlreadyResolved`; a second pass cannot claim the same size again. This is falsifiable with two calls against the same in-memory row. **Evidence state:** SOURCE; physical deletion is excluded. **Unknown:** real-object retention and deletion are uninspected.

<a id="b05"></a>
## B05 — TTL and quota-trigger records

The following records are intentionally independent: changing an Enterprise override, reservation duration, or quota/reclaim arithmetic must identify its own source contract and falsifier. They share no runtime claim.

### B05a — Tier default and override relation

**Relation:** `Tier`, `ttl_for_tier`, and `ttl_for_tier_with_override` map the source tier domain to default TTLs and validate Enterprise-only overrides. **Source endpoint:** [tier.rs](../../../../crates/corelink-eviction/src/tier.rs). **Invariant:** a non-Enterprise override and an Enterprise override over 730 days return an error; both inputs are falsifiers. **Evidence state:** SOURCE, with test source unexecuted. **Unknown:** no caller or external entitlement system is traced.

### B05b — Reservation TTL relation

**Relation:** `reservation_ttl_ms` maps request bytes to a saturating duration bounded by source constants. **Source endpoint:** [reservation.rs](../../../../crates/corelink-eviction/src/reservation.rs). **Invariant:** every result stays at or above the 60,000 ms floor and at or below the seven-day cap; zero, small, and large byte inputs falsify an incorrect boundary. **Evidence state:** SOURCE, with test source unexecuted. **Unknown:** reservation creation, expiry, and upload behavior are untraced.

### B05c — Quota trigger and reclaim relation

**Relation:** `should_fire_quota_trigger` and `target_bytes_to_reclaim` map a supplied state row to a trigger outcome and saturated reclaim target. **Source endpoint:** [trigger.rs](../../../../crates/corelink-eviction/src/trigger.rs). **Invariant:** 95/100 triggers, 94/100 does not, zero quota remains below threshold, and the target is measured from a rounded-up 90% floor. **Evidence state:** SOURCE, with test source unexecuted. **Unknown:** the actual quota authority, dispatch, and resulting reclaim are unobserved.

<a id="b06"></a>
## B06 — Completeness, review, and non-relations

**Completeness criteria:** B01–B04 and B05a–B05c cover the source-visible decision pipeline, state key, reachable check, soft-delete result, tier override, reservation TTL, and quota/reclaim arithmetic; the table below records four known direct workspace consumers. They do not cover uninspected trait implementations, consumers outside this workspace, migration application, clocks, scheduler invocation, or physical object lifecycle. A change is incomplete when it changes an exported symbol or source invariant without updating its relation and R05.

**Success criteria:** each proposed change identifies one or more affected atomic relations, a falsifiable invariant, source links, and an evidence state. **Quality standards:** distinguish SOURCE from execution/runtime; retain declared unknowns; use no comment, fake, or embedded SQL string as proof of external behavior.

**Definition of Done:** update all affected B sections, R05/R06/R07, and the corresponding M section; then obtain independent review for interfaces or ownership outside this crate. No relationship here authorizes deploy, data operation, or runtime conclusion.

**Change review method:** begin at the changed symbol, identify the one relation that carries its direct contract, then inspect adjacent relations only where a value crosses their boundary. For example, a `size_bytes` accounting change starts at B04 and continues to B02.

A change to the trigger threshold starts at B05 and continues to B01. Route shared tier changes through analytics/rate-limit rows, facade exports through CAS, and reservation/region/state changes through billing. This ordering limits speculative claims while ensuring a change cannot bypass the result or state contract. Consumers beyond the statically found workspace remain unknown.

**Evidence discipline:** test-source references demonstrate that assertions exist in the repository. They cannot show that an assertion passed, how a runtime orders events, or whether a backend matches a fake. The same distinction applies to trait comments and embedded migration text.

A relation may therefore be complete for source review and incomplete for integration review at the same time. The latter state is not a defect hidden by this document; it is an explicit reason to route a decision outward.

| Atomic source relation | Evidence and bounded impact | Change route and limit |
|---|---|---|
| `corelink-analytics` → `corelink-eviction::Tier` | `labels.rs` re-exports `Tier` and enumerates its five variants in the canonical list; changes can affect label vocabulary. `crates/corelink-analytics/Cargo.toml:16`; `crates/corelink-analytics/src/labels.rs:19-37`. | Route tier changes to analytics with eviction. No emitted/consumed metric is established. |
| `corelink-ratelimit` → `corelink-eviction::Tier` | `tier.rs` imports the enum, uses it in `TIER_RATE_LADDER`, and matches it in `refill_rate_for_tier`; changes can affect refill mapping. `crates/corelink-ratelimit/Cargo.toml:16`; `crates/corelink-ratelimit/src/tier.rs:25,49-80`. | Route tier changes to rate-limit with eviction. No request-time use is established. |
| `corelink-cas` → `corelink-eviction` public API | `eviction.rs` glob-reexports top-level items and re-exports named submodules; changes can affect CAS facade paths. `crates/corelink-cas/Cargo.toml:23`; `crates/corelink-cas/src/eviction.rs:15-28`. | Route API changes to CAS with eviction. Downstream imports are not exhaustively traced. |
| `corelink-billing` → eviction TTL/region contracts | Reservation source imports `reservation_ttl_ms`/`EvictionRegion`; quota state uses `EvictionRegion` in its typed state key. `crates/corelink-billing/Cargo.toml:38`; `crates/corelink-billing/src/quota/core/reservation.rs:41`; `crates/corelink-billing/src/quota/cas/state.rs:26,34-47`. | Route TTL/region/state changes to billing with eviction. No DO/D1 adapter or live quota behavior is established. |

**Known unknowns:** consumers outside these four statically identified workspace packages; real store/probe implementations; actual transaction and error behavior; scheduler/clock behavior; real audit/metrics transport; and production quota semantics. These are intentional non-relations, not negative claims. They require separate evidence before being represented as behavior.

[Return to decision pipeline](#b01)
