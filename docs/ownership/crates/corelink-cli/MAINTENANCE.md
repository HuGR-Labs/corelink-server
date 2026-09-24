---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-cli
manifest: tools/cli/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: candidate
evidence_set: corelink-cli-source-static-20260921
---

# corelink-cli — maintenance

S-profile maintenance is SOURCE/DOCUMENTARY only. The modes below authorize static inspection and the specified documentary checks; they do not authorize or produce Cargo/build/test/check/clippy, fuzz, binary invocation, filesystem, network, endpoint, telemetry, package/release, runtime, deployment, approval, or review evidence.

[Baseline](#m01) · [Manifest/facade](#m02) · [Auth/config/output](#m03) · [Telemetry](#m04) · [Validate](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Establish the static baseline

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Assigned worktree and pinned revision. | Package, manifest, and exactly four owned artifacts match the assignment. | Read revision; name manifest and four paths before source review. | Stop on baseline/path mismatch; recover only by reconciling assigned scope. | Revision, paths, source/documentary text. |

<a id="m02"></a>
## M02 — Review manifest or facade change

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | A target, dependency, public module, path attribute, or command declaration changed. | Each changed declaration has one falsifier and separately mapped R01/R02/R03/R07 and B01/B02 impact. | Read `Cargo.toml`, `src/lib.rs`, and/or `src/main.rs`; update only affected atomic records. | Stop when resolution, macro expansion, compilation, generated help, consumer, execution, artifact, or release evidence is required; retain UNKNOWN. | Named manifest/source text. |

<a id="m03"></a>
## M03 — Review auth, config, or output change

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | A resolver branch, parser call, config field/default, redaction helper, format enum, or formatter branch changed. | Resolver order, parser delegation, each changed field/default, and branch are independently recorded in R04/R05 and B03/B04. | Read the named local branch plus direct manifest dependency entry; retain direct arrows only. | Stop when an environment/config value, parser result, authentication, file effect, serialization, stdout, command result, or compatibility proof is required; retain UNKNOWN. | Named local Rust text. |

<a id="m04"></a>
## M04 — Review telemetry source or route an unknown

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S / ROUTE_ONLY | A telemetry field/guard/call changes, or a question exceeds static evidence. | The false guard, payload fields, and B05 endpoints remain falsifiable; external questions are explicitly UNKNOWN. | Read `src/config.rs`, `src/main.rs`, and `src/telemetry.rs`; route broader policy context only to [CLI reference](../../../knowledge/ops/cli-reference.md). | Stop before treating URL text, task source, OKF, docs, or comments as a request, reachability, delivery, retention, or policy proof. Recover by preserving UNKNOWN and escalating to the relevant owner. | Named source text or explicit route; no imported proof. |

<a id="m05"></a>
## M05 — Validate the ownership set

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| DOCUMENTARY_S | Exactly four owned artifacts and supplied S-profile checker. | S01–S07, R01–R08, B01–B06, and M01–M06 exist; checker runs once per artifact; `git diff --name-only 398e586ccef712477f2a4ce51e026443b67e5747 HEAD` is exactly the four owned artifact paths; diff has no whitespace error. | Compare that changed-path list with `.claude/skills/own-corelink-cli/SKILL.md`, `docs/ownership/crates/corelink-cli/REFERENCE.md`, `docs/ownership/crates/corelink-cli/BLAST_RADIUS.md`, and `docs/ownership/crates/corelink-cli/MAINTENANCE.md`; run the supplied checker four times and `git diff --check 398e586ccef712477f2a4ce51e026443b67e5747 HEAD`; correct only owned files. | Stop on checker, whitespace, or path-scope failure; recover only the reported owned file. Never replace this mode with Cargo/build/test/check/clippy or runtime work. | Changed-path list, checker output, and diff status; documentary only, not semantic approval. |

<a id="m06"></a>
## M06 — Handoff with evidence limits

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| DOCUMENTARY_S | Scoped commit and M05 documentary result. | Handoff names baseline, four paths, affected R/B/M IDs, five axioms, direct arrows, check status, OKF route, and material unknowns. | State source/documentary results and request independent review. | Stop before claiming execution, build/test, release, endpoint reachability, telemetry delivery, runtime/deploy, approval, or review evidence; state UNKNOWN. | Commit, paths, checker/diff output. |

The verified canonical OKF route remains [CLI reference](../../../knowledge/ops/cli-reference.md); route to it without copying or revalidating it.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Back to baseline](#m01)
