---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-byok
manifest: crates/corelink-byok/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: H
state: author_validated
evidence_set: corelink-byok-structural-normalization-20260921
---

# corelink-byok — blast radius

Static map of local source, public-path, feature, target-declaration, and
consumer-reference relations. Arrows are not runtime edges: none prove a
provider selection, key use, HTTP activity, cryptographic operation, target
execution, durable effect, or deployment.

[Local map](#b01) · [Public routes](#b02) · [Features and targets](#b03) ·
[Exclusion gates](#b04) · [Consumers](#b05) · [Closure](#b06)

<a id="b01"></a>
## B01 — Local source-family relations

| Source relation | Direction | Change impact |
|---|---|---|
| `byok_core` → crate-root glob route | local core → `corelink_byok::*` | Root-path availability needs source-level compatibility assessment |
| `byok_revocation` → `corelink_byok::revocation::*` | local revocation → public nested route | Revocation-path availability needs assessment; no operation is established |
| `byok_<provider>` → matching public namespace | local provider family → feature-gated public route | Namespace availability depends on the named gate; no provider is selected |

Evidence: `src/lib.rs`, `src/byok_core.rs`, `src/byok_revocation.rs`, and the
four provider roots.

<a id="b02"></a>
## B02 — Public-path routing relation

`src/lib.rs` is the source-level public routing table. Changes to the root
glob export, revocation module, provider-module visibility, or the associated
internal gate can alter import-path availability. The relation is limited to
source shape: it does not show a caller compiled, migrated, selected a
provider, or invoked a KMS operation.

<a id="b03"></a>
## B03 — Feature and target-declaration relations

| Manifest/source relation | Direction | Static impact / limit |
|---|---|---|
| `aws` → `_internal-aws`, `real-aws` | public feature → local module/flavor declaration | A mapping change can alter declared source gates; resolution is unknown |
| `gcp` → `_internal-gcp`, `production-gcp` | public feature → local module/flavor declaration | Same source-level effect; no provider or HTTP use is shown |
| `azure` → `_internal-azure`, `production-azure` | public feature → local module/flavor declaration | Same source-level effect; no provider or HTTP use is shown |
| `vault` → `_internal-vault`, `real-vault` | public feature → local module/flavor declaration | Same source-level effect; no provider or HTTP use is shown |
| `_matrix-test` → four `_internal-*` names | internal aggregate feature → local module declarations | This is a manifest relation, not a selected test configuration |
| target dependency tables → native/wasm dependency declarations | manifest target condition → declared dependency set | No target, link, or execution result is known |

<a id="b04"></a>
## B04 — Public-feature exclusion relation

The six source predicates are `aws+gcp`, `aws+azure`, `aws+vault`,
`gcp+azure`, `gcp+vault`, and `azure+vault`, each attached to a
`compile_error!` in `src/lib.rs`. A changed predicate affects the static
public-feature contract and requires checking all six pair relations. This is
not evidence that any predicate compiled, failed, or selected a provider.

<a id="b05"></a>
## B05 — Bounded static consumer references

| Consumer reference | Classification | Impact boundary |
|---|---|---|
| `corelink-container/Cargo.toml` → `corelink-byok` | Direct manifest dependency | A public path or feature mapping change requires container source assessment; no configured provider is asserted |
| `corelink-container/src/{byok,byok_orchestrator,byok_revocation_runtime}.rs` → root/provider/revocation names | Source-visible imports/references | These are static paths only; no boot, route, scheduler, KMS, or HTTP execution is asserted |
| `corelink-ops/Cargo.toml` and `src/alerts/alerter.rs` → root/revocation names | Direct manifest dependency plus source-visible imports | Alert-source references do not prove delivery or revocation operation |
| `corelink-adapters-vault/Cargo.toml` and `src/vault.rs` → `corelink-byok` / `corelink_byok::vault::*` | Manifest and forwarding-source reference | The forwarding relation does not establish Vault selection or transport activity |

The verified canonical OKF reference is a documentation route, not a consumer
or runtime edge: [BYOK envelope encryption at rest](../../../knowledge/storage/byok-envelope-encryption.md).
It is intentionally not revalidated or duplicated here.

The static harness boundary `tests/e2e-tenant-isolation` →
`corelink-byok::EnvelopeEncryptor` shares identity
`repo:1232040291:boundary:e2e-tenant-isolation-byok-001` with
`e2e-tenant-isolation` BLAST `REL-003`. The test supplies tenant/blob context to
the envelope API and checks local encrypt/decrypt outcomes; `corelink-byok` owns
the API/algorithm, while the harness owns its `StubKms` and assertions. No
provider selection, production composition, execution, or runtime reachability
is established by this relation.

<a id="b06"></a>
## B06 — Coverage and closure

Coverage is a bounded source census: manifest feature and target tables,
`src/lib.rs`, local core/revocation/provider families, and the listed direct
consumer references. Unknown: complete reverse graph, resolved versions and
features, target selection, consumer adoption, API compatibility, provider
selection, keys/credentials, HTTP/KMS/crypto behavior, scheduling, durable
effects, test results, runtime reachability, deployment, and independent
review. Claims beyond these static relations require new evidence.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Back to local map](#b01)
