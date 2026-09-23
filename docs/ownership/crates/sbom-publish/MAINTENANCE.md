---
schema: corelink-ownership/1.1
document: maintenance
package: sbom-publish
manifest: tools/sbom-publish/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: candidate
evidence_set: w013-sbom-publish-source-static-398e586c
---

# sbom-publish — maintenance

S-profile maintenance permits bounded SOURCE/DOCUMENTARY record work only. It does not
authorize Cargo, build, test, check, clippy, network, GitHub, deployment,
production, upload, publication, release, provider, or runtime execution.

[Baseline](#m01) · [Modes](#m02) · [Procedures](#m03) · [Validation](#m04) · [Recovery](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Static baseline

| Mode | Prerequisite | Predicate | Stop / recovery | Evidence |
|---|---|---|---|---|
| STATIC_S | Assigned worktree and pinned revision. | Package, manifest, and exactly four artifacts match this assignment. | Stop on path/revision mismatch; recover only by restoring scope. | Revision and paths; SOURCE/DOCUMENTARY only. |

<a id="m02"></a>
## M02 — Procedure modes

| Situation | Procedure | Mode | Permitted effect | Stop |
|---|---|---|---|---|
| Manifest declaration, bounded package source, or ownership record changes. | [PROC-001](#proc-001) | STATIC_S | Read and update only the four records. | Any claim requires files outside the assigned package scope or execution. |
| Delivery validation is needed. | [PROC-002](#proc-002) | DOCUMENTARY_S | Run the specified checker and whitespace diff. | Checker/diff/scope failure. |
| A policy or operating question exceeds manifest evidence. | [PROC-003](#proc-003) | ROUTE_ONLY | Name UNKNOWN and route to ADR-S12-001. | Copying, redefining, or revalidating the OKF. |

<a id="m03"></a>
## M03 — Procedures

Index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003).

<a id="proc-001"></a>
### PROC-001 — Record a source-bounded package change

**Trigger / predicate:** a named declaration or bounded package source construct
changes; each affected R predicate or B relation remains individually
falsifiable. **Mode:** STATIC_S.

1. Confirm the pinned revision, package name, manifest path, and four-file scope.
2. Read only relevant package files in S01; update the matching R/B/M record and distinguish declarations from code-level paths.
3. Keep resolution, execution, external interaction, and runtime questions UNKNOWN.

**Stop / recovery:** stop if evidence would require sources beyond the assigned package or execution;
recover by preserving the unknown and routing it. **Evidence:** changed manifest
line plus record identifiers; no execution result.

[Procedure index](#m03)

<a id="proc-002"></a>

### PROC-002 — Validate the ownership set

**Trigger / predicate:** four records are ready; each controlled checker returns
`IMPLEMENTED_CHECKS_PASS` and the baseline whitespace diff is clean. **Mode:** DOCUMENTARY_S.

1. Run the four checker commands in M04 once each.
2. Run the M04 baseline diff command.
3. Correct only a reported owned artifact, then repeat the affected check.

**Stop / recovery:** stop on scope, checker, or diff failure; recover only the
reported owned document. **Evidence:** command output; structural-only, never
semantic approval, independent review, or execution proof.

[Procedure index](#m03)

<a id="proc-003"></a>

### PROC-003 — Route an out-of-scope policy question

**Trigger / predicate:** a question needs policy beyond the manifest. **Mode:** ROUTE_ONLY.

1. Record the question as UNKNOWN.
2. Link [ADR-S12-001](../../../knowledge/adr/adr-s12-001-sbom-cyclonedx-ntia-tsa-dt.md) as the canonical route.

**Stop / recovery:** stop before restating, copying, redefining, or revalidating
the ADR; recover by retaining the unknown. **Evidence:** route and explicit
boundary, not local SOURCE proof.

[Procedure index](#m03)


<a id="m04"></a>
## M04 — Documentary validation matrix

| Change | Command / procedure | Predicate | Evidence limit |
|---|---|---|---|
| Ownership guide | `python3 docs/ownership/tools/check_docs.py --kind skill --profile S --root . .claude/skills/own-sbom-publish/SKILL.md` | Structural pass. | Not semantic approval or execution. |
| Reference | `python3 docs/ownership/tools/check_docs.py --kind reference --profile S --root . docs/ownership/crates/sbom-publish/REFERENCE.md` | Structural pass. | Not semantic approval or execution. |
| Blast radius | `python3 docs/ownership/tools/check_docs.py --kind blast_radius --profile S --root . docs/ownership/crates/sbom-publish/BLAST_RADIUS.md` | Structural pass. | Not semantic approval or execution. |
| Maintenance | `python3 docs/ownership/tools/check_docs.py --kind maintenance --profile S --root . docs/ownership/crates/sbom-publish/MAINTENANCE.md` | Structural pass. | Not semantic approval or execution. |
| Scoped delivery | `git diff --check` | No whitespace error. | Not scope approval or independent review. |

<a id="m05"></a>
## M05 — Recovery and compatibility boundary

| Surface | Reversible state | Safe recovery | Proof boundary |
|---|---|---|---|
| Four owned documents | Repository text only. | Amend only the reported owned document; preserve existing unrelated work. | A clean diff/checker is documentary, not behavior. |
| Package source/manifest | Not modified by this documentation work. | Stop and defer functional changes to their authorized task. | No compatibility, resolution, build, test-result, or runtime claim. |

<a id="m06"></a>
## M06 — Handoff and escalation

Handoff records the baseline, four paths, affected R/B/M IDs, four checker
results, diff result, canonical route, and at least three material unknowns.
The canonical [ADR-S12-001](../../../knowledge/adr/adr-s12-001-sbom-cyclonedx-ntia-tsa-dt.md) remains route/reference only. Do not self-approve; route independent review separately. Stop before representing documentation checks as Cargo, tests, uploads, publication, release, provider, runtime, deployment, or production evidence.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Back to baseline](#m01)
