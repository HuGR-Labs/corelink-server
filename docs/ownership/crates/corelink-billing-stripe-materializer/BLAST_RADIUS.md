---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-billing-stripe-materializer
manifest: crates/corelink-billing-stripe-materializer/Cargo.toml
source_commit: 9f372cc1f5a34752a6b6eb4674df10b3921bce88
profile: H
state: author_validated
evidence_set: billing-materializer-static-graph-20260920
---

# corelink-billing-stripe-materializer — blast radius

Static dependency, flow, and impact relations only. They identify inspection
scope when a source surface changes; they do not prove a Stripe delivery, D1
write, audit delivery, Worker execution, feature selection, or migration.

[Local surface](#b01) · [Provider edges](#b02) · [Ordering](#b03) · [Target gates](#b04) · [Composition](#b05) · [Gaps](#b06).

<a id="b01"></a>
## B01 — Local-port and handler relation

`lib.rs` reexports local writer/audit ports, handler, D1 shapes/constants,
clock, idempotency, runners, and tier bridge. A change to a port signature,
public reexport, `MaterializedRow`, `BillingAuditRecord`, error, SQL literal,
or event matrix can affect the handler, package fakes, split tests, and static
callers that compile against the exposed surface. Inspect `lib.rs`, the named
module, both integration test files, and direct manifest/source edges. No
complete consumer set or release compatibility is known.

<a id="b02"></a>
## B02 — Materializer-to-provider relation

The materializer imports webhook trait/identity types from
`corelink-billing-stripe-traits`, `TierKind` from `corelink-tier-selection`,
and declares a direct `corelink-stripe-real` dependency. Its optional binder
source imports `CfD1DatabaseReal` from `corelink-cf-bindings` and
`ArchiveProducer`-related types from `corelink-audit-chain`.

These are directed static dependency/adapter relations: a provider contract
change may require materializer inspection, and a materializer-side port use
may require provider-owner coordination. They do not make this crate owner of
CF binding, audit-chain, Stripe-real, trait, or tier implementation behavior.

<a id="b03"></a>
## B03 — Audit-before-writer flow relation

In handler mutation helpers, `emit_billing(...)?` precedes the corresponding
`BillingD1Writer` call. This order is visible for subscription upsert/cancel,
Runners seed/revoke, tier change/downgrade, and the remaining per-event helper
paths. Changing those helpers, the audit port, the writer port, or error
mapping can alter that source-level fail-closed sequence; inspect both calls
together. An audit failure returns before the local writer invocation, but no
durable audit or D1 outcome follows from that control flow alone.

<a id="b04"></a>
## B04 — Feature-and-target-gate relation

`cf-billing-real` controls inclusion/reexport of `wasm32_binders.rs` and its
two optional provider dependencies. The manifest’s wasm32-only `js-sys` edge
and `clock.rs` target cfgs select different clock type source on wasm32 versus
non-wasm32. A change to the feature, module cfg, optional dependency, target
dependency, or binder can affect which source is available under a chosen
build configuration. It cannot establish that configuration was selected,
compiled, or run.

<a id="b05"></a>
## B05 — Package-to-external-composition relation

The package defines seams and source-level adapters; a caller must supply
concrete writer, audit emitter, tier selector, and any dispatcher/composition.
CF binding, audit archive, Stripe handling, D1 schema/migrations, routes,
credentials, feature flags, and Worker boot/async dispatch sit outside this
ownership boundary. A requested operational or routing change must cross to
those owners. Source references to provider types, SQL, or staging diagnostics
are not evidence of an external effect.

<a id="b06"></a>
## B06 — Coverage limits and stop conditions

The static map does not establish all consumers, resolved feature sets, target
builds, historical stored-data readers, or compatibility policy. It also does
not establish D1 table existence, migration application, Worker execution,
audit-chain persistence, Stripe delivery/retry, price configuration, or route
mounting. Stop when a conclusion needs any omitted evidence; preserve the
changed symbol and inspected paths, then obtain the responsible consumer,
provider, or runtime evidence. A fake, test declaration, SQL string, or
manifest feature is not recovery evidence for external state.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Back to local surface](#b01)
