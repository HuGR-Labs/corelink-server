---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-telemetry
manifest: crates/corelink-telemetry/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: H
state: candidate
evidence_set: telemetry-static-source-20260920
---

# corelink-telemetry — maintenance

The procedures distinguish local source review from graph assessment and
document checks. None authorizes network, credentials, deployment, telemetry
delivery, runtime operation, or production change. No execution is claimed.

[Preparation](#m01) · [Selection](#m02) · [Procedures](#m03) · [Validation](#m04) · [Recovery](#m05) · [Escalation](#m06).

Procedure index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005) · [PROC-006](#proc-006).

[Reference](REFERENCE.md#r01) · [Blast Radius](BLAST_RADIUS.md#b01) · [Skill](../../../../.claude/skills/own-corelink-telemetry/SKILL.md#s01).

<a id="m01"></a>
## M01 — Safe preparation

Pin the requested revision; inspect manifest/root plus the affected family.
Confirm whether the change is in a local implementation or facade route. A
static import does not prove composition, selection, or runtime use.
[Procedure index](#m03)

<a id="m02"></a>
## M02 — Select a procedure

Local public surface/schema → PROC-002; facade/dependency or reverse consumer
search → PROC-003; test-target change → PROC-004; artifact update → PROC-005;
documentary gate → PROC-006. Route tracing/SLO implementation changes to their
defining owners. Stop when asked to establish runtime delivery or deployment.
[Procedure index](#m03)

<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Pin revision and scope
Objective: start a change review. Mode `READ_ONLY`; local checkout; source-read
permission only. Inputs: assigned SHA and paths. Steps: (1) record
`git rev-parse HEAD`; (2) inspect `Cargo.toml`, `src/lib.rs`, and changed root;
(3) compare local/facade ownership to R02. Expected: identity, source revision,
and scope match. Stop on drift or ambiguous owner; re-anchor. Evidence: SHA,
paths, authority table. Review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`;
result `NOT_EXECUTED`.
[Procedure index](#m03)

<a id="proc-002"></a>
### PROC-002 — Trace local public contract
Objective: review local module, type, trait, schema, or error changes. Mode
`READ_ONLY`; pinned local source. Inputs: changed symbols. Steps: trace root
export to defining module; compare the linked API/INV and REL; inspect all
effects/errors in the changed implementation family; update evidence paths.
Expected: claim matches declarations and implementation. Stop if an adapter,
store, endpoint, or delivery result is required. Evidence: root, module, symbol
and diff. Review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; result
`NOT_EXECUTED`.
[Procedure index](#m03)

<a id="proc-003"></a>
### PROC-003 — Trace facade and consumers
Objective: review a dependency/re-export or consumer change. Mode `READ_ONLY`;
tracked-repository source and manifests. Inputs: package/module/symbol names.
Steps: inspect the Cargo edge and facade source; locate imports; classify each
as manifest, re-export, or source call; update REL endpoints and shared key only
when counterpart facts are verified. Expected: located surface has correct
directions and owners. Stop if resolved selection, full consumer graph, or
runtime route is needed. Evidence: manifests, call sites, search terms/results.
Review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; result `NOT_EXECUTED`.
[Procedure index](#m03)

<a id="proc-004"></a>
### PROC-004 — Review test-target boundary
Objective: test target, file, or test-sensitive contract changes. Mode
`READ_ONLY`; local checkout; source reads only. Inputs: target and changed API.
Steps: compare `[[test]]` target/path; read the test assertions and imported
symbols; connect to API/INV and one atomic REL; record exact intended command if
execution is later authorized. Expected: target maps to a real test source and
predicate. Stop before claiming execution or coverage. Evidence: manifest/test
source. Review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; result
`NOT_EXECUTED`.
[Procedure index](#m03)

<a id="proc-005"></a>
### PROC-005 — Update ownership artifacts
Objective: align four artifacts after source change. Mode `LOCAL_ISOLATED`; edit
only assigned docs. Inputs: pinned source diff and record IDs. Steps: update
skill routing, reference API/INV, atomic blast relations, and maintenance
validation/recovery; retain unknowns; run per-file checker. Expected: claims
are supported and relations navigable. Stop if a necessary relation lacks
evidence. Recovery: record capacity/unknown; do not omit it. Evidence: source
and artifact diffs. Review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; result
`NOT_EXECUTED`.
[Procedure index](#m03)

<a id="proc-006"></a>
### PROC-006 — Run documentary checks
Objective: check artifact structure. Mode `READ_ONLY`; local Python and
checked-in checker. Inputs: four files, profile H.

Commands:
```sh
python3 docs/ownership/tools/check_docs.py .claude/skills/own-corelink-telemetry/SKILL.md --kind skill --profile H --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-telemetry/REFERENCE.md --kind reference --profile H --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-telemetry/BLAST_RADIUS.md --kind blast_radius --profile H --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-telemetry/MAINTENANCE.md --kind maintenance --profile H --root .
git diff --check
```

Expected: all checks exit zero. Stop on nonzero; correct the finding and rerun.
Save outputs. Review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; result
`NOT_EXECUTED` until run.
[Procedure index](#m03)

<a id="m04"></a>
## M04 — Validation matrix

Static contract edits use PROC-002/003 and matching source assertions. Declared
test targets are `prop_canary`, `prop_logpush`,
`pii_redaction_100k_synthetic`, `migration_canonical_0016`, `prop_otel_export`,
and `prop_synthetic_pager`.

Candidate command: `cargo test -p corelink-telemetry --test <target>` after
choosing one declared target. This command was not executed here. It is not
production proof.
[Procedure index](#m03)

<a id="m05"></a>
## M05 — Recovery and compatibility

A source revert restores an old facade or local schema definition but cannot
undo data already migrated, telemetry already emitted, remote exporter effects,
or consumer upgrades. For schema/migration changes coordinate version
compatibility and rollout order with the actual storage owner. For facade API
changes, roll forward dependent consumers as needed; do not assume revert fixes
wire or stored-data contracts. Runtime recovery remains operator-owned.
[Procedure index](#m03)

<a id="m06"></a>
## M06 — Escalation and evidence

Escalate defining implementation changes to `corelink-tracing`/`corelink-slo`
owners; composition questions to the container owner; endpoint, secret,
deployment, persistence, and delivery questions to their operators. Record SHA,
source paths, changed APIs/INVs/RELs/PROCs, commands actually run, exit status,
and unresolved unknowns. Static validation is not independent approval.
[Procedure index](#m03)
