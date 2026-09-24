---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-openapi
manifest: tools/openapi/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: candidate
evidence_set: corelink-openapi-source-static-20260921
---

# corelink-openapi — maintenance

S-profile maintenance permits SOURCE and DOCUMENTARY evidence only. It never authorizes Cargo, build, test, parsing, generation, publication, network, deployment, runtime, or independent review as evidence.

[Baseline](#m01) · [Declarations](#m02) · [Axioms](#m03) · [Unknowns](#m04) · [Validation](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Establish static baseline

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Assigned worktree and pinned revision. | Package, manifest, source path, and exactly four owned artifacts match scope. | Read revision; name paths; inspect manifest and `src/lib.rs`. | Stop on mismatch; recover only by reconciling scope. | Revision and static paths. |

<a id="m02"></a>
## M02 — Review declaration change

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Named manifest, constant, module, or parser change. | Each changed declaration has one falsifier and mapped R/B IDs. | Read exact declaration; update atomic records only. | Stop if resolution, compilation, parsing, or external input behavior is needed; retain UNKNOWN. | `Cargo.toml` and `src/lib.rs`. |

<a id="m03"></a>
## M03 — Review five-axiom change

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | A change touches AX-001–AX-005. | Every changed axiom has an exact falsifier in R05. AX-002, AX-003, and AX-005 additionally map to B02, B03, and B04 respectively; AX-001 and AX-004 have no dedicated blast relation and remain R05-only. | Compare changed source text with R05; revise the applicable R05 row and, where mapped, its affected B record. | Stop if file inclusion, document validity, route behavior, or safety outcome is requested; retain UNKNOWN. | Named local source line. |

<a id="m04"></a>
## M04 — Route operational unknown

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| ROUTE_ONLY | Question exceeds static crate evidence. | It is named UNKNOWN and routed without copying/revalidating policy. | Use [Worker edge plane](../../../knowledge/planes/worker-edge.md) as the designated canonical OKF route; identify the required owner/evidence. | Stop before calling it local proof; recover by escalating with the unknown retained. | Explicit unknown and route only. |

<a id="m05"></a>
## M05 — Validate ownership set

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| DOCUMENTARY_S | Exactly four owned artifacts and supplied checker. | S01–S07, R01–R08, B01–B06, M01–M06 exist; the changed-path set against the pinned revision is exactly the four owned artifacts; checker passes once per artifact; diff has no whitespace error. | Run `git diff --name-only 398e586ccef712477f2a4ce51e026443b67e5747 HEAD` and compare its complete output with the four owned paths; run the supplied checker four times; then run `git diff --check 398e586ccef712477f2a4ce51e026443b67e5747 HEAD`. Correct only an owned file. | Stop on checker, diff, or changed-path-scope failure; recover only the reported owned file. | Changed-path output, checker output, and diff status; none is semantic approval. |

<a id="m06"></a>
## M06 — Handoff with limits

| Mode | Prerequisite | Predicate | Steps | Stop / recovery | Evidence |
|---|---|---|---|---|---|
| STATIC_S | Scoped commit and M05 result. | Handoff names revision, paths, affected R/B/M IDs, five axioms, check status, OKF route, and unknowns. | State only direct source relations and documentary checks. | Stop before claiming generation, publication, consumer reachability, test, CI, route serving, or runtime; state UNKNOWN. | Commit, paths, checker/diff result. |

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Back to baseline](#m01)
