---
schema: corelink-ownership/1.1
document: reference
package: corelink-crypto
manifest: crates/corelink-crypto/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-crypto-structural-normalization-20260921
---

# corelink-crypto — ownership reference

Static-source reference for a canonical import facade. It does not certify an
algorithm beyond its provider contract, execution, FFI behavior, SDK delivery,
or deployment.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Contracts](#r04) ·
[Invariants](#r05) · [Configuration](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity

Record index: [API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005)

| Field | Static evidence |
|---|---|
| Package / manifest | `corelink-crypto` / `crates/corelink-crypto/Cargo.toml` |
| Role | Canonical Rust import facade for three CoreLink providers and two upstream primitive facades |
| Source inventory | Six files: `lib.rs`, `blake3.rs`, `ed25519.rs`, `client_verify.rs`, `ct_eq.rs`, and `hmac.rs` |
| Declared direct providers | `corelink-hash`, `corelink-erasure-attestation`, `corelink-client-verify`, `subtle`, `hmac`, and `sha2` |
| Source mode | SOURCE evidence only; no resolved graph or execution result was collected |

<a id="r02"></a>
## R02 — Boundary and authority

This crate owns the namespace wiring in its six source files. `blake3.rs` and
`ed25519::attestation` re-export provider APIs; `client_verify.rs` re-exports
the provider that retains its SDK and FFI implementation boundary. `ct_eq.rs`
and `hmac.rs` only expose upstream names and one alias. Provider implementations,
foreign artifacts, cryptographic policy, and consumer behavior remain outside
this crate’s ownership.

In particular, `corelink_crypto::blake3` being a BLAKE3-provider path does not
mean every digest in the workspace has that contract, and a Rust re-export does
not demonstrate execution or FFI loading.

<a id="r03"></a>
## R03 — Implementation map

| Module | Source-visible responsibility | Ownership classification |
|---|---|---|
| `lib.rs` | Forbids unsafe code, declares the five public modules, and contains static smoke-test source | Local module wiring |
| `blake3.rs` | `pub use corelink_hash::*` | Re-export; hash provider owns implementation |
| `ed25519.rs` | `ed25519::attestation` re-exports `corelink_erasure_attestation::*` | Re-export; attestation provider owns implementation |
| `client_verify.rs` | `pub use corelink_client_verify::*` | Re-export; client-verification/SDK provider owns implementation |
| `ct_eq.rs` | Re-exports `subtle::{Choice, ConstantTimeEq}` | Local upstream facade |
| `hmac.rs` | Re-exports HMAC traits/types and defines `HmacSha256` | Local upstream facade |

<a id="r04"></a>
## R04 — Public contracts

<a id="api-001"></a>
### API-001 — BLAKE3 namespace

`corelink_crypto::blake3::*` is a direct re-export of the public
`corelink_hash` surface. Changing the re-export can change source-level path
availability, but it does not transfer ownership of hash algorithm, digest, or
storage contracts. Evidence: `src/blake3.rs`.

<a id="api-002"></a>
[↩](#r01)
### API-002 — Ed25519 attestation namespace

`corelink_crypto::ed25519::attestation::*` is a direct re-export of
`corelink_erasure_attestation::*`. It is the source-visible route for the
attestation provider; it is not evidence that an erasure flow signs, persists,
or verifies an attestation. Evidence: `src/ed25519.rs`.

<a id="api-003"></a>
[↩](#r01)
### API-003 — Client verification namespace

`corelink_crypto::client_verify::*` directly re-exports
`corelink_client_verify::*`. The provider’s source declares optional FFI and
stream features, but this facade neither enables a feature nor exposes a new
foreign ABI itself. Evidence: `src/client_verify.rs`; provider manifest.

<a id="api-004"></a>
[↩](#r01)
### API-004 — Constant-time equality facade

`corelink_crypto::ct_eq::*` exposes only `subtle::Choice` and
`subtle::ConstantTimeEq`. It introduces no comparison function or timing
guarantee beyond the upstream trait and a caller’s selected use. Evidence:
`src/ct_eq.rs`.

<a id="api-005"></a>
[↩](#r01)
### API-005 — HMAC-SHA-256 facade

`corelink_crypto::hmac::*` exposes `Hmac`, `KeyInit`, `Mac`, and `Sha256` and
defines `HmacSha256` exactly as `Hmac<Sha256>`. It does not create keys, select
protocol input, or verify a tag. Evidence: `src/hmac.rs`.
[↩](#r01)

<a id="r05"></a>
## R05 — Falsifiable invariants

| Identifier | Atomic predicate | Static falsifier / evidence |
|---|---|---|
| <a id="inv-001"></a>INV-001 | `blake3.rs`, `ed25519.rs`, and `client_verify.rs` each contain a direct `pub use` of their named CoreLink provider | Removing, renaming, or wrapping one `pub use` falsifies the source relation |
| <a id="inv-002"></a>INV-002 | The crate root declares exactly the five public modules `blake3`, `client_verify`, `ct_eq`, `ed25519`, and `hmac` in this baseline | Adding/removing/privatizing a declaration falsifies the namespace inventory |
| <a id="inv-003"></a>INV-003 | `ct_eq.rs` re-exports `Choice` and `ConstantTimeEq` from `subtle` without a local comparison implementation | A local comparator or changed upstream source falsifies the statement |
| <a id="inv-004"></a>INV-004 | `HmacSha256` is spelled `Hmac<Sha256>` | Any alias target other than those two exported generic names falsifies it |
| <a id="inv-005"></a>INV-005 | `lib.rs` contains `#![forbid(unsafe_code)]` for this crate’s source | Removing or weakening that crate attribute falsifies the statement; it says nothing about a re-exported provider’s feature-gated code |

<a id="r06"></a>
## R06 — Configuration and feature boundary

The facade manifest declares no crate-local features. The client-verification
provider declares `stream` and `ffi` features in its own manifest; no Cargo
resolution or selected feature set was inspected. `lib.rs` contains test source
for path resolution and local facade use, but this document records no test
execution.

<a id="r07"></a>
## R07 — Failure and compatibility boundary

An unresolved provider symbol, changed export path, or changed `HmacSha256`
alias is a source-level compatibility concern. The local crate has no source
for key storage, digest generation, signature generation, network transport,
or foreign-header generation. Do not infer failures, recovery, or status codes
for those systems from this facade.

<a id="r08"></a>
## R08 — Evidence and unknowns

Evidence set: this manifest; all six `src/*.rs` files; the three provider
manifests and crate roots; and static manifests/imports selected for the impact
map. Unknowns: resolved features and reverse graph, all consumer adoption,
provider semantic compatibility, SDK/header generation, FFI loading, runtime
reachability, cryptographic timing behavior, deployment, operational owner,
and cold-review status.

[Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
