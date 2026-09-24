---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-dsr-statuspage-scheduler
manifest: crates/corelink-dsr-statuspage-scheduler/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: draft
evidence_set: w012-dsr-statuspage-scheduler-static-20260920
---

# corelink-dsr-statuspage-scheduler — maintenance

SOURCE-static procedures only. They do not run or establish Cargo/tests,
scheduling, Statuspage API activity, D1/audit effects, deployment, or runtime behavior.

[Baseline](#m01) · [Root](#m02) · [Targets](#m03) · [Contracts](#m04) · [Relations](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Fix the baseline

**Mode:** SOURCE. **Predicate:** package identity, manifest path, and source
commit match R01. **Action:** inspect the manifest and named local source.
**Stop/recovery:** stop on another baseline; obtain the scoped snapshot. **Evidence:**
manifest/source text, not execution.

<a id="m02"></a>
## M02 — Reconcile root and exports

**Mode:** SOURCE. **Predicate:** R02/B01 still identify every module and re-export.
**Action:** map each root change to one affected contract. **Stop/recovery:** stop
for consumer or invocation evidence; record it UNKNOWN. **Evidence:** `src/lib.rs`.

<a id="m03"></a>
## M03 — Reconcile target declarations

**Mode:** SOURCE. **Predicate:** R03, AX-DSRSP-01, and B02 keep manifest target
blocks and source `cfg` text distinct. **Action:** compare declarations only.
**Stop/recovery:** stop for target selection, Cargo/build/link, or wasm behavior;
route to separately selected evidence. **Evidence:** manifest and source text.

<a id="m04"></a>
## M04 — Reconcile contracts and axioms

**Mode:** SOURCE. **Predicate:** each changed trait, branch, fake, or helper maps
to R04–R06, one AX-DSRSP axiom, and B03–B05. **Action:** name the textual
falsifier and retain the non-inference boundary. **Stop/recovery:** stop for D1,
audit, API, credentials, schedule, or runtime facts. **Evidence:** named local source.

<a id="m05"></a>
## M05 — Validate documentary artifacts

**Mode:** DOCUMENTARY. **Predicate:** only the assigned skill and three package
documents changed; S01–S07, R01–R08, B01–B06, and M01–M06 exist; links resolve.
**Action:** run the supplied profile-S checker once per kind and baseline
whitespace diff. **Stop/recovery:** repair only these four paths on a structural
or whitespace failure. **Evidence:** checker/diff output is documentary only.

```sh
python3 "$CHECKER" --kind skill --profile S --root . .claude/skills/own-corelink-dsr-statuspage-scheduler/SKILL.md
python3 "$CHECKER" --kind reference --profile S --root . docs/ownership/crates/corelink-dsr-statuspage-scheduler/REFERENCE.md
python3 "$CHECKER" --kind blast_radius --profile S --root . docs/ownership/crates/corelink-dsr-statuspage-scheduler/BLAST_RADIUS.md
python3 "$CHECKER" --kind maintenance --profile S --root . docs/ownership/crates/corelink-dsr-statuspage-scheduler/MAINTENANCE.md
git diff --check 3feae2baed63061354533ffdfe2d94acfd24aa9a..HEAD
```

`$CHECKER` is externally supplied and unintegrated. If absent, report BLOCKED;
do not call a missing checker a pass. A pass is not semantic approval or
scheduling/API/runtime evidence.

<a id="m06"></a>
## M06 — Handoff

**Mode:** DOCUMENTARY. **Predicate:** M01–M05 are satisfied or any unavailable
checker is reported literally. **Action:** hand off baseline, paths, R/B/M IDs,
five axioms, five unknowns, four checker results, and diff result; route verified
OKF context only to the [SRE operations hub](../../../knowledge/ops/sre-operations-hub.md).
**Stop/recovery:** replace any operational claim with the matching R08 unknown.

Definition of Done is structural documentation validation and a scoped whitespace
diff, not Cargo/test, schedule, API, D1, deployment, or runtime evidence.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership guide](../../../../.claude/skills/own-corelink-dsr-statuspage-scheduler/SKILL.md#s01)
