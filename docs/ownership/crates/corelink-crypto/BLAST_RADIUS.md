---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-crypto
manifest: crates/corelink-crypto/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-crypto-structural-normalization-20260921
---

# corelink-crypto — blast radius

Static dependency and source-relation map. Arrows below classify source-visible
relations only; none establish runtime use, execution order, deployment, or
foreign-ABI behavior.

[Dependencies](#b01) · [Provider flows](#b02) · [Local facades](#b03) ·
[Direct consumers](#b04) · [Review population](#b05) · [Closure](#b06).

<a id="b01"></a>
## B01 — Declared dependency relation

`corelink-crypto` → `{corelink-hash, corelink-erasure-attestation,
corelink-client-verify}` and `corelink-crypto` → `{subtle, hmac, sha2}` are
manifest dependency declarations. Impact: a changed provider/exported upstream
name can break this facade’s source contract. Unknown: resolution, selected
features, target compatibility, and invocation.

<a id="b02"></a>
## B02 — Provider-to-facade export relations

| Atomic source relation | Direction | Impact of changing the right-hand side |
|---|---|---|
| `corelink_hash::*` → `corelink_crypto::blake3::*` | provider export → facade export | Canonical hash-path availability can change; hash implementation ownership remains with `corelink-hash` |
| `corelink_erasure_attestation::*` → `corelink_crypto::ed25519::attestation::*` | provider export → nested facade export | Attestation-path availability can change; signing/persistence semantics are not established here |
| `corelink_client_verify::*` → `corelink_crypto::client_verify::*` | provider export → facade export | Client-verification path availability can change; SDK/FFI artifact behavior remains provider-owned |

Evidence: `src/{blake3,ed25519,client_verify}.rs`. A re-export is not an
execution edge and is not a physical source move.

<a id="b03"></a>
## B03 — Upstream-to-local-facade relations

`subtle::{Choice, ConstantTimeEq}` → `corelink_crypto::ct_eq::*`; changing the
trait/name can affect callers adopting the canonical path. `hmac::{Hmac,
KeyInit, Mac}` plus `sha2::Sha256` → `corelink_crypto::hmac::*` and
`HmacSha256`; changing either generic type can affect alias users. These are
source-level import relations, not a claim that all sensitive comparisons use
the trait or all workspace HMACs use this alias.

<a id="b04"></a>
## B04 — Direct static consumer relations

| Consumer / source relation | Classification | Impact |
|---|---|---|
| `corelink-ac/Cargo.toml` → `corelink-crypto` | Direct manifest dependency | A facade API change requires AC source assessment; `corelink-hash` remains separately declared there |
| `corelink-cas/Cargo.toml` → `corelink-crypto` | Direct manifest dependency | A facade API change requires CAS source assessment; this does not show which module imports it |
| `corelink-privacy/src/{erasure,lib}.rs` → `corelink_crypto::ed25519::attestation` | Source-visible canonical-path reference | Erasure-attestation path documentation/migration intent may need reconciliation; no direct dependency or execution is asserted |

<a id="b05"></a>
## B05 — Static review population and adjacent paths

The static review population includes DSR, replication, DPA acceptance, AC,
CAS, clerk, BYOK, privacy, worker, container, and SDK-related paths. Their
relations are intentionally not collapsed into one consumer claim:

| Group | Static relation observed | Unknown / impact boundary |
|---|---|---|
| DSR and replication | No direct facade/provider match was found in the targeted static source search | They remain adjacent migration paths; do not represent them as direct consumers without new evidence |
| DPA acceptance | Its manifest declares `subtle`; `src/service.rs` references `ConstantTimeEq` | A future `ct_eq` migration needs source-level assessment, not assumed adoption |
| AC and CAS | Each manifest declares `corelink-crypto`; AC/CAS also have legacy/direct hash context | Dependency declaration does not identify symbols, runtime path, or migration completion |
| Clerk and BYOK | Source uses/references `subtle::ConstantTimeEq`; clerk has HMAC/SHA-256 dependencies | They are potential facade-migration consumers, not direct `corelink-crypto` import evidence |
| Privacy | Source documentation names the Ed25519 canonical path; local privacy code uses HMAC/ct-eq primitives | No direct facade dependency or erasure-worker execution is evidenced |
| Worker and container | Static source/manifests directly use hash, attestation, HMAC, and/or `subtle` provider paths | Provider use does not prove adoption of this aggregator; changes require compatibility assessment |
| SDK-related | `corelink-client-verify` source/manifest declares Rust, optional stream, and feature-gated FFI surfaces | The facade re-export does not prove cbindgen, Go, Python, JS/WASM, header, or ABI behavior |

<a id="b06"></a>
## B06 — Coverage and closure

Coverage is a targeted static census: the facade manifest and six source files,
its three provider manifests/crate roots, direct AC/CAS declarations, privacy
canonical-path references, and selected adjacent consumers. Unknowns are the
complete reverse graph, features/targets, generated or external SDK consumers,
runtime routes, provider behavior, deployment, and all test/review outcomes.
Any claim beyond those source relations needs new evidence.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
