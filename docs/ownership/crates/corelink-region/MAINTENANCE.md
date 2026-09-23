---
schema: corelink-ownership/1.1
document: maintenance
package: corelink-region
manifest: crates/corelink-region/Cargo.toml
source_commit: 6ed297f5b2b64cf97447985111a2ecbbaa9536bb
profile: S
state: candidate
evidence_set: corelink-region-static-source-20260920
---

# corelink-region — maintenance

Perform these source-only procedures when the package contracts change. Each
procedure produces reviewable source evidence; none authorizes Cargo, builds,
tests, networking, deployment, GitHub actions, production access, or a claim
that a declared runtime path has executed.

[Surface](#m01) · [Vocabulary](#m02) · [Events](#m03) · [Metrics](#m04) · [Migration](#m05) · [Artifact gate](#m06).

[Reference contracts](REFERENCE.md#r01) · [Blast-radius relations](BLAST_RADIUS.md#b01) · [Ownership skill](../../../../.claude/skills/own-corelink-region/SKILL.md#s01).

Procedure index: [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005) · [PROC-006](#proc-006).

<a id="m01"></a>
## M01 — Review package surface

<a id="proc-001"></a>
### PROC-001 — Reconcile manifest and root exports
**Objective/trigger:** manifest, root module, or export changes. **Mode/environment/permission:** `READ_ONLY`; pinned source checkout; no network or credentials. **Inputs/preconditions:** assigned source SHA and package paths. **Steps:** (1) read `Cargo.toml` and `src/lib.rs`; (2) compare identity, dependency classes, targets, modules, and re-exports to R01/R03; (3) update mismatches.

**Expected predicate:** every recorded declaration matches source. **Stop/failure:** identity or source scope differs. **Recovery:** record the discrepancy as unknown and re-anchor. **Evidence:** SHA, files/symbols read, diff. **Status:** review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; result `NOT_EXECUTED`; acceptance required; review evidence `R01/R03`; execution evidence none; limitation: no build or consumer proof. [Index](#m01)

<a id="m02"></a>
## M02 — Review vocabulary and jurisdiction

<a id="proc-002"></a>
### PROC-002 — Trace region mapping contracts
**Objective/trigger:** region, name, jurisdiction, or health enum changes. **Mode/environment/permission:** `READ_ONLY`; pinned source; source access only. **Inputs/preconditions:** affected symbols and requested change. **Steps:** (1) inspect `region.rs`; (2) compare `Region::ALL`, parser, naming helpers, jurisdiction mapping, status labels, INV-001/002; (3) update R04 and REL-001–003.

**Expected predicate:** each documented branch matches source. **Stop/failure:** a provider or policy conclusion is required. **Recovery:** retain the unknown and route to authorized evidence. **Evidence:** source paths/symbols and reviewed diff. **Status:** review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; result `NOT_EXECUTED`; acceptance required; review evidence `R04`, `INV-001/002`; execution evidence none; limitation: no live region or enforcement. [Index](#m02)

<a id="m03"></a>
## M03 — Review event and audit contracts

<a id="proc-003"></a>
### PROC-003 — Reconcile event and audit boundary
**Objective/trigger:** event constant, record, sink, or error change. **Mode/environment/permission:** `READ_ONLY`; pinned source; no provider access. **Inputs/preconditions:** event/audit paths and changed symbols. **Steps:** (1) inspect `event.rs`, `audit.rs`, `error.rs`; (2) trace constructors to sink interface; (3) revise API-003/REL-004.

**Expected predicate:** event strings, fields, signatures, and fixture outcomes match source. **Stop/failure:** external delivery or audit durability is requested. **Recovery:** remove unsupported claim and record unknown. **Evidence:** symbol paths and diff. **Status:** review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; result `NOT_EXECUTED`; acceptance required; review evidence `API-003/REL-004`; execution evidence none; limitation: no emitted event or delivery proof. [Index](#m03)

<a id="m04"></a>
## M04 — Review metrics and probe contracts

<a id="proc-004"></a>
### PROC-004 — Trace metrics and probes
**Objective/trigger:** metric, sample, probe trait, fixture, or threshold changes. **Mode/environment/permission:** `READ_ONLY`; pinned source; no endpoint access. **Inputs/preconditions:** affected metric/probe names. **Steps:** (1) inspect `metrics.rs` and the five probe modules; (2) classify vectors as local state and interfaces/fakes separately; (3) update R06, the affected API-002/API-005–API-009 contracts, and REL-003/REL-011–REL-015; (4) include REL-010 when the region/SLO binding assertion changes.

**Expected predicate:** metric names, probe sample fields, thresholds, and trait methods match their defining modules. **Stop/failure:** export, replica, alert, or SLO execution is inferred without source evidence. **Recovery:** remove claim and retain unknown. **Evidence:** `R06`, `API-002`, `API-005`–`API-009`, `REL-003`, `REL-011`–`REL-015`; include `REL-010` when the region/SLO metric-binding assertion changes. Execution evidence: none; limitation: no metric/probe result observed. [Index](#m04)

<a id="m05"></a>
## M05 — Review migration contracts

<a id="proc-005"></a>
### PROC-005 — Trace migration result state
**Objective/trigger:** migration decision/report/error change. **Mode/environment/permission:** `READ_ONLY`; pinned source; no data access. **Inputs/preconditions:** changed report/decision symbols. **Steps:** (1) inspect `migration.rs` and `error.rs`; (2) trace constructors, `add_tenant_result`, counters, `complete`, `has_failures`; (3) update API-004, INV-004, REL-006.

**Expected predicate:** report state changes only as documented. **Stop/failure:** execution, copied data, hash verification, or rollback claim is needed. **Recovery:** state the unknown and hand off to an operation owner. **Evidence:** source branches and diff. **Status:** review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; result `NOT_EXECUTED`; acceptance required; review evidence `API-004/INV-004`; execution evidence none; limitation: no migration is asserted. [Index](#m05)

<a id="m06"></a>
## M06 — Run artifact gate

<a id="proc-006"></a>
### PROC-006 — Validate assigned documents
**Objective/trigger:** any ownership artifact changes. **Mode/environment/permission:** `READ_ONLY`; local Python/checker, no writes. **Inputs:** four paths/profile S. **Commands:**
```sh
python3 docs/ownership/tools/check_docs.py .claude/skills/own-corelink-region/SKILL.md --kind skill --profile S --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-region/REFERENCE.md --kind reference --profile S --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-region/BLAST_RADIUS.md --kind blast_radius --profile S --root .
python3 docs/ownership/tools/check_docs.py docs/ownership/crates/corelink-region/MAINTENANCE.md --kind maintenance --profile S --root .
git diff --check
```
**Expected predicate:** all four structural checks and diff check exit zero. **Stop:** any nonzero status or scope drift. **Recovery:** fix the finding and rerun affected gate. **Evidence:** exact commands, outputs, and paths. **Status:** review `BLOCKED`; execution `REVIEWED_NOT_EXECUTED`; result `NOT_EXECUTED`; acceptance required; limitation: not semantic, runtime, or cold-review approval. [Index](#m06)
