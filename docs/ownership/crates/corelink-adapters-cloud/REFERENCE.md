---
schema: corelink-ownership/1.1
document: reference
package: corelink-adapters-cloud
manifest: crates/corelink-adapters-cloud/Cargo.toml
source_commit: 1c99a5b8ce1db7b622db7811512432a5d070bbbe
profile: S
state: author_validated
evidence_set: w005-cloud-source-static-20260920
---

# corelink-adapters-cloud — ownership reference

This reference records SOURCE evidence from the package manifest and its six local source modules at the fixed commit. The package is a static aggregator facade: it declares five canonical public submodule paths and re-exports an upstream package from each. It does not itself implement provider behavior, select a target, or establish build, migration, deployment, or runtime facts.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Public API](#r04) · [Invariants](#r05) · [Configuration](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity and function

| Field | Source-defined value |
|---|---|
| Package / manifest | `corelink-adapters-cloud` / `crates/corelink-adapters-cloud/Cargo.toml` |
| Role | Option-A canonical import facade over five direct package dependencies |
| Source inspected | `lib`, `cf`, `clerk`, `stripe`, `statuspage`, `slack` |
| Direct package dependencies | `corelink-cf-bindings`, `corelink-clerk-cf`, `corelink-stripe-real`, `corelink-statuspage-real`, `corelink-slack-real` |
| Evidence class | SOURCE only |

The root declares five public modules. Each local module is a whole-public-surface re-export of one direct package dependency. The observed `#[cfg(test)]` path-resolution tests are source declarations, not executed results.

<a id="r02"></a>
## R02 — Boundary and ownership

This package owns the public facade paths and the local mapping that names them. The five direct dependency packages own their respective implementation sources and public contracts; this package does not transfer that ownership merely by re-exporting them. The upstream packages are, respectively, `corelink-cf-bindings`, `corelink-clerk-cf`, `corelink-stripe-real`, `corelink-statuspage-real`, and `corelink-slack-real`.

Target selection belongs outside this facade at the relevant build/workspace and consumer boundary. The crate’s manifest and source text cannot prove a selected target, target compatibility, a successful build, consumer migration, provider connectivity, or runtime reachability. See [B02](BLAST_RADIUS.md#b02) for directed mappings and [M06](MAINTENANCE.md#m06) for handoff limits.

<a id="r03"></a>
## R03 — Implementation map

| Local module | Canonical public path | Direct re-export target | Local responsibility |
|---|---|---|---|
| `cf` | `corelink_adapters_cloud::cf` | `corelink_cf_bindings::*` | Name the facade path |
| `clerk` | `corelink_adapters_cloud::clerk` | `corelink_clerk_cf::*` | Name the facade path |
| `stripe` | `corelink_adapters_cloud::stripe` | `corelink_stripe_real::*` | Name the facade path |
| `statuspage` | `corelink_adapters_cloud::statuspage` | `corelink_statuspage_real::*` | Name the facade path |
| `slack` | `corelink_adapters_cloud::slack` | `corelink_slack_real::*` | Name the facade path |
| `lib` | crate root module declarations | the five modules above | Expose and group facade modules |

The five facade-module rows (`cf`, `clerk`, `stripe`, `statuspage`, and `slack`) are `pub use <upstream>::*`; the `lib` row declares and groups those modules. This evidence identifies a source-level path mapping, not a filtered symbol inventory or target-specific availability claim.

<a id="r04"></a>
## R04 — Public contracts

[API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003).

<a id="api-001"></a>
### API-001 — Five canonical facade modules

The root publicly declares `cf`, `clerk`, `stripe`, `statuspage`, and `slack`. Each name is therefore a source-defined canonical import segment under `corelink_adapters_cloud`. Removing or renaming one is source compatibility risk for an unresolved consumer set. Source evidence: `src/lib.rs`. [Index](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Whole-surface re-export mapping

Each facade module uses a glob re-export of exactly one direct dependency, as listed in R03. No local adapter type, trait, provider client, configuration parser, target selector, or stateful implementation was found in the scoped modules. Source evidence: `src/{cf,clerk,stripe,statuspage,slack}.rs`. [Index](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Upstream-defined symbols remain upstream-defined

Symbols reached through a facade path are defined by the matched direct dependency rather than by this package’s local module text. Their signatures, behavior, visibility under target/feature conditions, and compatibility must be resolved in their implementation package; a re-export does not independently specify them. Source evidence: each local `pub use` statement and manifest dependency entry. [Index](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Falsifiable source invariants

| ID | Predicate tied to source |
|---|---|
| INV-001 | The root declares exactly the five public facade modules named in API-001. |
| INV-002 | Each named facade module contains one glob `pub use` of the one matching direct package target in R03. |
| INV-003 | The scoped facade modules introduce no local provider implementation, configuration parser, target selector, or persistent state. |
| INV-004 | The local path-resolution test module names all five facade modules, but its presence is not proof of test execution or target availability. |

These predicates are directly inspectable source conditions. They do not guarantee that an upstream symbol is present under a particular target/feature selection or imported by any consumer.

<a id="r06"></a>
## R06 — Configuration and compatibility

No package-local feature table, binary target, environment-variable parser, provider binding, or target-selection mechanism appears in the scoped manifest/source. Workspace version, edition, rust-version, license, publish, and lint fields are inherited declarations. The package’s source-level compatibility surface is the five facade module names plus whatever public symbols are re-exported by the upstream packages under an independently selected compilation context.

Changes to a facade name or its re-export target require review as source compatibility risk. A target declaration, conditional export, build result, consumer migration, or provider behavior must be assessed by its appropriate owner and evidence class.

<a id="r07"></a>
## R07 — Failure model

The facade contains no local fallible operation or error taxonomy in the scoped modules. At source level, its material failure mode is a changed or unavailable upstream public surface causing a dependent compilation context to reject an import. Which target/feature context encounters that outcome is unknown here.

No retry, transport, provider error, credential handling, migration rollback, or runtime recovery is implemented evidence for this package. Do not project failures in an upstream implementation onto this facade without inspecting that implementation’s own contract.

<a id="r08"></a>
## R08 — Evidence, quality, and done gate

Evidence class is SOURCE: the manifest and six local modules were inspected at `1c99a5b8ce1db7b622db7811512432a5d070bbbe`. Success for this reference is accurate package identity, complete five-path mapping, explicit separation of facade ownership from implementation ownership and target selection, and falsifiable source predicates. Completeness means R01–R07 cover all scoped local modules and direct dependencies. Quality means no upstream documentation is promoted into a provider, build, migration, or runtime fact.

Definition of done requires the companion blast-radius and maintenance documents, structural validation, scope-only diff, and independent review. Unknown: exact upstream symbol inventories under feature/target combinations, selected targets, build results, reverse consumers, migration state, provider configuration, credentials, network behavior, and runtime reachability.

[Back to start](#r01)
