---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-rate-headers
manifest: crates/corelink-rate-headers/Cargo.toml
source_commit: 38f43b6d6b7f461edbd3a9bdd5cff34f2fd64018
profile: S
state: author_validated
evidence_set: rate-headers-source-static-20260920
---

# corelink-rate-headers — maintenance

S-profile maintenance is source/documentary only. It does not authorize or
represent Cargo, test, network, HTTP, provider/D1, runtime, deploy, or review
execution as evidence.

[Baseline](#m01) · [Header change](#m02) · [Circuit change](#m03) ·
[Fake change](#m04) · [Validate](#m05) · [Handoff](#m06)

Procedures: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003).

<a id="m01"></a>
## M01 — Establish static baseline

| Mode | Prerequisite | Expected predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| `READ_ONLY` | Assigned worktree and expected revision are supplied. | Revision, package, manifest, and four owned paths match assignment. | Read revision/status, then name each owned path before source review. | Stop on a divergent revision or path; recover only with reconciled scope, never reset. | Git revision and paths; SOURCE/documentary, not execution. |

<a id="m02"></a>
## M02 — Assess a header-surface change

| Mode | Prerequisite | Expected predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| `READ_ONLY` | A changed header symbol/path is known. | The kind, bound, renderer/body field, root export, and each affected consumer relation are separately named. | Read `headers.rs`, `lib.rs`, then bounded consumer text; record only direct arrows. | Stop if HTTP emission or client behavior is needed; recover by retaining it as UNKNOWN. | Named source text; SOURCE only. |

<a id="m03"></a>
## M03 — Assess a circuit-contract change

| Mode | Prerequisite | Expected predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| `READ_ONLY` | A changed circuit/audit/metric symbol or import is known. | The state, decision, predicate, trait, and root route each have a textual falsifier. | Read `circuit.rs`, `audit.rs`, `metrics.rs`, and `lib.rs`; record each local relation. | Stop if provider/D1 action, persisted audit, or runtime transition is needed; recover by marking it UNKNOWN. | Named local source text; SOURCE only. |

<a id="m04"></a>
## M04 — Assess a fake or test declaration

| Mode | Prerequisite | Expected predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| `READ_ONLY` | A fake, declared test target, or migration include is in scope. | Each fake/test/include is labelled declared source rather than an outcome. | Inventory named in-memory/failing types, test target declarations, and include text. | Stop if run status, provider behavior, or migration application is needed; recover by recording only declaration text. | Manifest and source/test text; SOURCE only. |

<a id="m05"></a>
## M05 — Validate the ownership set

| Mode | Prerequisite | Expected predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| `READ_ONLY` | Exactly four assigned artifacts and supplied profile-S checker are available. | S01–S07, R01–R08, B01–B06, M01–M06 render and checks pass without whitespace error. | Run checker once per artifact and `git diff --check` against the pinned baseline; repair only owned files. | Stop on checker/diff/scope failure; recover by correcting only the exact owned artifact finding. | Checker JSON and diff output; DOCUMENTARY, not semantic approval/runtime proof. |

Declared package-test matrix (commands are instructions, not execution evidence):

| Surface | Exact command | Expected predicate | Current state |
|---|---|---|---|
| Header property target | `cargo test -p corelink-rate-headers --locked --offline --test prop_rate_headers` | Local builder, kind and bound assertions pass. | Reviewed-not-executed |
| Migration text target | `cargo test -p corelink-rate-headers --locked --offline --test migration_canonical_0014` | Embedded migration and version text assertions pass; does not prove D1 apply. | Reviewed-not-executed |
| Combined package targets | `cargo test -p corelink-rate-headers --locked --offline` | Both declared test targets selected under the current workspace resolution. | Reviewed-not-executed |

Run these only in `LOCAL_ISOLATED` with a known Rust toolchain/cache; if offline resolution fails, record the missing dependency and do not silently drop `--locked` or `--offline`.

<a id="m06"></a>
## M06 — Handoff with unknowns

| Mode | Prerequisite | Expected predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| `READ_ONLY` | Scoped commit and documentary checks are present. | Handoff names baseline, paths, R/B/M coverage, checks, each consumer relation, canonical route, and unknowns. | Summarize only verified static/documentary facts and list evidence boundaries. | Stop before claiming graph resolution, tests, HTTP, provider/D1, runtime, deploy, or review; recover by stating UNKNOWN. | Commit/path/check output; SOURCE/documentary only. |

The verified canonical OKF route remains [storage quota header](../../../knowledge/tenancy/storage-quota-header.md); route to it without copying or revalidating its policy.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Back to baseline](#m01)

<a id="proc-001"></a>
### PROC-001 — Review header contract change
**Objective/trigger:** header kind, field, bound or renderer changes. **Mode:** `READ_ONLY`; **state:** reviewed-not-executed. **Preconditions/environment:** pinned source/diff; no server/client. **Permissions/inputs:** repository read only; changed symbol and callers. **Steps:** inspect kind literals, clamps, renderers and body mirror; compare API-001 and REL-001/007; inventory declared tests. **Expected predicate:** source inputs, clamps and fields reconcile. **Failure/stop/recovery:** stop at emitted HTTP/client compatibility; require authorized fixture. **Evidence:** anchors, relations, checker/diff.
[Procedure index](#m02)

<a id="proc-002"></a>
### PROC-002 — Review circuit transition change
**Objective/trigger:** state, signal, audit, metric or override edit. **Mode:** `READ_ONLY`; **state:** reviewed-not-executed. **Preconditions/environment:** pinned source; no provider credentials. **Permissions/inputs:** local read only; changed branch. **Steps:** trace check/observation/probe/override and observer results; compare API-002/INV and REL-002–004. **Expected predicate:** each transition/failure edge is source-mapped. **Failure/stop/recovery:** stop at D1, auth, metrics backend or runtime claims; preserve UNKNOWN and escalate. **Evidence:** branch map, relation IDs, diff.
[Procedure index](#m02)

<a id="proc-003"></a>
### PROC-003 — Review migration and test declarations
**Objective/trigger:** embedded SQL, schema version, or test-target change. **Mode:** `READ_ONLY`; **state:** reviewed-not-executed. **Preconditions/environment:** pin, manifest and migration file. **Permissions/inputs:** source only; no Wrangler/D1 credentials. **Steps:** trace `include_str!`, inspect migration-test predicates, list declared targets/features. **Expected predicate:** embedded path/version/test sources reconcile; apply/runtime remain UNKNOWN. **Failure/stop/recovery:** stop before migration execution or applied-schema claim; request authorized operation evidence. **Evidence:** manifest/source/test anchors, REL-008–010.
[Procedure index](#m02)
