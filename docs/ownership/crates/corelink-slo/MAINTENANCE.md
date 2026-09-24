---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-slo
manifest: crates/corelink-slo/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: candidate
evidence_set: slo-source-static-20260920
---

# corelink-slo — maintenance

These source-review procedures do not authorize tests, network, deployment,
credentials, PagerDuty, Prometheus, or production operation. Review and
execution states are kept distinct; no procedure below is claimed executed.

[Preparation](#m01) · [Selection](#m02) · [Procedures](#m03) · [Validation](#m04) · [Recovery](#m05) · [Escalation](#m06).

Procedure index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005) · [PROC-006](#proc-006).

[Reference](REFERENCE.md#r01) · [Blast Radius](BLAST_RADIUS.md#b01) · [Skill](../../../../.claude/skills/own-corelink-slo/SKILL.md#s01).

<a id="m01"></a>
## M01 — Safe preparation

Before source assessment, verify the intended repository revision and changed
paths. Read the package identity, manifest, and root exports. Never infer
selected runtime behavior from a Cargo declaration or doc comment.
[Procedure index](#m03)

<a id="m02"></a>
## M02 — Select a procedure

Public API or taxonomy → PROC-002; SLI/target/window/calculation → PROC-003;
audit/state/dispatch → PROC-004; consumer or manifest edge → PROC-005; document
change → PROC-006. Stop if the requested outcome requires external evidence.
[Procedure index](#m03)

<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Pin source and scope
Objective: begin a package review. Mode `READ_ONLY`; local pinned checkout;
source read permission only. Inputs: intended SHA and package paths. Steps:
(1) record `git rev-parse HEAD`; (2) inspect `crates/corelink-slo/Cargo.toml`
and `src/lib.rs`; (3) compare changed paths with this package. Expected:
identity and scope match the assigned revision. Stop on mismatch; recovery is
re-anchor and repeat. Evidence: SHA and path list. Review `BLOCKED`;
execution `REVIEWED_NOT_EXECUTED`; result `NOT_EXECUTED`; no execution claimed.
[Procedure index](#m03)

<a id="proc-002"></a>
### PROC-002 — Trace public API and callers
Objective: review module, symbol, error, or taxonomy changes. Mode `READ_ONLY`;
local checkout; source read only. Inputs: changed symbol list. Steps: follow
`src/lib.rs` into defining module; inspect matching API/INV and REL; search
tracked source/manifests for the symbol; classify façade, test, and direct
consumer edges separately. Expected: each changed public symbol has a located
contract and impact record. Stop on untraced compatibility claim; retain it as
unknown. Evidence: paths, symbol, search scope/output. Review `BLOCKED`;
execution `REVIEWED_NOT_EXECUTED`; result `NOT_EXECUTED`.
[Procedure index](#m03)

<a id="proc-003"></a>
### PROC-003 — Trace definition and burn calculation
Objective: review definition, SLI, sample, window, or decision changes. Mode
`READ_ONLY`; local source checkout. Inputs: affected fields/variants. Steps:
(1) inspect definition validation; (2) trace zero-total calculation and
threshold comparison; (3) compare every canonical window multiplier and
decision arm; (4) update linked API/INV/REL records. Expected: all branches and
units match source. Stop if real measurements or rule evaluation are required;
record unknown and escalate. Evidence: source paths and branch list. Review
`BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; result `NOT_EXECUTED`.
[Procedure index](#m03)

<a id="proc-004"></a>
### PROC-004 — Trace audit, state, and dispatch ordering
Objective: review alert orchestration, audit, ledger, event, or dispatch
changes. Mode `READ_ONLY`; local source. Inputs: changed branches. Steps: trace
each `evaluate` arm; record emit-before-mutation/dispatch order; verify only
page decisions reach dispatch; inspect typed failure propagation. Expected:
source sequence and documented effects agree. Stop before asserting durable
atomicity or provider behavior. Recovery: retain unknown and hand off to runtime
owner. Evidence: branch/call sites. Review `BLOCKED`; execution
`REVIEWED_NOT_EXECUTED`; result `NOT_EXECUTED`.
[Procedure index](#m03)

<a id="proc-005"></a>
### PROC-005 — Reconcile static consumer boundary
Objective: review a manifest, facade, metric string, or public compatibility
change. Mode `READ_ONLY`; repository search only. Inputs: symbol/package names.
Steps: inspect direct manifests, telemetry re-export, and located metric-binding
test sources; label each edge dependency/re-export/test; record uncovered graph
as unknown. Expected: found relations map to REL IDs and local evidence.
Stop if completeness, feature resolution, or runtime selection is needed.
Evidence: exact paths and search scope. Review `BLOCKED`; execution
`REVIEWED_NOT_EXECUTED`; result `NOT_EXECUTED`.
[Procedure index](#m03)

<a id="proc-006"></a>
### PROC-006 — Run documentary checks
Objective: validate changed artifacts. Mode `READ_ONLY`; local Python and
checked-in checker. Inputs: four files, profile S.

Commands:
```sh
python3 docs/ownership/tools/check_docs.py .claude/skills/own-corelink-slo/SKILL.md --kind skill --profile S --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-slo/REFERENCE.md --kind reference --profile S --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-slo/BLAST_RADIUS.md --kind blast_radius --profile S --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-slo/MAINTENANCE.md --kind maintenance --profile S --root .
git diff --check
```

Expected: all checks exit zero. Stop on nonzero; repair finding and rerun.
Save exact outputs. Review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; result
`NOT_EXECUTED` until run.
[Procedure index](#m03)

<a id="m04"></a>
## M04 — Tests and validation matrix

Source changes: the repository declares `cargo test -p corelink-slo` and
integration target `prop_slo`; these are candidate validation commands, not
results. This static-doc repair did not execute them. Run Rust tests only under
the package's approved development workflow, and record exact environment,
command, exit status, and target. The documentary checker validates structure
only. `[VALIDATION-MATRIX.md](../../VALIDATION-MATRIX.md)` is shared routing
context and is not a substitute for a package-specific result.
[Procedure index](#m03)

<a id="m05"></a>
## M05 — Compatibility and recovery

For pure source changes, revert may restore source compatibility but cannot
undo emitted alerts, external incidents, or already-written audit data. A
changed public SLI string, enum, or event may require coordinated consumer
updates and roll-forward; do not assume `git revert` repairs wire or metric
compatibility. Stop for production rollback and obtain the operator's recovery
procedure.
[Procedure index](#m03)

<a id="m06"></a>
## M06 — Escalation and evidence

Escalate source/runtime boundary, complete consumer graph, feature resolution,
credential access, Prometheus, PagerDuty, incident state, audit persistence,
deployment, or policy decisions to their respective owners. Preserve SHA,
paths, symbols, REL/API/INV/PROC IDs, commands actually run, outputs, and
unknowns. Never convert static review or checker output into execution proof.
[Procedure index](#m03)
