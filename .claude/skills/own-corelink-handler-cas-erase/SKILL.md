---
name: own-corelink-handler-cas-erase
description: Ownership routing for corelink-handler-cas-erase; static draft only and never production authorization.
metadata:
  schema: corelink-ownership/1.1
  package: corelink-handler-cas-erase
  manifest: crates/corelink-handler-cas-erase/Cargo.toml
  source-commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
  evidence-set: corelink-handler-cas-erase-structural-normalization-20260921
---

# Ownership — corelink-handler-cas-erase

[S01](#s01) · [S02](#s02) · [S03](#s03) · [S04](#s04) · [S05](#s05) · [S06](#s06) · [S07](#s07)

This guide is limited to `crates/corelink-handler-cas-erase` at the recorded
source baseline. It records SOURCE/static-manifest evidence, never a mounted
endpoint, R2 delete, D1 write, authorization operation, or runtime result.

<a id="s01"></a>
## S01 — Enter the boundary

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A change names this package or its public symbols | Read `Cargo.toml`, `src/lib.rs`, `src/handler.rs`, and `src/error.rs` before deciding scope | [R01](../../../docs/ownership/crates/corelink-handler-cas-erase/REFERENCE.md#r01) | Package identity or baseline differs |
| A request requires R2, D1, a route, auth key, or HTTP status delivery | Classify it as container composition, not this pure crate | [B03](../../../docs/ownership/crates/corelink-handler-cas-erase/BLAST_RADIUS.md#b03) | Do not infer an operation from comments or a function call boundary |

<a id="s02"></a>
## S02 — Preserve request validation

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| `CasEraseRequest` or `prepare_erase` changes | Preserve cross-tenant denial before digest validation, then construct the marker from the authenticated tenant and digest | [API-002](../../../docs/ownership/crates/corelink-handler-cas-erase/REFERENCE.md#api-002), [INV-001](../../../docs/ownership/crates/corelink-handler-cas-erase/REFERENCE.md#inv-001) | The identity/authentication owner or route contract is required |
| Digest rules change | Preserve non-empty, at-most-128-byte, ASCII alphanumeric/`-`/`_` admission unless consumers agree on a new key grammar | [API-001](../../../docs/ownership/crates/corelink-handler-cas-erase/REFERENCE.md#api-001) | Storage-key or compatibility evidence is absent |

<a id="s03"></a>
## S03 — Preserve tombstone semantics

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Read decision changes | Keep `true → Gone` and `false → Proceed`; this function receives an already-resolved boolean and performs no lookup | [API-003](../../../docs/ownership/crates/corelink-handler-cas-erase/REFERENCE.md#api-003) | A D1 query, HTTP route, or observed response is requested |
| Erase outcome changes | Keep a prior tombstone as `AlreadyErased`; map only a non-prior tombstone to `Erased` | [API-004](../../../docs/ownership/crates/corelink-handler-cas-erase/REFERENCE.md#api-004) | Delete/upsert ordering or durable-state authority is needed |

<a id="s04"></a>
## S04 — Keep local data boundaries explicit

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Request or marker fields change | Treat all fields as caller-owned strings; `new` constructors do not validate or persist them | [API-005](../../../docs/ownership/crates/corelink-handler-cas-erase/REFERENCE.md#api-005) | A claim requires PII policy, reason bounds, or durable retention |
| Error taxonomy changes | Preserve `#[non_exhaustive]` handling and do not turn `Transport` into evidence of a transport implementation | [API-006](../../../docs/ownership/crates/corelink-handler-cas-erase/REFERENCE.md#api-006) | A transport-specific recovery behavior is required |

<a id="s05"></a>
## S05 — Coordinate the composition seam

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| A public function/error changes | Trace the static consumer `corelink-container` and its `routes/cas_erase` imports separately from this crate | [REL-ERASE-002](../../../docs/ownership/crates/corelink-handler-cas-erase/BLAST_RADIUS.md#rel-erase-002) | Reverse consumer census or container owner decision is incomplete |
| R2 deletion or D1 tombstone persistence changes | Route the decision to the container owner; retain this crate as the pure decision kernel only | [REL-ERASE-003](../../../docs/ownership/crates/corelink-handler-cas-erase/BLAST_RADIUS.md#rel-erase-003) | Do not assign adapter/runtime ownership to this package |

<a id="s06"></a>
## S06 — Validate at the correct level

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Static ownership artifacts change | Run the four structural checks and `git diff --check`; inspect the four allowed paths | [M04](../../../docs/ownership/crates/corelink-handler-cas-erase/MAINTENANCE.md#m04) | Do not present documentation checks as Cargo, tests, runtime, or cold review |
| Source changes are separately authorized | Have the source owner choose bounded local validation; keep R2, D1, credentials, and production data out of scope | `tests/prop_handler_cas_erase.rs` | Scope requires external operation |

<a id="s07"></a>
## S07 — Report and escalate

| Condition | Action | Evidence | Stop |
|---|---|---|---|
| Static work is ready | Report baseline, source paths, changed paths, static relations, checks, and unknowns | [M06](../../../docs/ownership/crates/corelink-handler-cas-erase/MAINTENANCE.md#m06) | Do not self-certify cold review |
| A production conclusion is requested | Ask the container, R2/D1, auth, or operations owner for direct evidence | [R08](../../../docs/ownership/crates/corelink-handler-cas-erase/REFERENCE.md#r08) | Do not translate source comments into observed behavior |

[Reference](../../../docs/ownership/crates/corelink-handler-cas-erase/REFERENCE.md#r01) ·
[Blast radius](../../../docs/ownership/crates/corelink-handler-cas-erase/BLAST_RADIUS.md#b01) ·
[Maintenance](../../../docs/ownership/crates/corelink-handler-cas-erase/MAINTENANCE.md#m01)
