---
schema: corelink-ownership/1.1
document: reference
package: corelink-tier-selection
manifest: crates/corelink-tier-selection/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: S
state: author_validated
evidence_set: w011-tier-selection-source-static-20260920
---

# corelink-tier-selection — ownership reference

SOURCE-only reference for the tier taxonomy, local gate declarations, and
in-memory fixtures. It records checked-in Rust and the manifest at the pinned
baseline. It does not establish a caller, external communication, durable
state, execution, deployment, or independent review. The verified OKF route is
routing context only; it is not repeated or revalidated by this packet.

[Identity](#r01) · [Taxonomy](#r02) · [Identifiers](#r03) · [Gates](#r04) · [Axioms](#r05) · [Local state](#r06) · [Fakes](#r07) · [Unknowns](#r08)

<a id="r01"></a>
## R01 — Package identity and static boundary

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| The manifest names `corelink-tier-selection`; the crate root forbids unsafe code, declares seven public modules, and re-exports the named public surface. | Rename the package or alter the root attribute, module declarations, or re-exports in `Cargo.toml` or `src/lib.rs`. This does not establish a selected build or caller. |

The local source inventory is `audit`, `dpa`, `error`, `ledger`, `stripe`,
`tenant`, and `tier`. Names in that inventory describe source files only.

<a id="r02"></a>
## R02 — Tier taxonomy contract

`TierKind` is a `#[non_exhaustive]` enum with six customer-facing variants:
`Free`, `Solo`, `Starter`, `Pro`, `Max`, and `Enterprise`. Its separate runner
axis contains `RunnerStarter`, `RunnerPro`, `RunnerTeam`, `RunnerScale`, and
`RunnerMax`. `canonical_tiers` returns the six-tier array; `canonical_runner_tiers`
returns the five runner-tier array. `as_str`, `requires_stripe_checkout`, and
`routes_to_inquiry_form` are local source helpers. Evidence: `src/tier.rs`.

These declarations do not prove a published catalog, entitlement, route, or
caller interpretation.

<a id="r03"></a>
## R03 — Identifier and context contract

`TenantId` and `StripeCustomerId` are opaque string newtypes with constructors
and string borrows. `TenantCtx` is non-exhaustive and exposes `tenant_id`,
`now_ms`, and `correlation_id`; `new` supplies those three fields. This is a
static Rust shape, not proof that identifiers are validated, unique, persisted,
or received by a request. Evidence: `src/tenant.rs`.

<a id="r04"></a>
## R04 — Gate, audit, and error declarations

`DpaAcceptanceGate` is a `Debug + Send + Sync` trait with one acceptance lookup
method. `TierSelectionAuditSink` is a `Debug + Send + Sync` trait with one
record-emission method. The audit enum is non-exhaustive and its canonical list
function returns eight source strings. `TierError` is a non-exhaustive typed
error taxonomy. These are declarations and local control-flow vocabulary only.
Evidence: `src/{dpa,audit,error}.rs`.

<a id="r05"></a>
## R05 — Five falsifiable source axioms

| ID | Falsifiable SOURCE axiom | Static falsifier / limit |
|---|---|---|
| AX-TS-01 | `canonical_tiers` lists exactly `Free`, `Solo`, `Starter`, `Pro`, `Max`, and `Enterprise` in that order. | Change an array member or order in `src/tier.rs`; no catalog result follows. |
| AX-TS-02 | `canonical_runner_tiers` lists exactly five runner variants, separate from `canonical_tiers`. | Alter either array or mix the two functions in `src/tier.rs`; no entitlement result follows. |
| AX-TS-03 | `InMemoryDpaGate` stores `(TenantId, String)` keys in `Arc<Mutex<HashMap<..., bool>>>`; a poisoned lookup returns `false`. | Change the field or poisoned-lock branch in `src/dpa.rs`; no durable acceptance result follows. |
| AX-TS-04 | `AlwaysDenyDpaGate::is_accepted` returns `false` for every input. | Change its implementation in `src/dpa.rs`; this is an adversarial fixture, not a policy decision. |
| AX-TS-05 | `FailingTierSelectionAuditSink::emit` returns `TierSelectionAuditEmitError::Rejected`. | Remove or change that branch in `src/audit.rs`; no audit delivery result follows. |

<a id="r06"></a>
## R06 — Local selection-state declarations

`TierSelectionLedger` holds a local `Arc<Mutex<LedgerInner>>` and trait-object
fields for a gate, a client-shaped port, and an audit sink. The public local
types include `TierSelectionReceipt`, `SubscriptionActivationReceipt`,
`SubscriptionState`, `TierSelectionRow`, `CheckoutSessionRow`, and
`TIER_SELECTION_LOCK_WINDOW_MS`. `TierSelectionRow::holds_paid_subscription`
is a source predicate over its local fields. Evidence: `src/ledger.rs`.

This inventory is not evidence that any record exists outside process memory,
that a lock is shared, or that a selection was made.

<a id="r07"></a>
## R07 — In-memory and adversarial fixture contract

`InMemoryDpaGate` exposes `new`, `accept`, and `revoke`; `AlwaysDenyDpaGate`
always denies. `InMemoryTierSelectionAuditSink` keeps local records and exposes
`snapshot`, `len`, `is_empty`, and `has_event`; its failing counterpart always
rejects. `InMemoryStripeClient` stores generated responses locally, exposes
`new`, `arm_failure`, and `sessions`, and implements its local trait. Each is a
fixture declaration with mutex-backed memory. Evidence: `src/{dpa,audit,stripe}.rs`.

No fixture demonstrates an external adapter, transmission, persistence,
concurrency guarantee, or test pass.

<a id="r08"></a>
## R08 — Evidence limit and five unknowns

Evidence mode is SOURCE: the manifest and named local text at the pinned
baseline. DOCUMENTARY evidence can establish only structural-document results.

1. Selected target, feature resolution, compilation, linking, and test outcome are UNKNOWN.
2. Complete caller graph, compatibility, invocation order, and reachability are UNKNOWN.
3. Identifier provenance, validation, authorization, and correlation use are UNKNOWN.
4. Durable state, sharing, locking, timing, and recovery outside local memory are UNKNOWN.
5. External communication, environment configuration, deployment, observation, and independent review are UNKNOWN.

Claiming an unknown requires separately selected evidence. The OKF is available
only as the designated route, never as proof for a local-source assertion.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-corelink-tier-selection/SKILL.md#s01)
