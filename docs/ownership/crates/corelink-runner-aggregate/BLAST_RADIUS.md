---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-runner-aggregate
manifest: crates/corelink-runner-aggregate/Cargo.toml
source_commit: cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6
profile: S
state: author_validated
evidence_set: runner-aggregate-main-readback-20260923
---

# corelink-runner-aggregate — blast radius

This is a source-only relation map pinned to `cd74a094c34ea80fb7e1914bf8a0da5fdf6216b6`. A Cargo declaration is build intent; a call site is source wiring; neither proves a shipped target or observed runtime path. Verified OKF remains canonical and is routed through the [wave plan](../../WAVE_010_PLAN.md).

[Scope](#b01) · [Method](#b02) · [Direct relations](#b03) · [Local propagation](#b04) · [Compatibility and tests](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Scope

Inspected boundary: `Cargo.toml`, `src/lib.rs`, `src/bin/runner-aggregate-run.rs`, and `tests/runner_aggregate_run_bin.rs` at the pin. No reverse-consumer, workflow, deployment, database migration, or external repository census is established. Relation direction labels distinguish dependency, data flow, and impact propagation.

<a id="b02"></a>
## B02 — Method and populations

Manifest inventory yields nine direct dependency declarations: six external crates and three CoreLink packages; the manifest declares one bin target, no explicit package features, and no target-specific dependencies. Source inspection confirms imported symbols and local transformations below. Tests are source relations only. No resolved Cargo graph, inverse query, target/feature selection, or semantic search beyond this package was performed. Therefore reverse consumers, runtime reachability, and deployment remain unknown.

<a id="b03"></a>
## B03 — Direct dependency relations

[REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009).

<a id="rel-001"></a>
### REL-001 — `corelink-billing-emit` vocabulary

**Type/endpoints:** dependency; this package → `corelink-billing-emit`. **Surface/activation:** `IdemKey`, `UsageEventKind::RunnerVcpuSeconds` in `src/lib.rs`; each valid unique key and aggregate construction. **Contract/effect:** decoded bytes become typed keys and event kind. **Failure propagation:** symbol/representation changes affect compilation or builder input semantics. **Boundary:** producer identity and event publication are unknown. **Validation/coordination:** inspect local use and defining package change together. **Evidence:** manifest/import/append arguments. [Direct index](#b03)

<a id="rel-002"></a>
### REL-002 — `corelink-billing-aggregator` chain contract

**Type/endpoints:** dependency; this package → `corelink-billing-aggregator`. **Surface/activation:** `AggregatedCounter`, `ChainHash`, `HashChainBuilder::{new,resume,append}` in `src/lib.rs`. **Contract/effect:** local grouped data is supplied to aggregate construction and append; returned hashes/state populate local rows and updates. **Failure propagation:** input encoding/order or imported API/hash semantics can change chain bytes/errors. **Boundary:** durable chain storage and concurrent writers unknown. **Validation/coordination:** trace fields into imported constructor and returned values into outputs; coordinate with defining owner. **Evidence:** manifest and call sites. [Direct index](#b03)

<a id="rel-003"></a>
### REL-003 — `corelink-runner-overage` arithmetic helpers

**Type/endpoints:** dependency; this package → `corelink-runner-overage`. **Surface/activation:** `SECONDS_PER_VCPU_HOUR`, `overage_vcpu_hours_decimal`, `millicents_to_cents` in charge construction. **Contract/effect:** unit conversion/display values are derived by imported helpers. **Failure propagation:** changed units/rounding affect `ShadowLine` values and consumer interpretation. **Boundary:** no charge or Stripe call is present in this package. **Validation/coordination:** trace helper inputs/outputs and coordinate if helper contracts change. **Evidence:** manifest and imports/calls. [Direct index](#b03)

<a id="rel-004"></a>
### REL-004 — `serde` serialization derives

**Type/endpoints:** dependency; public data types → `serde`. **Surface/activation:** derives on public input/output, event, prior, terms, row, line, and error-adjacent types. **Contract/effect:** types implement serialization/deserialization traits; `RunnerAggregateInput` denies unknown fields and defaults absent `prior_chain_heads`. **Failure propagation:** field/type/attribute changes alter trait behavior and accepted input shape. **Boundary:** no external consumer inventory or compatibility guarantee. **Validation/coordination:** inspect every affected derive/attribute and direct caller. **Evidence:** manifest and `src/lib.rs`. [Direct index](#b03)

<a id="rel-005"></a>
### REL-005 — `serde_json` binary JSON codec

**Type/endpoints:** dependency; stdin bytes → `RunnerAggregateInput`, `RunnerAggregateOutput` → stdout JSON. **Surface/activation:** parse and pretty serialization in bin `main`. **Contract/effect:** parse/serialize errors take failure exits; successful output is one JSON line. **Failure propagation:** serde shape/codec changes can reject input or prevent output. **Boundary:** no invocation/observed wire consumer. **Validation/coordination:** inspect bin branches and public serde contract. **Evidence:** manifest, bin source, integration-test source. [Direct index](#b03)

<a id="rel-006"></a>
### REL-006 — `hex` fixed-size decoding

**Type/endpoints:** dependency; text fields → 32-byte key/head. **Surface/activation:** `parse_hex32` on each staged event and visited supplied chain head; canonical check on terms digest. **Contract/effect:** decoding gates grouping/resume and terms acceptance. **Failure propagation:** decoding/canonicalization change alters fail-closed branches. **Boundary:** no independently versioned external hex protocol established. **Validation/coordination:** trace malformed and uppercase/noncanonical inputs against source predicates. **Evidence:** manifest and helpers. [Direct index](#b03)

<a id="rel-007"></a>
### REL-007 — `blake3` terms digest

**Type/endpoints:** dependency; canonical terms fields → BLAKE3 digest hex. **Surface/activation:** `TenantPeriodTerms::expected_snapshot_digest_hex`, validation of supplied digest. **Contract/effect:** digest binds tenant, period, allowance, rate, and ref using explicit domain prefix and big-endian length/value encoding. **Failure propagation:** field order/domain/encoding/hash changes invalidate snapshots/prior bindings. **Boundary:** authoritative snapshot store and independent digest producer unknown. **Validation/coordination:** compare exact canonical byte recipe with any producer before changing. **Evidence:** manifest and method/validator. [Direct index](#b03)

<a id="rel-008"></a>
### REL-008 — `uuid` deterministic IDs

**Type/endpoints:** dependency; `uuid` v5/serde features → public UUID fields and aggregate ID. **Surface/activation:** deserialize tenant IDs; `Uuid::new_v5` per `(tenant,region,period)`. **Contract/effect:** fixed namespace plus formatted name produces aggregate ID; changing either changes downstream chain input. **Failure propagation:** feature/API or identity-input changes affect serde/build or chain digest. **Boundary:** persistence/uniqueness consumers unknown. **Validation/coordination:** preserve namespace, delimiters, and fields or explicitly coordinate compatibility. **Evidence:** manifest feature declaration and `lib.rs`. [Direct index](#b03)

<a id="rel-009"></a>
### REL-009 — `thiserror` local diagnostics

**Type/endpoints:** dependency; `RunnerAggregateError` → standard error formatting. **Surface/activation:** typed local validation/arithmetic errors. **Contract/effect:** derive provides error trait/display behavior. **Failure propagation:** variant/source changes affect matching or diagnostics. **Boundary:** error text is not established as a network protocol. **Validation/coordination:** inspect enum uses and callers if identified. **Evidence:** manifest and enum. [Direct index](#b03)

<a id="b04"></a>
## B04 — Local data flow and propagation

[REL-010](#rel-010) · [REL-011](#rel-011) · [REL-012](#rel-012) · [REL-013](#rel-013) · [REL-014](#rel-014) · [REL-015](#rel-015). [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Artifact version and terms gate

**Type/endpoints:** local validation/data; input artifact and tenant terms → aggregation. **Surface/activation:** function entry checks version 3; terms/prior/staged-tenant loops validate bindings before aggregation. **Contract/effect:** unsupported version or invalid/missing snapshot returns typed error before output. **Failure propagation:** relaxing checks admits incompatible or unbound pricing inputs. **Boundary:** source of terms/version negotiation unknown. **Validation/coordination:** trace each fail-closed branch and any producer contract. **Evidence:** `aggregate_runner_usage`, `validate_terms`. [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Batch idempotency and grouped counters

**Type/endpoints:** local data flow; staged records → deduplicated `(region,tenant)` accumulators → `CounterRow`. **Surface/activation:** global `BTreeSet` before nested `BTreeMap`; checked quantity and count increments. **Contract/effect:** duplicate decoded key contributes nothing; ordering is stable; each group creates one row. **Failure propagation:** altered key scope/group ordering changes counts, chain input, and later tenant totals; overflow fails the call. **Boundary:** uniqueness outside a single supplied batch is not established. **Validation/coordination:** compare duplicate, ordering, and overflow predicates. **Evidence:** grouping loop and INV-003/004. [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Group to hash-chain row and region update

**Type/endpoints:** local data flow using REL-002; grouped tenant values + prior region head → counter rows and head updates. **Surface/activation:** one imported aggregate/append per ordered group; one update per visited region. **Contract/effect:** pre-head and append digest populate row; post-head/sequence populate update. **Failure propagation:** mapping/order/resume changes affect chain continuity and output consumers; builder rejection becomes `ChainBreak`. **Boundary:** no persistence, transaction, or concurrency guarantee. **Validation/coordination:** verify pre/post mapping and imported contract. **Evidence:** chain loop. [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — Tenant totals to cumulative shadow line

**Type/endpoints:** local data flow using REL-003; region quantities + prior usage + terms → tenant shadow line. **Surface:** tenants with new grouped usage. **Contract/effect:** checked cumulative totals, tenant-wide allowance, and charge delta. **Failure:** scope, snapshot, units, or rounding changes alter values; overflow errors. **Boundary:** no provider side effect. **Validation/coordination:** preserve partition/allowance predicates and helper units. **Evidence:** shadow loop. [Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — Manifest binary target to process shell

**Type/endpoints:** build target/source; manifest `runner-aggregate-run` → declared bin source. **Surface/activation:** target selection by a build or caller, neither observed here. **Contract/effect:** source defines stdin/JSON/library/stdout and exit branches. **Failure propagation:** target name/path or shell behavior changes can affect build/caller compatibility. **Boundary:** no build, invocation, workflow schedule, or deployment proof. **Validation/coordination:** compare manifest target and bin imports/branches. **Evidence:** `Cargo.toml`, bin source. [Relation index](#b03)

<a id="rel-015"></a>
### REL-015 — Integration test source to bin behavior

**Type/endpoints:** test; `tests/runner_aggregate_run_bin.rs` → compiled binary process contract. **Surface/activation:** test helper spawns `runner-aggregate-run` with stdin and captures exit/stdout/stderr. **Contract/effect:** source cases assert valid JSON/charge output, malformed JSON failure/no stdout, and empty staged success. **Failure propagation:** changed shell contract can break these assertions if executed. **Boundary:** tests are checked in but no execution result is known. **Validation/coordination:** inspect test expectations; obtain execution evidence only through an authorized local test procedure. **Evidence:** integration-test source. [Relation index](#b03)

<a id="b05"></a>
## B05 — Change to impact to validation

**Event keys/grouping** → counters, chain input, tenant usage; validate REL-006/011 and INV-003/004.

**Terms/digest/prior state** → acceptance and charge delta; validate REL-007/010/013 and binding/partition test source.

**Chain inputs/mapping** → row digests and region continuation; validate REL-002/012 and INV-005.

**Shadow arithmetic/conversions** → overage and amount fields; validate REL-003/013 and INV-006.

**Serde/errors/bin shell** → JSON, diagnostics, and exit behavior; validate REL-004/005/009/014/015 and integration-test source.

**Manifest dependency/feature/target** → build graph or target compatibility; inspect REL-001–009/014. Resolved target evidence is absent.

These are source review routes, not executed validation results. Coordinate changes with each defining dependency owner when its contract changes.

<a id="b06"></a>
## B06 — Coverage, unknowns, and done gate

Coverage: all nine direct manifest declarations; imported symbols observed in local code; terms, dedup/group, chain, shadow, process-shell, and integration-test source relations. Reverse consumers, aliases/re-exports, resolved/inverse graph, selected features/target inclusion, workflows, migrations/schema, data stores, config, deployment, telemetry, SDKs, external APIs, and runtime observations were not censused. No consumer or deployment count is asserted.

Success means each material local edge has its own producer/consumer/surface/activation/failure boundary. Completeness means B01–B05 cover declaration and inspected semantic paths with explicit unknowns. Quality means dependency declaration, source wiring, and runtime reachability stay distinct. Done for this artifact requires its structural checker, `git diff --check`, source reconciliation, and independent cold review; a checker pass is not approval.

[Back to start](#b01)
