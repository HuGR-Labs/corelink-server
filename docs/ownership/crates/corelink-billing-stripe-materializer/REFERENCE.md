---
schema: corelink-ownership/1.1
document: reference
package: corelink-billing-stripe-materializer
manifest: crates/corelink-billing-stripe-materializer/Cargo.toml
source_commit: 9f372cc1f5a34752a6b6eb4674df10b3921bce88
profile: H
state: author_validated
evidence_set: billing-materializer-static-graph-20260920
---

# corelink-billing-stripe-materializer — ownership reference

H profile: source, manifest, target-gate, and test-adjacent surface recorded
from static evidence. This is not evidence that D1, a CF Worker, an audit
chain, Stripe delivery, a feature selection, a wasm execution, or a migration
is operating.

[Identity](#r01) · [Boundary](#r02) · [Modules](#r03) · [Contracts](#r04) · [Ports](#r05) · [Features/tests](#r06) · [Relations](#r07) · [Unknowns](#r08).

<a id="r01"></a>
## R01 — Identity and evidence

| Field | Static observation |
|---|---|
| Package / manifest | `corelink-billing-stripe-materializer` / `crates/corelink-billing-stripe-materializer/Cargo.toml` |
| Source baseline | `9f372cc1f5a34752a6b6eb4674df10b3921bce88` |
| Declared role | Stripe-webhook state materialization contracts, orchestration, and native in-memory seams |
| Evidence method | source, manifest, repository text matches, and target/feature attributes; no Cargo command, test, network, runtime, deployment, or publication was run |
| Test-adjacent targets | split internal test modules plus `tests/materializers_e2e.rs` and `tests/wasm32_binders.rs` |

<a id="r02"></a>
## R02 — Ownership boundary

This package owns the local materializer surface: `BillingD1Writer` and
`BillingAuditEmitter` ports, `D1SubscriptionStateHandler`, idempotency bridge,
materialized-row and SQL-literal surface, clock abstraction, tier/runners
bridges, event matrix, package-local fakes, and cfg-gated binder source.

`corelink-billing-stripe-traits` owns the imported dispatcher/materializer port
types and identities. `corelink-cf-bindings` owns `CfD1DatabaseReal`;
`corelink-audit-chain` owns `ArchiveProducer` and its archive contracts;
`corelink-stripe-real` remains a direct dependency for concrete dispatcher/test
seams; and `corelink-tier-selection` owns `TierKind`. Those dependency edges do
not transfer their implementations or operations to this package.

<a id="r03"></a>
## R03 — Twelve-module and test map

| Module / target | Static surface | Boundary |
|---|---|---|
| `lib.rs` | module declarations, public reexports, feature-gated binder exports | package entry point |
| `audit.rs` | audit record/severity, emitter port, in-memory emitter, dispatcher-audit adapter | provider archive remains external |
| `clock.rs` | `MatClock`, native/wasm clock implementations, deterministic fake, target factory | clock source does not prove execution |
| `d1.rs` | D1 writer port, errors, materialized row, SQL constants, in-memory mirror | D1 binding/schema operation remains external |
| `handler.rs` | handler, ten-event matrix, event routing, audit-before-writer ordering, tier/runners calls | provider dispatch and route selection remain external |
| `idempotency.rs` | D1-backed port adapter and outcome mapping | delivery/retry operation remains external |
| `runners.rs` | entitlement resolver port and in-memory mapping | price configuration and entitlement authority remain external |
| `tier.rs` | tier-selector port and in-memory plan map | tier-kind definition and selection implementation remain external |
| `wasm32_binders.rs` | optional CF D1/audit-chain wrapper source | feature selection and Worker dispatch are unproven |
| `tests.rs` | internal test inclusion | tests were not run |
| `tests_part_01.rs` | first split internal test cases | tests were not run |
| `tests_part_02.rs` | second split internal test cases | tests were not run |
| `tests/materializers_e2e.rs`, `tests/wasm32_binders.rs` | integration and feature-gated binder seams | declarations are not execution evidence |

<a id="r04"></a>
## R04 — Local contracts and falsifiable ordering invariant

`EVENT_MATERIALIZATION_MATRIX` in `handler.rs` declares ten canonical event
entries with a table-or-none and audit name. `D1SubscriptionStateHandler`
implements the imported `StateMaterializer` port and `materialize_echo` exposes
the non-dispatcher echo arms. `MaterializedRow` carries a table, tenant, Stripe
object/event identity, payload, and timestamp; `BillingD1Error` distinguishes
transient from invalid-payload errors.

The code-visible audit-before-mutation invariant is falsifiable: in
`do_subscription_upsert`, a `BillingAuditRecord` is passed to
`self.audit.emit_billing(...).map_err(audit_to_mat)?` before the subsequent
`mark_subscription_canceled` or `upsert_subscription` writer call. The
Runners seed/revoke and tier change/downgrade helpers follow the same local
sequence: an audit call with `?`, then the corresponding writer call. Thus an
audit error returns before that helper’s writer invocation. This proves source
ordering only; it does not prove an audit record was durably stored or any D1
mutation occurred.

`subscription_status_grants_access` recognizes only `active` and `trialing`.
`reconcile_tier` skips its active-tier write for other statuses, while the
Runners path chooses a seed or revoke via its resolver. These are handler
branches, not evidence of subscription state, access, or a selected price map.

<a id="r05"></a>
## R05 — Port and compatibility surface

| Surface | Static relation | Compatibility limit |
|---|---|---|
| Traits identities | production sources import `StateMaterializer`, `IdempotencyStore`, `AuditEmitter`, webhook types, and outcomes from `corelink-billing-stripe-traits` | its types and any cross-version policy are owned by that crate |
| Stripe-real edge | manifest declares `corelink-stripe-real`; comments and test-adjacent sources use concrete dispatcher/fake seams | no Stripe endpoint, request, signature observation, or delivery is proven |
| Tier edge | manifest declares `corelink-tier-selection`; `tier.rs` imports `TierKind` | plan mapping, price configuration, and tier implementation compatibility are unknown |
| Local public shape | `lib.rs` reexports local ports, handler, D1 structures/constants, clocks, runners, and tier selector; externally extensible structs/enums are marked `#[non_exhaustive]` in source | N/N-1 callers and stored rows were not resolved or exercised |
| Idempotency | `D1IdempotencyStore` maps `try_record_event` insertion result to first-sight/already-processed outcomes | no actual dedup table, migration, or delivery retry is observed |

<a id="r06"></a>
## R06 — Features, targets, and test seams

`cf-billing-real` is an optional manifest feature that enables optional
`corelink-cf-bindings` and `corelink-audit-chain` dependencies. `lib.rs` gates
the binder module and its reexports with `#[cfg(feature = "cf-billing-real")]`.
That is source wiring only: no feature selection is established.

The manifest declares `js-sys` only under
`cfg(target_arch = "wasm32")`. `clock.rs` gates `SystemMatClock` on the
non-wasm target and `WasmWorkerMatClock` on wasm32; `default_mat_clock` uses
the same target split. The binder source is feature-gated rather than
target-gated at its module inclusion, and its own code describes staged
transients. `tests/wasm32_binders.rs` is feature-gated. These are falsifiable
target/feature declarations; no wasm build, Worker execution, or compatibility
result was obtained.

<a id="r07"></a>
## R07 — Static relation map

The manifest has direct local edges to `corelink-billing-stripe-traits`,
`corelink-stripe-real`, and `corelink-tier-selection`; it has optional edges to
`corelink-cf-bindings` and `corelink-audit-chain`, with the latter two also
listed as dev dependencies. The source maps their roles as described in R02
and R05. These are dependency and source-flow entry points, not an inverse
consumer graph or a runtime topology.

The `corelink-cf-bindings` ownership reference names this package as a static
semantic consumer of `CfD1DatabaseReal`; that relationship still leaves the CF
binding implementation with its own owner. No complete reverse dependency,
route mount, or feature-resolved graph was produced.

<a id="r08"></a>
## R08 — Explicit unknowns and deferred claims

Unknown: human owner/reviewer; complete consumers and feature combinations;
cross-release callers and stored-row compatibility; selected feature/target;
real D1, Worker, audit-chain, Stripe event-delivery, clock, price mapping, and
entitlement behavior; migration application/state; secrets/configuration;
route composition; and all test/build results.

The manifest, source, fakes, test declarations, and static SQL strings do not
close any of those claims. Obtain provider/runtime or consumer-specific
evidence before making them.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Back to identity](#r01)
