---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-config-do
manifest: crates/corelink-config-do/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-config-do-structural-normalization-20260921
---

# corelink-config-do — blast radius

Static dependency and source-flow map. A manifest dependency, import,
re-export, comment, or test source does not prove a runtime path, provider
integration, or externally observable behavior.

[Workspace](#b01) · [Exports](#b02) · [Consumers](#b03) · [Data contract](#b04) · [Store](#b05) · [Models](#b06) · [Coverage](#b07).

<a id="b01"></a>
## B01 — Workspace relation

The root `Cargo.toml` lists `crates/corelink-config-do` as a workspace member
and declares `corelink-config-do = { path = "crates/corelink-config-do" }` in
workspace dependencies. Impact: a package or manifest-interface change can
affect resolution/builds that select this workspace package. Static source does
not show resolved features or which packages are built in an environment.

<a id="b02"></a>
## B02 — Module-to-root export relation

`src/{error,metrics,propagation,store,types,validation}.rs` feed re-exports in
`src/lib.rs`; `hash` remains public as a module. Impact: changing a re-exported
type, trait, constant, or validation function can change source-level consumer
compatibility. `lib.rs` also exposes `VERSION`. The static map does not
enumerate all downstream use sites.

<a id="b03"></a>
## B03 — Known static consumer relation

`corelink-ops/Cargo.toml` declares a dependency on this package and
`corelink-ops/src/config.rs` re-exports its root API under
`config::durable_object`. The checked ops config API handlers import the store
trait and payload/domain types; checked ops examples import in-memory fixtures,
errors, and types. Impact: root-surface or semantic changes need a source-level
assessment of those relations. This does not prove handler routing, HTTP
operation, authentication, deployment, or actual consumer use.

<a id="b04"></a>
## B04 — Payload/validation/hash relation

`types.rs` → `validation::validate_payload` → `store::{update,rollback_to}`;
the selected store paths also call `hash::compute_payload_hash` and copy the
result into `ConfigVersionEntry::payload_hash`. Impact: schema, map ordering,
domain bound, or hashing changes can alter locally accepted payloads and
in-memory entry values. No persisted-record or cross-process compatibility is
proved.

<a id="b05"></a>
## B05 — In-memory mutation and audit relation

`ConfigSingletonStore` → `InMemoryConfigSingletonStore` →
`ConfigAuditSink`/`MetricsObserver`: update and rollback construct an entry,
call the audit sink before later in-memory mutation, then record source-level
metrics; history/payload maps are pruned on successful mutation paths. Impact:
changing any interface or ordering can affect tests and static consumers. This
is not evidence of a durable transaction, audit delivery, external metric
delivery, or live rollback behavior.

<a id="b06"></a>
## B06 — Observer and snapshot model relation

`metrics.rs` provides constants/outcome vocabularies and an observer trait;
`propagation.rs` provides a change-event data type and version-monotone
`ConfigSnapshot`. Impact: changes to labels, method signatures, event fields,
or comparison semantics affect source consumers and fixtures. The relation
does not demonstrate Prometheus, queue/poll, timing, delivery, or fleet
convergence.

<a id="b07"></a>
## B07 — Target declaration and test-source relation

The manifest’s wasm32 UUID declaration adds the `js` feature to the base UUID
feature set. `tests/prop_cas.rs` is the declared `prop_cas` test target and
contains property-test source for CAS winner/conflict behavior, schema drift,
rollback target handling, and validation bounds. Impact: changes to those
contracts should be reconciled against the manifest and named test source.
Unknown: target compilation, test execution, property-case outcomes, and any
runtime behavior.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
