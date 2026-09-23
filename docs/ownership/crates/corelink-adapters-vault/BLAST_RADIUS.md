---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-adapters-vault
manifest: crates/corelink-adapters-vault/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-adapters-vault-structural-normalization-20260921
---

# corelink-adapters-vault — blast radius

Static relation map for a re-export facade. Arrows classify dependency,
namespace, feature-declaration, or workspace relations only; none establishes
a target selection, provider execution, Vault/mTLS operation, key handling,
FIPS property, migration outcome, or deployment.

[Dependencies](#b01) · [Facade flow](#b02) · [Provider boundary](#b03) ·
[Workspace relation](#b04) · [Review population](#b05) · [Closure](#b06).

<a id="b01"></a>
## B01 — Declared dependency relation

`corelink-adapters-vault` → `corelink-byok` is a manifest dependency. Its
requested feature set includes `vault`. Impact: changing the dependency name,
feature declaration, or exported provider namespace can break this facade’s
source-level path. Unknown: resolved versions/features, target, build, and
invocation.

<a id="b02"></a>
## B02 — Provider-to-facade namespace flow

| Atomic source relation | Direction | Impact of changing the right-hand side |
|---|---|---|
| `corelink_byok::vault::*` → `corelink_adapters_vault::vault::*` | Provider namespace export → facade namespace export | Canonical import availability can change; provider definitions remain owned by `corelink-byok` |
| `corelink-adapters-vault/src/lib.rs` → `corelink_adapters_vault::vault` | Local module declaration → public namespace | Renaming/removing the declaration changes the canonical path; it does not alter provider implementation |

Evidence: `src/lib.rs`, `src/vault.rs`. The relations are re-export wiring, not
execution edges or physical source moves.

<a id="b03"></a>
## B03 — Feature and provider boundary

| Atomic source relation | Direction | Impact / explicit limit |
|---|---|---|
| adapter `features = ["vault"]` → BYOK public `vault` feature | Consumer declaration → provider feature declaration | A declaration change can affect availability of the namespace; no resolved-feature or selected-provider claim follows |
| BYOK public `vault` → `_internal-vault` + `real-vault` | Provider feature declaration → provider cfg inputs | The provider’s source-path eligibility can change; compilation and target selection are unknown |
| BYOK `_internal-vault` cfg → `corelink_byok::vault` re-export | Provider cfg → provider namespace | The public provider namespace is source-gated by BYOK; the facade does not own this gate |
| `corelink-byok/src/byok_vault.rs` → provider `real`/`auth` cfg modules | Provider implementation → target-gated provider modules | Provider runtime, TLS, certificates, key behavior, and any FIPS assertion remain outside the facade and unproved |

Evidence: `crates/corelink-adapters-vault/Cargo.toml`,
`crates/corelink-byok/Cargo.toml`, and `corelink-byok/src/{lib.rs,byok_vault.rs}`.

<a id="b04"></a>
## B04 — Workspace and consumer boundary

| Static relation observed | Classification | Impact / limit |
|---|---|---|
| workspace `Cargo.toml` → `crates/corelink-adapters-vault` | Workspace-member declaration | The package belongs to this workspace source inventory; this is not a build or publication result |
| workspace dependency alias `corelink-adapters-vault` → package path | Workspace dependency declaration | The alias exists in the workspace declaration; it does not identify a consuming crate or prove resolution |
| Targeted manifest search found no other package manifest declaring `corelink-adapters-vault` | Bounded negative static search | No direct manifest consumer was found in that search scope; absence does not prove no source, generated, external, or runtime consumer |

<a id="b05"></a>
## B05 — Review population and migration adjacency

The required static review population is the adapter facade, its BYOK provider
umbrella and Vault source, the workspace root, and any caller that imports the
canonical adapter path or declares it directly. Historical Stage-1/Stage-2 and
absorption wording in local metadata is an adjacent migration concern, not a
consumer relationship or proof of completion. Any requested provider, target,
or migration conclusion must be independently evidenced and reviewed by its
owner.

<a id="b06"></a>
## B06 — Coverage and closure

Coverage is a bounded SOURCE census: local manifest/two-file facade,
provider manifest/crate root/Vault provider source, workspace member and alias
declarations, and a targeted package-manifest search. Unknowns are the full
reverse graph, feature resolution, targets, all consumer imports, actual BYOK
provider behavior, Vault/mTLS activity, keys/secrets, FIPS, migration state,
runtime, deployment, and every test/review result. No source relation closes
those gaps.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
