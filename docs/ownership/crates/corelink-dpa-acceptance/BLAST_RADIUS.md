---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-dpa-acceptance
manifest: crates/corelink-dpa-acceptance/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-dpa-acceptance-structural-normalization-20260921
---

# corelink-dpa-acceptance — blast radius

Source/static relation map. It separates declared/reference relations from delivered behavior.

[Scope](#b01) · [Method](#b02) · [Relations](#b03) · [Propagation](#b04) · [Changes](#b05) · [Coverage](#b06).

<a id="b01"></a>
## B01 — Scope

Changes can alter consent proof compatibility, locale/hash rejection, receipt verification, replay identity, audit ordering, or effect-seam contracts. They do not by themselves change a legal notice, configure a key, write D1, send email, mount a route, or process onboarding traffic.

<a id="b02"></a>
## B02 — Static method and limit

Relations below come from `Cargo.toml`, package source, and static references in `corelink-privacy`, `corelink-container`, and `e2e-signup-flow`. A dependency or source reference means only that source coupling was found. No graph resolution, compilation, test execution, deployment, or runtime observation is claimed.

<a id="b03"></a>
## B03 — Atomic relations

| ID | Type / direction | Surface | Activation / limit |
|---|---|---|---|
| REL-001 | dependency, privacy → DPA | thin `dpa::acceptance` re-export | import compatibility; not runtime use |
| REL-002 | dependency, container → DPA | locale, hash, schema, receipt primitives | static route source reference; mount/traffic unknown |
| REL-003 | consumer reference, e2e-signup-flow → DPA | signup acceptance flow | static consumer set supplied for this package; execution unknown |
| REL-004 | flow, `TenantCtx` → `service::accept` → locale/hash checks | request proof and resolved locale | failure precedes acceptance path in source |
| REL-005 | flow, service → audit → store | accepted event then `insert_idempotent` | only trait/fake semantics observed |
| REL-006 | flow, service → JWT receipt → notification envelope | JTI/receipt propagation | notification is a trait; delivery unknown |
| REL-007 | contract, JWT signer ↔ verifier | RS256, PEM, optional exact `kid` | configured keys and external verifier unknown |
| REL-008 | state relation, signup ID → record | lookup then in-memory map insert | durable uniqueness/transactionality unknown |

<a id="b04"></a>
## B04 — Propagation paths

`ConsentProofPayload` or locale changes propagate to source consumers through public types and into hash/idempotency comparisons. JWT claim, algorithm, or `kid` changes propagate to receipt producers/verifiers. Store/audit trait changes propagate to implementations; notification changes propagate to envelope consumers. Each path stops at the unobserved adapter, key, legal-content, route, or runtime boundary.

<a id="b05"></a>
## B05 — Change controls

| Change | Required static relation/invariant | Stop / recovery |
|---|---|---|
| Add/remove/rename proof field | API-001; REL-001–004; INV-001–003 | Hold for privacy/container/e2e consumer decision |
| Change locale or hash rule | API-002; REL-004; INV-001/002 | Hold for canonical legal-content owner; do not invent content |
| Change signup identity or replay | API-003; REL-008; INV-003 | Hold for durable-store semantics and migration owner |
| Change JWT/claims/key ID | API-004; REL-007; INV-004 | Hold for key/rotation owner and receipt compatibility |
| Change ordering or sink result | API-005; REL-005/006; INV-005 | Hold for audit/notification delivery guarantees |

<a id="b06"></a>
## B06 — Coverage and unknowns

Covered: package modules, manifest, declared property/integration targets, and the three supplied static direct consumers. Unknown: exhaustive consumers/features, actual legal texts, configured RSA keys, D1 rows/migrations in use, audit chain, email system, mounted container route, e2e execution, user data, deployment, and runtime behavior. A future resolved graph or runtime trace must be recorded as new evidence rather than inferred from these relations.
