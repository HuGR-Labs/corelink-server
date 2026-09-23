---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-pat
manifest: crates/corelink-pat/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-pat-structural-normalization-20260921
---

# corelink-pat — maintenance

Source/static maintenance guide for an isolated checkout. It does not authorize
Cargo execution, credential handling, store inspection, network access,
deployment, runtime operations, or cold review.

[Baseline](#m01) · [Scope](#m02) · [Format](#m03) · [Crypto](#m04) · [Consumers](#m05) · [Handoff](#m06).

<a id="m01"></a>
## M01 — Baseline control

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | Assigned checkout and expected SHA are known | Read status, manifest, and target source before assessment | Stop on baseline/scope divergence; recover by obtaining the intended baseline without reset | SHA, branch, porcelain status, inspected paths |

<a id="m02"></a>
## M02 — Scope selection

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_LOCAL | Requested symbol maps to one of eight PAT modules | Map the change to its R04 contract and B01–B05 relations | Stop if it requires key custody, a token store, request wiring, or a real secret; route to that owner | `src/lib.rs`, R03, R08 |

<a id="m03"></a>
## M03 — Format, types, or scopes change

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_COMPAT | Change names a segment, `Pat*` type, scope bit/mask/name, or error | Compare `format`, `mint`, `types`, `scopes`, `error`, and root exports; record INV-001 or INV-004 impact | Stop when compatibility of stored/external tokens or authorization is needed; recover with a consumer/store decision | R04–R05, REL-001/004/006, static consumers |

<a id="m04"></a>
## M04 — Argon2id, HMAC, comparison, or dummy primitive change

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_SECURITY | Change identifies a local crypto primitive and its inputs/outputs | Trace parse → token-id compare → HMAC → Argon2id and the separate dummy primitive; retain relevant invariant | Stop if timing measurement, real signing key, rotation operation, or middleware completeness is required; recover by recording the unknown and escalating to the authorized owner | `argon.rs`, `sig.rs`, `verify.rs`, INV-002/003/005, REL-002/003/005 |

<a id="m05"></a>
## M05 — Static consumer and target assessment

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_GRAPH | Requested contract change identifies direct consumers and declared targets separately | Inspect auth/container/worker/CLI manifests and named import sites; inspect PAT's declared targets only | Stop if a complete graph, feature selection, test execution, or runtime route is necessary; recover with fresh authorized evidence | B03–B06, `Cargo.toml`, test target declarations |

<a id="m06"></a>
## M06 — Static handoff

| Mode | Predicate | Action | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_HANDOFF | Documents state baseline, source claims, relations, and unknowns consistently | Run only the approved document-structure checks and `git diff --check`; report exact results and changed paths | Stop on checker/diff failure or unsupported claim; recover by correcting only assigned artifacts | R08, B06, checker output, diff check |

No procedure above proves that a command, test, runtime request, recovery,
deployment, or cold review occurred. Never load, print, construct from, or
record a real signing key or PAT plaintext while carrying out this static work.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Start](#m01)
