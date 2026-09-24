---
schema: corelink-ownership/1.1
document: reference
package: corelink-privacy
manifest: crates/corelink-privacy/Cargo.toml
source_commit: 8b800acd3ffb042e5f68bedbb989415a5c5b0bbe
profile: H
state: draft
evidence_set: privacy-source-static-20260920
---

# corelink-privacy — ownership reference

H-profile static reference for the hybrid privacy umbrella. The verified OKF
privacy/compliance concept remains the canonical cross-cutting reference; this
document applies only the package's observable source boundary and does not copy
or redefine that concept.

[Identity](#r01) · [Boundary](#r02) · [Local modules](#r03) ·
[External facades](#r04) · [Public paths](#r05) · [Static surface](#r06) ·
[Consumers](#r07) · [Unknowns](#r08)

<a id="r01"></a>
## R01 — Package identity

| Field | Static fact |
|---|---|
| Package | `corelink-privacy`; `crates/corelink-privacy/Cargo.toml` |
| Root | `src/lib.rs` declares nine root module paths |
| Role | One import umbrella containing both local implementations and thin facades |
| Canonical cross-cutting reference | `docs/knowledge/crates/privacy-compliance.md` |

<a id="r02"></a>
## R02 — Ownership boundary

| Class | Paths | Ownership conclusion |
|---|---|---|
| Absorbed local implementation | `breach`, `consent`, `notice`, `residency`, `sub_processor`, `dpa::versioning` | Source under `crates/corelink-privacy/src/` belongs to this package boundary |
| External facade | `dsr`, `dsr::statuspage`, `erasure`, `pseudonymize`, `dpa::acceptance` | These public routes delegate through `pub use`; their defining implementation remains outside this package |

Re-export creates a name route only. It does not transfer DSR, worker,
pseudonymizer, or DPA implementation ownership, and does not establish runtime
ownership. Evidence: `src/{dsr,erasure,pseudonymize,dpa}.rs`; `Cargo.toml`.

<a id="r03"></a>
## R03 — Absorbed local implementation map

| Module / submodules | Public contract anchors | Source-stated invariant or boundary |
|---|---|---|
| `breach`: `event`, `error`, `metrics`, `audit_emit` | `BreachNotificationDispatch`, `escalation_policy_for`, `BreachAuditSink`, `BreachEmitError` | escalation mapping is deterministic; audit emit precedes state mutation. Source documents dispatch priority on audit failure, so do not infer generic fail-closed dispatch behavior. |
| `consent`: `audit_emit`, `cascade`, `error`, `hmac_sign`, `ledger`, `locale_enforce`, `notice_version_check`, `schema`, `store` | `ConsentLedger`, `ConsentProofPayload`, `ConsentAuditSink`, `ConsentHmacSigner`, `ConsentStore` | grant/revoke proof uses the declared six-field schema and HMAC; audit precedes store mutation and failure aborts locally. |
| `notice`: `audit`, `emitter`, `error`, `event`, `store` | `NoticeEmitter`, `NoticePublishRequest`, `NoticeEmitDecision`, `notice_text_hash` | three locales are required; hash normalization is deterministic; major bump triggers re-consent; audit failure leaves local state unchanged. |
| `residency`: `assert_request`, `assert_write`, `audit_emit`, `enforcement`, `error`, `migration`, `region` | `Region`, `TenantCtx`, `assert_request_residency`, `assert_write_residency`, `ResidencyEnforcement` | request/write region checks use tenant primary region; audit failure prevents local mutation; external routing/D1 backstop remain unwired here. |
| `sub_processor`: `audit`, `broadcast`, `dkim`, `emitter`, `error`, `event`, `objection` | `SubProcessorEmitter`, `BroadcastStore`, `ObjectionStore`, `derive_dkim_key` | audit-before-mutation; broadcast key is idempotent; DKIM derivation is tenant-scoped; canonical plans are all included by source contract. |
| `dpa::versioning`: `broadcast`, `cron`, `error`, `metrics`, `middleware`, `re_accept`, `schema`, `store`, `version`, `versioning` | `DpaVersioning`, `ReadOnlyDegradeGate`, `SemverVersion`, `TenantDpaState`, `ReAcceptanceReceipt` | major-bump grace boundary is 30 days; after grace, reads stay allowed and writes are denied except re-acceptance; re-acceptance is version-matched/idempotent. |

This is a bounded contract index, not a symbol-by-symbol inventory of every
public declaration. These anchors direct review to source-level contracts.

Trait and in-memory types do not prove an external implementation. Evidence:
`src/{breach,consent,notice,residency,sub_processor,dpa}.rs` and the matching
subdirectories. For facade contracts, see R04.

<a id="r04"></a>
## R04 — External facade map

| Public path | Static construction | Defining implementation boundary |
|---|---|---|
| `dsr` | `pub use corelink_dsr::*` | `corelink-dsr` |
| `dsr::statuspage` | nested `pub use corelink_dsr_statuspage_scheduler::*` | `corelink-dsr-statuspage-scheduler` |
| `erasure` | `pub use corelink_privacy_erasure_worker::*` | `corelink-privacy-erasure-worker` |
| `pseudonymize` | `pub use corelink_privacy_pseudonymize::*` | `corelink-privacy-pseudonymize` |
| `dpa::acceptance` | nested `pub use corelink_dpa_acceptance::*` | `corelink-dpa-acceptance` |

The manifest declares the five defining packages as workspace dependencies.
Changing a facade path can affect imports without making the umbrella the owner
of its delegated behavior. Evidence: `Cargo.toml`; `src/{dsr,erasure,pseudonymize,dpa}.rs`.

<a id="r05"></a>
## R05 — Public path and local contract index

`src/lib.rs` declares `breach`, `consent`, `dpa`, `dsr`, `erasure`, `notice`,
`pseudonymize`, `residency`, and `sub_processor`. A change to one declaration,
its local module file, or its re-export can change the corresponding source-level
import path. The root also forbids unsafe code and denies missing documentation.

For local behavioral review, pair the path with its R03 row. A changed public
trait, schema, state type, or decision function can break consumers while the
root path stays unchanged. Evidence: `src/lib.rs` and the local module files
named in R03.

<a id="r06"></a>
## R06 — Static module and test surface

The manifest declares development dependencies; the 23 checked-in integration
test files cover breach, consent, DPA versioning, notice, residency, and
sub-processor families. Filenames identify source-adjacent review surfaces,
not passed tests or complete contract coverage. No execution result is
recorded. Evidence: `Cargo.toml`; `tests/*.rs`.

<a id="r07"></a>
## R07 — Static consumer census

The workspace root declares `corelink-privacy` as a workspace dependency. This
source pass found package-local imports in its checked-in tests and no separate
non-test manifest declaration of `corelink-privacy`. That is not an exhaustive
reverse-dependency result and does not establish that the package is selected by
any build. Evidence: root `Cargo.toml`; `crates/corelink-privacy/tests/*.rs`.

<a id="r08"></a>
## R08 — Evidence limits and unknowns

Evidence is SOURCE: the manifest, root, local module roots/submodules, external
facade files, checked-in tests, workspace declaration, and the canonical OKF
reference. Unknown: complete resolved consumer graph, feature selection, source
semantics of each external package, adapter configuration, data handling,
deployment, execution, and independent review. A re-export or source comment
does not close any of these gaps.

[Guide](../../../../.claude/skills/own-corelink-privacy/SKILL.md#s01) ·
[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01)
