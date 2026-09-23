---
schema: corelink-ownership/1.1
document: maintenance
package: e2e-dsr
manifest: tests/e2e-dsr/Cargo.toml
source_commit: cb94e251c0f17382565bf863f517945cbb2a84d6
profile: S
state: draft
evidence_set: e2e-dsr-source-cb94e251c
---

# e2e-dsr — maintenance guide

This is a bounded source/documentation procedure. It does not authorize Cargo or any other Rust command, build, test, property-test, fuzz, network, GitHub, provider, deploy, or production work.

[Preparation](#m01) · [Procedure selection](#m02) · [Procedures](#m03) · [Validation matrix](#m04) · [Recovery](#m05) · [Handoff](#m06)

<a id="m01"></a>
## M01 — Safe preparation

| Mode | Prerequisite | Stop |
|---|---|---|
| `READ_ONLY` | Confirm the baseline and inspect the manifest, local harness, affected target, and ownership artifacts; PROC-003 may read only the named upstream declaration/API source or cited documentary consumer line needed for its relation | Evidence requires execution, credentials, external data, broad graph discovery, or a production action |
| `LOCAL_ISOLATED` | PROC-004/006 edit only the four ownership artifacts; PROC-005 applies only in a separate authorized source task/worktree and only to named harness files | The source task lacks explicit scope, edits another package, or requires an external effect |

Neither mode grants `AUTHORIZED_OPERATION`. This guide does not authorize Cargo or any other Rust command, build, test, property-test, fuzz, network, GitHub, provider, deploy, or production work. Do not read or load secrets or customer data. Start with [R01](REFERENCE.md#r01) and [B03](BLAST_RADIUS.md#b03). Treat prose and test names as declared intent until their exact source assertion is inspected.

<a id="m02"></a>
## M02 — Procedure selection

| Signal | Procedure | Mode |
|---|---|---|
| Manifest, target, or dependency declaration changed | [PROC-001](#proc-001) | `READ_ONLY` |
| Fixture/helper, policy, or R2 contract needs source tracing | [PROC-002](#proc-002) | `READ_ONLY` |
| Upstream contract or cross-package reference changed | [PROC-003](#proc-003) | `READ_ONLY` |
| Ownership artifacts are ready for handoff | [PROC-004](#proc-004) | `LOCAL_ISOLATED` |
| A separately authorized local harness source change is requested | [PROC-005](#proc-005) | `LOCAL_ISOLATED` |
| A separately authorized manifest/target source diff needs ownership-doc updates | [PROC-006](#proc-006) | `LOCAL_ISOLATED` |

<a id="m03"></a>
## M03 — Bounded procedures

**Procedure state:** PROC-001/002/003/005/006 have `execution_status=REVIEWED_NOT_EXECUTED` and `result=NOT_EXECUTED`; PROC-004 has `execution_status=EXECUTED_LOCAL` and `result=PASS`. Each `review_status` is `BLOCKED` pending a fresh independent cold review. **Index:** [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005) · [PROC-006](#proc-006)

<a id="proc-001"></a>
### PROC-001 — Reconcile manifest declarations

**Gatilho / resultado esperado:** manifest facts are under review. Pass when package identity, library path, direct dependencies, dev dependency, and all twelve declared test targets match the manifest anchors; resolved selection/features stay UNKNOWN.

**Mode / environment / permission:** `READ_ONLY`; pinned source snapshot; no Cargo/Rust or external service. `review_status: BLOCKED` because independent cold review is pending. `execution_status: REVIEWED_NOT_EXECUTED`; `result: NOT_EXECUTED`. `required_for_acceptance: true` when manifest identity/declarations are in scope.

**Inputs and validation:** `tests/e2e-dsr/Cargo.toml`; R01/R06; compare recorded lines with source. Do not resolve Cargo selection.

**Steps:**
1. Inspect package, library, dependency, dev-dependency, and test-target declarations.
2. Compare each recorded fact with exact manifest lines.
3. Retain unresolved resolution/selection questions as UNKNOWN.

**Falhas e parada:** mismatch blocks this record; Cargo resolution/execution is out of scope. **Recovery:** correct affected ownership text only. **Limitação:** no target-selection or runtime conclusion. **Evidence:** baseline SHA, manifest line anchors, fact-to-section mapping, explicit UNKNOWNs. [Index](#m02)

<a id="proc-002"></a>

### PROC-002 — Trace a local fixture contract

**Gatilho / resultado esperado:** a fixture contract needs documentation. Pass when one producer-to-consumer source predicate is mapped and manual test mutations/calls remain distinct from automatic wiring; runtime effects remain UNKNOWN.

**Mode / environment / permission:** `READ_ONLY`; pinned source snapshot; no Cargo/Rust, credentials, or provider. `review_status: BLOCKED` pending cold review. `execution_status: REVIEWED_NOT_EXECUTED`; `result: NOT_EXECUTED`. `required_for_acceptance: true` when this contract is in scope.

**Inputs and validation:** exact local helper/policy/R2 source and declared caller; related API/INV/REL records; verify both source anchors.

**Steps:**
1. Read the named local producer and consumer.
2. Identify one source-falsifiable predicate.
3. Separate manual ledger mutation or worker call from any claimed automatic path.
4. Update only the matching evidence record.

**Falhas e parada:** missing producer/consumer evidence blocks the relation; real DSR/MFA/erasure/storage/backend claims stop here. **Recovery:** remove unsupported wording and retain UNKNOWN. **Limitação:** no runtime behavior is inferred. **Evidence:** source paths/lines, predicate, affected IDs, document diff. [Index](#m02)

<a id="proc-003"></a>

### PROC-003 — Coordinate upstream or documentary drift

**Gatilho / resultado esperado:** a named upstream or documentary relation changes. Pass when the direct Cargo edge and documentary reference are separately sourced and classified; complete reverse graphs and runtime compatibility remain UNKNOWN.

**Mode / environment / permission:** `READ_ONLY`; pinned package source snapshot; inspect only the named upstream declaration/API or cited consumer line. `review_status: BLOCKED` pending cold review. `execution_status: REVIEWED_NOT_EXECUTED`; `result: NOT_EXECUTED`. `required_for_acceptance: true` when the relation changes.

**Inputs and validation:** changed manifest/import/call site; direct relation record; named owner; inspect no broader graph.

**Steps:**
1. Read the exact declaration or consumer reference.
2. Classify Cargo and documentary relations separately.
3. Update only the affected relation and handoff.

**Falhas e parada:** requests for a complete graph, compatibility result, or runtime effect block the conclusion. **Recovery:** remove unsupported relation claims and retain owner/evidence UNKNOWN. **Limitação:** only named static evidence is covered. **Evidence:** repository-relative paths/lines, relation ID, unknowns. [Index](#m02)

<a id="proc-004"></a>

### PROC-004 — Prepare documentary handoff

**Gatilho / resultado esperado:** documentation is ready for handoff. Pass when the checker returns `IMPLEMENTED_CHECKS_PASS`, diff exits zero, and both path scopes are recorded.

**Mode / environment / permission:** `LOCAL_ISOLATED`; isolated author worktree; Python 3.14.5; v1.3 checker in `$CHECKER`; no Cargo/Rust/network. `review_status: BLOCKED` pending cold review. `execution_status: EXECUTED_LOCAL`; `result: PASS`. `required_for_acceptance: true`.

**Inputs and validation:** this document, source baseline, candidate SHA, intermediate author SHA, checker.

**Steps:**
1. Run the maintenance checker command below.
2. Run `git diff --check cb94e251c`.
3. Record checker output and separate cumulative baseline paths from the intermediate-commit delta.
4. Hand the candidate commit to an independent reviewer.

**Falhas e parada:** either check fails; do not call it a test, runtime, or approval. **Recovery:** fix docs in scope and rerun. **Limitação:** no semantic or cold-review claim. **Evidence:** commands/output and exact commit/path/blob sets. [Index](#m02)

Checker command (set `$CHECKER` to the validated v1.3 `tools/check_docs.py`; the absolute path is run-specific and is not a reusable default):

```sh
/usr/local/bin/python3 "$CHECKER" docs/ownership/crates/e2e-dsr/MAINTENANCE.md --kind maintenance --profile S --root .
git diff --check cb94e251c
```

See [M06](#m06) for the prior review's exact commit/path/blob evidence. For later handoffs, report the candidate SHA and cumulative and incremental path/blob sets separately.

<a id="proc-005"></a>

### PROC-005 — Change a local harness source contract

**Gatilho / resultado esperado:** an explicitly authorized local Rust harness change is assigned. Pass when one scoped diff maps to a source invariant and affected assertions/records, then is handed to the separate code-validation workflow; compile/test outcome remains UNKNOWN here.

**Mode / environment / permission:** `LOCAL_ISOLATED`; separate clean worktree authorized for the exact source task; in-memory fixtures only; no secrets/network/provider/production. `review_status: BLOCKED` pending cold review. `execution_status: REVIEWED_NOT_EXECUTED`; `result: NOT_EXECUTED`. `required_for_acceptance: true` for source-change review; code acceptance also requires its separately authorized validation gate.

**Inputs and validation:** explicit source-task scope, agreed baseline SHA supplied as `$BASE`, one named `tests/e2e-dsr/src` or `tests/e2e-dsr/tests` file, and affected contract/assertions. `Cargo.toml` is excluded.

**Steps:**
1. Confirm the separate source task authorizes the exact local file.
2. Trace callers/assertions and record one intended invariant.
3. Make one bounded harness change.
4. Inspect the diff and map affected API/INV/REL records.
5. Run `git diff --check "$BASE"`.
6. Stop before Cargo/Rust/build/test; route those gates to the separately authorized code workflow.

**Falhas e parada:** scope expansion, real DSR/erasure/storage, or missing required compile/test evidence stops acceptance. **Recovery:** revert only this source task's change or keep review blocked. **Limitação:** no Rust command or code acceptance is authorized by this manual. **Evidence:** authorization, baseline, exact paths/diff, predicate, validation owner/result. [Index](#m02)

<a id="proc-006"></a>

### PROC-006 — Reconcile docs after a manifest change

**Gatilho / resultado esperado:** an authorized source PR changes this package's Cargo manifest/targets and its reviewed diff is available. Pass when the four ownership artifacts reflect the exact changed declarations and unresolved resolution/selection facts remain UNKNOWN.

**Mode / environment / permission:** `LOCAL_ISOLATED`; isolated documentation worktree, package artifacts only; no Cargo/Rust command. `review_status: BLOCKED` pending independent cold review. `execution_status: REVIEWED_NOT_EXECUTED`; `result: NOT_EXECUTED`. `required_for_acceptance: true` when a manifest/target source diff changes this record.

**Inputs and validation:** source PR diff/commit, `tests/e2e-dsr/Cargo.toml`, affected target source, four ownership artifacts. Source PR/code owner is authoritative for manifest edits; this procedure edits documentation only.

**Steps:**
1. Read the approved/named source diff and exact manifest lines.
2. Route declaration facts through PROC-001; preserve unresolved Cargo resolution as UNKNOWN.
3. Update only the affected package ownership artifacts in a separate documentation task.
4. Run the relevant local document checker and `git diff --check`.
5. Return changed paths and source anchors for independent cold review.

**Falhas e parada:** no reviewed source diff or unclear authority blocks editing; do not change `Cargo.toml` here. **Recovery:** wait for the authoritative source PR or record UNKNOWN. **Limitação:** no source implementation, Cargo resolution, or execution. **Evidence:** source PR/commit, manifest anchors, doc diff, checker/diff outputs. [Index](#m02)


<a id="m04"></a>
## M04 — Change and evidence matrix

| Change | Package / target / features | Exact suite or command | Expected predicate / actual result | Environment / evidence |
|---|---|---|---|---|
| Manifest declaration documentation | `e2e-dsr`; manifest declarations; no package `[features]` block | PROC-001 then PROC-006; no Cargo command | Listed declarations match source; selected targets/resolved features remain UNKNOWN; `NOT_EXECUTED` | Read-only pinned source; manifest anchors |
| Bounded local restriction-ledger harness change | `e2e-dsr`; test target `happy_restriction`; no package `[features]` block; command adds no feature flags and resolved set is UNKNOWN | `cargo test --locked --offline -p e2e-dsr --test happy_restriction` (documented only; NOT EXECUTED) | Test expects explicit `set_restriction(Paused)` to make `evaluate_write` return `Restricted`; no automatic event edge; `result: NOT_EXECUTED` | Locked dependencies available locally; offline host; in-memory fixtures; no secrets/provider; exact target from manifest `:44-46` |
| Ownership-document correction | `e2e-dsr`; MAINTENANCE document; features N/A | `/usr/local/bin/python3 "$CHECKER" docs/ownership/crates/e2e-dsr/MAINTENANCE.md --kind maintenance --profile S --root .`; `git diff --check cb94e251c` | Checker `IMPLEMENTED_CHECKS_PASS`; diff exit `0`; result `PASS` | Python 3.14.5, configured v1.3 checker; run path/output recorded in PROC-004 |

Cargo line above is a future validation command only; it was not run in this documentation task.

<a id="m05"></a>
## M05 — Recovery and evidence retention

If a citation or relation is unsupported, correct the ownership text to match the inspected source or state UNKNOWN. If a check fails, capture its exact output and fix within the four-path documentation scope; do not bypass it. The source evidence inspected here shows the harness policy ledger and R2 evidence client are in-memory local test fixtures, while `Cargo.toml` contains package/dependency/test-target declarations.

These harness and manifest surfaces are not evidence that this package owns persisted data or changes wire/API state. Whether an upstream dependency/API has persistence, wire compatibility, or production effects remains UNKNOWN and belongs to its named package owner; do not infer it from this harness or its manifest.

| Authorized change route | Compatibility and downstream boundary | Local rollback / roll-forward | Evidence required before the change |
|---|---|---|---|
| PROC-005, harness source under `tests/e2e-dsr/src` or `tests/e2e-dsr/tests` | The authorized files are local test/build harness code; current inspected policy/R2 fixtures are process-local in-memory state. A change may alter harness assertions, helper/API usage, or which local test behavior is described, but does not establish persisted-data migration or production wire/API compatibility. Downstream test/CI behavior may change if a target or assertion is affected; exact CI selection and outcome remain UNKNOWN until the separate code-validation workflow reports them. | In the isolated source-task worktree, revert only the exact authorized harness patch or roll forward with a reviewed corrective patch, then reconcile affected ownership records. Do not treat that local patch rollback as rollback of any production DSR/erasure action; no such action is authorized or evidenced here. | Before editing: explicit source-task authorization and owner; baseline SHA; exact file/target and caller/assertion anchors; one expected source predicate; affected API/INV/REL IDs; documented package/target/feature command and expected result from M04; separate validation owner/gate; bounded rollback plan. Keep runtime/persistence and upstream API impact UNKNOWN absent owner evidence. |
| PROC-006, manifest/dependency/target declaration source PR | The manifest surface is a local package/build declaration, not persisted data or wire/API state. Changes can alter dependency availability, library/test target declarations, feature/target selection, and downstream CI inclusion or compilation; actual resolved Cargo selection and CI effects remain UNKNOWN here. A dependency declaration alone does not establish compatibility of the dependency's API or operational effects. | In the separately authorized package source PR, revert or correct only the reviewed manifest/target patch together with any source changes that depend on it; keep documentation aligned through PROC-006. A documentation revert alone cannot undo a source declaration or restore a previous build graph. No Cargo resolution/build is authorized by this guide. | Before editing: source-owner-authorized PR/task and reviewed diff; baseline and exact manifest/target lines; dependency/API owner evidence for changed imports or declarations; named affected targets/features and downstream CI command/predicate; affected ownership IDs; owner-defined rollback/roll-forward plan. If these facts are unavailable, leave them UNKNOWN and route to the package/API owner. |

For either source route, preserve the authorization, baseline, exact changed paths/diff, source anchors, expected predicate, independent validation owner/result, checker/diff output where applicable, and handoff separately. Reverting these ownership-document bytes restores only prior documentation. It does not compensate for, or reverse, any external or production effect; all runtime, data, provider, and production operations remain blocked by this guide.

<a id="m06"></a>
## M06 — Escalation and handoff

Route DSR endpoint/receipt questions to the `corelink-dsr` owner and erasure worker questions to the `corelink-privacy-erasure-worker` owner. The authoritative route for manifest/dependency/target edits is a separately authorized package source PR under repository source-owner review; owner identity and configured gates are UNKNOWN. PROC-006 updates docs only after that source diff exists and never edits `Cargo.toml`.

Route operational questions to the relevant system owner; verified canonical OKF is the [SRE operations hub](../../../knowledge/ops/sre-operations-hub.md), route-only. Handoff records source commit, paths, affected R/B/M IDs, separate procedure states, and five [R08](REFERENCE.md#r08) unknowns.

Prior review evidence, kept distinct by comparison range: snapshot `a364f379d80df7b6afeceac9c73e92ef595e3049` descends from source baseline `cb94e251c0f17382565bf863f517945cbb2a84d6`; checker returned `IMPLEMENTED_CHECKS_PASS` and baseline-to-snapshot diff check exited `0`. The delta from intermediate author commit `44114b024ab1f2ad9ff304387dd1b6468e9694d3` is only MAINTENANCE.md. The cumulative source-baseline-to-snapshot diff covers all four paths below. These are historical blob IDs, not hashes of the corrected file.

| Path at snapshot `a364f379d80df7b6afeceac9c73e92ef595e3049` | Git blob |
|---|---|
| `.claude/skills/own-e2e-dsr/SKILL.md` | `bfeacd85fb8e4837fc401f0b524c7eab026c611b` |
| `docs/ownership/crates/e2e-dsr/BLAST_RADIUS.md` | `1006bdcd23c9b8a939b411b2b1cd102745304d7f` |
| `docs/ownership/crates/e2e-dsr/MAINTENANCE.md` | `3134e8ef1cf62d372956c115bee4b442f1970fcd` |
| `docs/ownership/crates/e2e-dsr/REFERENCE.md` | `2f1d4b2d1cac7592f0684ec6dd4aaea1b750d5d4` |

[Reference](REFERENCE.md#r01) · [Impact map](BLAST_RADIUS.md#b01) · [Start](#m01).
