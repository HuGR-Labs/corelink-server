---
schema: corelink-ownership/1.1
document: reference
package: corelink-rotation-adapters
manifest: crates/corelink-rotation-adapters/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: author_validated
evidence_set: w009-rotation-adapters-source-static-20260920
---

# corelink-rotation-adapters — ownership reference

This is an S-profile SOURCE record for the checked-in adapter/rotation source
surface. It records declarations and code predicates, not a key, provider, or
rotation operation. The verified OKF manifest lookup returned no matching
concept; no policy is copied, redefined, or revalidated here.

[Identity](#r01) · [Root](#r02) · [Asset types](#r03) · [State predicates](#r04) · [Trait](#r05) · [Adapters/errors](#r06) · [Axioms](#r07) · [Evidence](#r08)

<a id="r01"></a>
## R01 — Identity invariant

| Atomic predicate | Static falsifier / evidence |
|---|---|
| The manifest names `corelink-rotation-adapters`, declares `thiserror` and `serde` dependencies, and declares `prop_rotation_invariants` as a test target. | Renaming/removing the package, dependency declarations, or test-target declaration in `Cargo.toml` falsifies this source inventory. A declared target is not an executed test. |

<a id="r02"></a>
## R02 — Root-route invariant

| Atomic predicate | Static falsifier / evidence |
|---|---|
| `src/lib.rs` declares nine public modules; re-exports the two predicates, `RotationAdapter`, six named adapter structs, `RotationError`, and the three type names; `rotation_adapters_schema_version()` returns literal `1`. | Removing/renaming a `pub mod`, matching `pub use`, or changing the literal in `src/lib.rs` falsifies the local export declaration. |

<a id="r03"></a>
## R03 — Asset-and-handle invariant

| Atomic predicate | Static falsifier / evidence |
|---|---|
| `AssetClass` and `KeyState` are `#[non_exhaustive]`; the former has six named variants and the latter has six named variants; `KeyHandle` declares its id, asset class, state, and four timestamp fields. | Editing an enum attribute/variant or a `KeyHandle` field in `src/types.rs` falsifies this declaration inventory. |

`AssetClass::overlap_seconds`, `hard_upper_bound_seconds`, and `as_str`, plus
`KeyState::as_str`, are source mappings in `src/types.rs`; they are not proof
that a schedule, store, or caller used any mapping.

<a id="r04"></a>
## R04 — State-predicate invariant

| Atomic predicate | Static falsifier / evidence |
|---|---|
| `is_valid_write_state` matches only `KeyState::Active`; `is_valid_read_state` matches `KeyState::Active | KeyState::Overlap`. | Changing either `matches!` expression in `src/adapter.rs` falsifies the predicate. |

<a id="r05"></a>
## R05 — Trait-transition invariant

| Atomic predicate | Static falsifier / evidence |
|---|---|
| `RotationAdapter` declares `asset_class`, `generate`, `promote`, `rekey_downstream`, `retire`, `destroy`, `rollback`, and `downstream_error_rate`; its default `validate_transition` has six explicit accepted `(KeyState, KeyState)` pairs. | Removing/renaming a signature or editing the `matches!` pairs in `src/adapter.rs` falsifies this trait-text claim. |

Trait signatures and the default helper describe a source contract only. They
do not establish that any key lifecycle transition was requested or completed.

<a id="r06"></a>
## R06 — Adapter-and-error invariant

| Atomic predicate | Static falsifier / evidence |
|---|---|
| Six public structs — `TdkRotationAdapter`, `PatSigningRotationAdapter`, `AuditChainRotationAdapter`, `AdminSigningRotationAdapter`, `ByokRotationAdapter`, and `ErasureAttestationRotationAdapter` — have local `impl RotationAdapter` blocks. | Removing a public struct or its local trait implementation from the corresponding module falsifies the inventory. |
| `RotationError` is `#[non_exhaustive]` and declares seven named variants. | Editing the enum attribute or variant list in `src/error.rs` falsifies the error-taxonomy declaration. |

<a id="r07"></a>
## R07 — Five static axioms

| ID | Atomic source axiom | Falsifier |
|---|---|---|
| AX-001 | The manifest has the identity and declarations stated in R01. | The named `Cargo.toml` entry changes. |
| AX-002 | The root route and schema literal are stated in R02. | A named root declaration/re-export or literal changes. |
| AX-003 | The type inventory and source mapping methods are stated in R03. | A named enum/field/method declaration changes. |
| AX-004 | The read/write predicates and trait transition pairs are stated in R04–R05. | A matching expression or pair list changes. |
| AX-005 | The six local adapter implementation blocks and seven error variants are stated in R06. | A named impl block or error variant changes. |

<a id="r08"></a>
## R08 — Evidence and five explicit unknowns

Evidence is limited to the manifest, ten local source modules, and declared
test source/target text at the pinned commit. The verified OKF route is a
no-match for `crates/corelink-rotation-adapters/Cargo.toml`; no canonical
concept is asserted.

1. No key creation, use, destruction, or rotation outcome is observed.
2. No provider, storage, audit sink, scheduler, target, or external binding is resolved.
3. No Cargo resolution, compilation, test execution, or property result is claimed.
4. No complete reverse-consumer graph, compatibility result, or caller invocation is established.
5. No deployment, runtime state, operational outcome, or independent review is established.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Back to identity](#r01)
