---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-adapters-cloud
manifest: crates/corelink-adapters-cloud/Cargo.toml
source_commit: 1c99a5b8ce1db7b622db7811512432a5d070bbbe
profile: S
state: author_validated
evidence_set: w005-cloud-source-static-20260920
---

# corelink-adapters-cloud — blast radius

This is a SOURCE-only map of the static aggregator facade. “Dependency” states a manifest edge; “flow” states a local re-export path; “impact” states what must be reconsidered if that source mapping changes. No relation proves selected target availability, a build, reverse consumption, migration, provider connectivity, or runtime operation.

[Scope](#b01) · [Dependencies](#b02) · [Path flows](#b03) · [Ownership](#b04) · [Compatibility](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Scope and reading rules

The owned source surface is `src/{lib,cf,clerk,stripe,statuspage,slack}.rs` plus the package manifest. Its production facade mapping consists of five public module declarations and five direct whole-surface re-exports. `src/lib.rs` also contains a `#[cfg(test)]` module with five local path-resolution test functions. Read every relation in three directions: dependency identifies a declared package edge; flow identifies a source path mapping; impact is a review obligation. Neither a glob re-export nor package-description prose proves that an implementation is usable by a given consumer or selected target.

<a id="b02"></a>
## B02 — Declared dependency relations

[REL-001](#rel-001) · [REL-002](#rel-002) · [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005).

<a id="rel-001"></a>
### REL-001 — CF facade relation

**Dependency:** the manifest declares `corelink-cf-bindings`. **Flow:** `corelink_adapters_cloud::cf` re-exports `corelink_cf_bindings::*`. **Impact:** changing the module name or dependency target can break imports using the facade path; changing implementation behavior belongs to the upstream package. **Evidence:** manifest and `src/cf.rs`. **Limit:** no target selection, build, Cloudflare operation, or reverse consumer is proven. [Index](#b02) [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Clerk facade relation

**Dependency:** the manifest declares `corelink-clerk-cf`. **Flow:** `corelink_adapters_cloud::clerk` re-exports `corelink_clerk_cf::*`. **Impact:** changing the module name or dependency target can break imports using the facade path; the upstream package owns its implementation contract. **Evidence:** manifest and `src/clerk.rs`. **Limit:** no identity validation, target selection, build, or provider/runtime behavior is proven. [Index](#b02) [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Stripe facade relation

**Dependency:** the manifest declares `corelink-stripe-real`. **Flow:** `corelink_adapters_cloud::stripe` re-exports `corelink_stripe_real::*`. **Impact:** changing the module name or dependency target can break imports using the facade path; the upstream package owns implementation behavior. **Evidence:** manifest and `src/stripe.rs`. **Limit:** no HTTPS request, provider state, target selection, build, or runtime outcome is proven. [Index](#b02) [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Statuspage facade relation

**Dependency:** the manifest declares `corelink-statuspage-real`. **Flow:** `corelink_adapters_cloud::statuspage` re-exports `corelink_statuspage_real::*`. **Impact:** changing the module name or dependency target can break imports using the facade path; the upstream package owns implementation behavior. **Evidence:** manifest and `src/statuspage.rs`. **Limit:** no HTTPS request, provider publication, target selection, build, or runtime outcome is proven. [Index](#b02) [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Slack facade relation

**Dependency:** the manifest declares `corelink-slack-real`. **Flow:** `corelink_adapters_cloud::slack` re-exports `corelink_slack_real::*`. **Impact:** changing the module name or dependency target can break imports using the facade path; the upstream package owns implementation behavior. **Evidence:** manifest and `src/slack.rs`. **Limit:** no HTTPS request, provider delivery, target selection, build, or runtime outcome is proven. [Index](#b02) [Relation index](#b03)

<a id="b03"></a>
## B03 — Public-path flow relations

[REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008). [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Root-to-module flow

**Dependency:** `src/lib.rs` publicly declares all five facade modules. **Flow:** a caller may name one of the canonical path segments listed in API-001, then access whatever the matching upstream package makes public in its own compilation context. **Impact:** a root-module removal or visibility change risks source breakage across all imports of that segment. **Evidence:** `src/lib.rs`. **Limit:** no complete caller graph or target-specific public-symbol inventory was resolved. [Index](#b03)

<a id="rel-007"></a>
### REL-007 — Facade-to-upstream type flow

**Dependency:** each module uses a whole-surface re-export. **Flow:** public upstream symbols can be reached through the named facade path without locally redefining their types. **Impact:** an upstream public rename/removal, or a swapped target package, can change facade import compatibility. **Evidence:** five local facade modules. **Limit:** this does not transfer implementation ownership, make a semantic promise beyond the upstream public surface, or certify target availability. [Index](#b03)

<a id="rel-008"></a>
### REL-008 — Test-name observation

**Dependency:** the root’s test-only module imports each local facade module under an unused alias. **Flow:** the source names all five paths as intended resolution checks. **Impact:** renaming a module requires updating that local source test and reviewing public-path compatibility. **Evidence:** `src/lib.rs`. **Limit:** test declaration is not an executed check and does not establish a successful build. [Index](#b03)

<a id="b04"></a>
## B04 — Ownership and selection boundaries

[REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011).

<a id="rel-009"></a>
### REL-009 — Facade ownership boundary

**Dependency:** local module declarations and `pub use` statements are in this package. **Flow:** this package provides a stable-looking source import grouping only. **Impact:** changes to the five names or their mapping belong to this facade’s review scope. **Evidence:** `src/lib.rs` and R03. **Limit:** ownership of the grouped source path is not ownership of any re-exported implementation. [Index](#b04) [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Implementation-owner boundary

**Dependency:** the five direct dependencies supply all re-exported symbols. **Flow:** implementation details, feature conditions, and public contracts originate in the respective upstream package. **Impact:** behavioral changes must be reviewed with the relevant implementation owner; this facade review covers only the mapping/import path. **Evidence:** manifest and five `pub use` statements. **Limit:** no team assignment or complete upstream API audit is inferred. [Index](#b04) [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Target-selection boundary

**Dependency:** no scoped local module selects a target or feature combination. **Flow:** selected target and feature resolution enter from the build/workspace and consumer context, outside the facade. **Impact:** a request about conditional export availability must be routed to that selection context and upstream package. **Evidence:** scoped manifest and source absence. **Limit:** absence here does not identify what target a consumer selected or whether it builds. [Index](#b04) [Relation index](#b03)

<a id="b05"></a>
## B05 — Compatibility and operational impact

[REL-012](#rel-012) · [REL-013](#rel-013) · [REL-014](#rel-014). [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Canonical path compatibility

**Dependency:** API-001 defines five public module names. **Flow:** consumers that elect to use the facade may form imports under those names. **Impact:** rename, removal, visibility reduction, or target-package swap is compatibility-sensitive until direct consumers are resolved. **Evidence:** `src/lib.rs`, R04. **Limit:** usage is not established by public availability. [Index](#b05) [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — Whole-surface expansion or contraction

**Dependency:** every facade module uses `::*`. **Flow:** an upstream public-surface change can become visible through the same facade module under an independently chosen context. **Impact:** facade documentation and compatibility review should re-check the direct mapping and affected upstream public contract. **Evidence:** five local modules. **Limit:** no exhaustive export set, semantic compatibility decision, or feature/target resolution is provided here. [Index](#b05) [Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — No local operational flow

**Dependency:** the scoped local modules contain module declarations and re-exports only. **Flow:** no local provider request, credential parse, target choice, persistent mutation, or migration sequence is observed. **Impact:** operational questions must not be answered from this facade artifact. **Evidence:** R03 and API-002. **Limit:** this says nothing about behavior in upstream packages or an external system. [Index](#b05) [Relation index](#b03)

<a id="b06"></a>
## B06 — Coverage, unknowns, and done gate

Coverage includes the manifest’s five direct package dependencies and every scoped local source module. The map deliberately does not infer reverse consumers from broad repository references. Unknown: selected feature/target combinations, upstream symbol inventories, builds, direct and indirect consumers, migration state, provider configuration and operation, credentials, network behavior, deployment, and runtime reachability.

Success is a direction-labeled facade map that separates dependency, source flow, impact, implementation owner, and target-selection boundary. Completeness means all five mappings and the root path relation are represented. Quality means a source re-export never becomes a provider, target, build, migration, or runtime claim. Definition of done is the companion [reference](REFERENCE.md#r08), [maintenance](MAINTENANCE.md#m06), static document validation, scope-only diff, and independent review.

[Back to start](#b01)
