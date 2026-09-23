---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-rotation-adapters
manifest: crates/corelink-rotation-adapters/Cargo.toml
source_commit: 6be030999de1f0e0fe62d3a9abb04ec2a4fefde6
profile: S
state: author_validated
evidence_set: w009-rotation-adapters-source-static-20260920
---

# corelink-rotation-adapters — blast radius

Each relation below is one static source arrow. It can identify an import,
declaration, or source-compatibility review obligation; it cannot prove a key,
provider, rotation, test, build, or runtime operation. The verified OKF route
for the manifest is no-match.

[Manifest](#b01) · [Root](#b02) · [Types](#b03) · [Predicates](#b04) · [Adapters](#b05) · [Closure](#b06)

<a id="b01"></a>
## B01 — Manifest-to-source relation

`Cargo.toml` → `src/error.rs` is a declared `thiserror` dependency/import
relation; `Cargo.toml` → `src/types.rs` is a declared `serde` dependency/import
relation. Changing either declaration requires reviewing the named local source
import; it does not prove dependency resolution or compilation.

<a id="b02"></a>
## B02 — Module-to-root relation

| Arrow | Source | Evidence | Source activation | Source failure |
|---|---|---|---|---|
| REL-001: `src/adapter.rs` → `src/lib.rs` | Local adapter module → crate root | `pub mod adapter;` | The declaration exposes the source path `corelink_rotation_adapters::adapter`; it does not establish a consumer or compilation. | Removing, renaming, or privatizing this declaration changes that named source path. |
| REL-002: `src/admin_signing.rs` → `src/lib.rs` | Local admin-signing module → crate root | `pub mod admin_signing;` | The declaration exposes the source path `corelink_rotation_adapters::admin_signing`; it does not establish construction or invocation. | Removing, renaming, or privatizing this declaration changes that named source path. |
| REL-003: `src/audit_chain.rs` → `src/lib.rs` | Local audit-chain module → crate root | `pub mod audit_chain;` | The declaration exposes the source path `corelink_rotation_adapters::audit_chain`; it does not establish an audit operation. | Removing, renaming, or privatizing this declaration changes that named source path. |
| REL-004: `src/byok.rs` → `src/lib.rs` | Local BYOK module → crate root | `pub mod byok;` | The declaration exposes the source path `corelink_rotation_adapters::byok`; it does not establish key or provider activity. | Removing, renaming, or privatizing this declaration changes that named source path. |
| REL-005: `src/erasure_attestation.rs` → `src/lib.rs` | Local erasure-attestation module → crate root | `pub mod erasure_attestation;` | The declaration exposes the source path `corelink_rotation_adapters::erasure_attestation`; it does not establish an attestation operation. | Removing, renaming, or privatizing this declaration changes that named source path. |
| REL-006: `src/error.rs` → `src/lib.rs` | Local error module → crate root | `pub mod error;` | The declaration exposes the source path `corelink_rotation_adapters::error`; it does not establish an error occurrence. | Removing, renaming, or privatizing this declaration changes that named source path. |
| REL-007: `src/pat_signing.rs` → `src/lib.rs` | Local PAT-signing module → crate root | `pub mod pat_signing;` | The declaration exposes the source path `corelink_rotation_adapters::pat_signing`; it does not establish signing activity. | Removing, renaming, or privatizing this declaration changes that named source path. |
| REL-008: `src/tdk.rs` → `src/lib.rs` | Local TDK module → crate root | `pub mod tdk;` | The declaration exposes the source path `corelink_rotation_adapters::tdk`; it does not establish a key operation. | Removing, renaming, or privatizing this declaration changes that named source path. |
| REL-009: `src/types.rs` → `src/lib.rs` | Local types module → crate root | `pub mod types;` | The declaration exposes the source path `corelink_rotation_adapters::types`; it does not establish use of a value. | Removing, renaming, or privatizing this declaration changes that named source path. |

<a id="b03"></a>
## B03 — Types-to-contract relation

This section records seven individually addressable type-import arrows:
`REL-010` for the predicate/trait module and `REL-011` through `REL-016` for
the six adapter modules. No row is a runtime activation claim.

| Arrow | Source | Evidence | Source activation | Source failure |
|---|---|---|---|---|
| REL-010: `src/types.rs` → `src/adapter.rs` | Shared types → predicate/trait module | `use crate::types::{KeyHandle, KeyState};` | The import names the shared types in the predicate/trait text; it does not establish use of a state value. | Changing/removing the import or a referenced type name changes this local source contract. |
| REL-011: `src/types.rs` → `src/tdk.rs` | Shared types → TDK adapter module | `use crate::types::{AssetClass, KeyHandle, KeyState};` | The import names the shared types in the local adapter text; it does not establish adapter construction or a key operation. | Changing/removing the import or a referenced type name changes this local source contract. |
| REL-012: `src/types.rs` → `src/pat_signing.rs` | Shared types → PAT-signing adapter module | `use crate::types::{AssetClass, KeyHandle, KeyState};` | The import names the shared types in the local adapter text; it does not establish signing activity. | Changing/removing the import or a referenced type name changes this local source contract. |
| REL-013: `src/types.rs` → `src/audit_chain.rs` | Shared types → audit-chain adapter module | `use crate::types::{AssetClass, KeyHandle, KeyState};` | The import names the shared types in the local adapter text; it does not establish an audit operation. | Changing/removing the import or a referenced type name changes this local source contract. |
| REL-014: `src/types.rs` → `src/admin_signing.rs` | Shared types → admin-signing adapter module | `use crate::types::{AssetClass, KeyHandle, KeyState};` | The import names the shared types in the local adapter text; it does not establish signing activity. | Changing/removing the import or a referenced type name changes this local source contract. |
| REL-015: `src/types.rs` → `src/byok.rs` | Shared types → BYOK adapter module | `use crate::types::{AssetClass, KeyHandle, KeyState};` | The import names the shared types in the local adapter text; it does not establish a provider or key operation. | Changing/removing the import or a referenced type name changes this local source contract. |
| REL-016: `src/types.rs` → `src/erasure_attestation.rs` | Shared types → erasure-attestation adapter module | `use crate::types::{AssetClass, KeyHandle, KeyState};` | The import names the shared types in the local adapter text; it does not establish an attestation operation. | Changing/removing the import or a referenced type name changes this local source contract. |

<a id="b04"></a>
## B04 — Predicate-to-trait relation

`src/adapter.rs` predicate functions → `src/lib.rs` re-export is a local public
route. `RotationAdapter::validate_transition` → `RotationError::InvalidTransition`
is a local type-reference relation. A changed predicate, pair list, or error
variant changes source semantics available to callers; it is not evidence that
any transition or error happened.

<a id="b05"></a>
## B05 — Adapter-to-trait relation

| Arrow | Source | Evidence | Source activation | Source failure |
|---|---|---|---|---|
| REL-017: `src/tdk.rs` → `src/adapter.rs` | TDK adapter module → shared trait module | `use crate::adapter::RotationAdapter;` and `impl RotationAdapter for TdkRotationAdapter` | The local implementation binds the named struct to the trait’s source contract; it does not establish construction, invocation, or a key operation. | Removing/changing the import, impl target, or struct name changes this source relation. |
| REL-018: `src/pat_signing.rs` → `src/adapter.rs` | PAT-signing adapter module → shared trait module | `use crate::adapter::RotationAdapter;` and `impl RotationAdapter for PatSigningRotationAdapter` | The local implementation binds the named struct to the trait’s source contract; it does not establish construction, invocation, or signing activity. | Removing/changing the import, impl target, or struct name changes this source relation. |
| REL-019: `src/audit_chain.rs` → `src/adapter.rs` | Audit-chain adapter module → shared trait module | `use crate::adapter::RotationAdapter;` and `impl RotationAdapter for AuditChainRotationAdapter` | The local implementation binds the named struct to the trait’s source contract; it does not establish construction, invocation, or an audit operation. | Removing/changing the import, impl target, or struct name changes this source relation. |
| REL-020: `src/admin_signing.rs` → `src/adapter.rs` | Admin-signing adapter module → shared trait module | `use crate::adapter::RotationAdapter;` and `impl RotationAdapter for AdminSigningRotationAdapter` | The local implementation binds the named struct to the trait’s source contract; it does not establish construction, invocation, or signing activity. | Removing/changing the import, impl target, or struct name changes this source relation. |
| REL-021: `src/byok.rs` → `src/adapter.rs` | BYOK adapter module → shared trait module | `use crate::adapter::RotationAdapter;` and `impl RotationAdapter for ByokRotationAdapter` | The local implementation binds the named struct to the trait’s source contract; it does not establish construction, invocation, provider activity, or a key operation. | Removing/changing the import, impl target, or struct name changes this source relation. |
| REL-022: `src/erasure_attestation.rs` → `src/adapter.rs` | Erasure-attestation adapter module → shared trait module | `use crate::adapter::RotationAdapter;` and `impl RotationAdapter for ErasureAttestationRotationAdapter` | The local implementation binds the named struct to the trait’s source contract; it does not establish construction, invocation, or an attestation operation. | Removing/changing the import, impl target, or struct name changes this source relation. |

<a id="b06"></a>
## B06 — Closure and unknowns

Coverage is the manifest, `src/lib.rs`, nine other local source modules, the
declared test target/source, and B03's seven type-import arrows (`REL-010`
through `REL-016`). The static map ends at those paths. Unknowns are exactly
the five categories in R08: key outcomes; external bindings; resolution/build/
test results; full consumers/compatibility/invocation; and deployment/runtime/
review. The no-match OKF lookup is a routing result, not an architecture claim.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Back to manifest](#b01)
