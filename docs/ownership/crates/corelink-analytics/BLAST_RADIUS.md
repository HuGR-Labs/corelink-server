---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-analytics
manifest: crates/corelink-analytics/Cargo.toml
source_commit: 59c76cf260bcdeb5246772f70821ac8b7e8a9780
profile: S
state: draft
evidence_set: analytics-static-source-20260920
---

# corelink-analytics — blast radius

Static dependency and source-flow map. A manifest dependency or import proves neither a runtime path nor a public/exported behavior.

[Direct dependency](#b01) · [Exports](#b02) · [Schema](#b03) · [Telemetry](#b04) · [Known consumers](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Direct dependency relation

`corelink-analytics` → `corelink-eviction`: the analytics manifest declares the workspace dependency. Impact: an eviction API or feature change can affect analytics compilation or its source contract. Static graph does not show invocation frequency, runtime reachability, or feature resolution.

<a id="b02"></a>
## B02 — Internal-to-export relation

`src/{audit,canonical,config,error,labels,observer,validator}.rs` → `src/lib.rs` re-exports: changing a re-exported type or constant can affect consumers using the crate root. Impact: source-level API compatibility review is required. Static evidence does not enumerate all downstream usages.

<a id="b03"></a>
## B03 — Migration embedding relation

`migrations/d1/0015_analytics_cardinality_budgets.sql` → `MIGRATION_0015_ANALYTICS_CARDINALITY_BUDGETS` in `src/lib.rs` → `migration_canonical_0015` test source. Impact: SQL text/schema-version changes can invalidate source-pinned migration assumptions. No application to D1 is evidenced.

<a id="b04"></a>
## B04 — Telemetry flow relation

`corelink-telemetry` imports analytics metric kinds, labels, validator, errors, and audit fixtures in static source. Impact: taxonomy, label, or validator changes can require telemetry source changes. This relation does not prove exporter execution, metric delivery, or a live backend.

<a id="b05"></a>
## B05 — Manifest-consumer relation

Static `Cargo.toml` consumers are CLI, tracing, billing-stripe, billing-aggregator, telemetry, container, billing-emit, audit, audit-chain, and audit-chain fuzz. The independent `corelink-audit-chain-fuzz` manifest's `Region::Iad` value edge is [REL-002](../corelink-audit-chain-fuzz/BLAST_RADIUS.md#rel-002), shared key `repo:1232040291:boundary:audit-fuzz-region-api-001`. Impact: a root export or semantic change may require each declared consumer to be assessed. The set is a manifest search result, not reverse-resolution or runtime proof.

<a id="b06"></a>
## B06 — Coverage and closure relation

`prop_analytics` and `migration_canonical_0015` statically name property and migration-shape coverage. Impact: an unrepresented change needs new static acceptance evidence before coverage is claimed. Unknown: test execution, feature combinations, generated consumers, runtime routes, deployment, and all transitive consumers.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
