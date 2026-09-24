---
schema: corelink-ownership/1.1
document: maintenance
package: chaos-campaign
manifest: tests/chaos/Cargo.toml
source_commit: cb94e251c0f17382565bf863f517945cbb2a84d6
profile: S
state: candidate
evidence_set: chaos-campaign-static-cb94e251c
---

# chaos-campaign — maintenance

Procedures are scoped to static ownership updates and isolated local validation. They do not authorize a live chaos campaign, provider/database/network access, GitHub actions, deployment, or production changes. The [canonical OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) is the designated route/reference only; it is neither copied nor revalidated.

[Preparation](#m01) · [Select](#m02) · [Procedures](#m03) · [Validation](#m04) · [Recovery](#m05) · [Escalation](#m06).

<a id="m01"></a>
## M01 — Preparation and safe environment

Confirm checkout and source commit. Inspect root `Cargo.toml` membership and `Cargo.lock`, full package manifest and all targets, `src/lib.rs` and 11 test sources. For drift inspect every normal/dev/build/target-specific dependency declaration. Zero declarations are a finding, not a reason to skip manifest diffs.

No credentials or service endpoints are inputs. Future PROC-003 needs isolated checkout, Rust toolchain, no secrets and offline cache; otherwise stop without downloads or lockfile edits. This task is static-only; no Cargo/build/test/fuzz command is run.

`CHECKER` and `DOC_CHECKER` are caller-provided Python/checker paths, not repository defaults. Checker output is documentary structure only.

<a id="m02"></a>
## M02 — Procedure selection

| Trigger | Procedure | Mode | Effect | Extra authority |
|---|---|---|---|---|
| Before or after changing package/source evidence | [PROC-001](#proc-001) | READ_ONLY | Inspect baselines and manifest/source drift. | None; no edits. |
| Editing local model/test source or this package's owner docs | [PROC-002](#proc-002) | LOCAL_ISOLATED | Change only declared local scope; update linked records. | No external effects; source-only change needs review. |
| Validating a behavior-changing model/test-source patch | [PROC-003](#proc-003) | LOCAL_ISOLATED | Compile/run the in-memory local tests under opt-in feature. | Separate task approval; not permitted in a static-only authoring task. |
| Request to trigger real chaos, call a provider, or change production | [PROC-004](#proc-004) | AUTHORIZED_OPERATION | No action until explicit operation authority and owner exist. | Explicit, operation-specific authorization and approved runbook required. |
| Validating these four ownership documents | [PROC-005](#proc-005) | LOCAL_ISOLATED | Structural S checks and whitespace/scope diff only. | Does not certify semantics, runtime, or cold review. |

<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Reconcile source and manifest baseline

**Trigger / objective:** Source, target, package, dependency, or ownership claim may have drifted; establish exact bounded inputs before documentation edits.
**Mode / environment / permission:** `READ_ONLY`; repository checkout and Git/text search only; no Cargo or external service.
**Inputs / preconditions:** Confirm root, source base, and clean review intent; inspect root `Cargo.toml`, `Cargo.lock`, `tests/chaos/Cargo.toml`, `src/lib.rs`, and 11 manifest paths.

1. Run `git diff --quiet cb94e251c0f17382565bf863f517945cbb2a84d6 HEAD -- Cargo.toml Cargo.lock tests/chaos/Cargo.toml tests/chaos/src tests/chaos/tests`; expect exit 0 for unchanged baseline inputs.
2. Read root workspace membership and every package dependency table family listed in M01; search exact package/crate identifiers in all tracked manifests and Rust sources.
3. Reconcile every added/removed target, key/alias/source in each dependency declaration, root member change, external Rust import, and changed matching file to a record ID.

**Expected predicate:** Source evidence equals baseline; counts reconcile by named population. **Stop/failure:** Nonzero diff, missing source, unexplained edge, or incomplete inverse search → mark stale. **Recovery:** Re-pin only with authorization; never overwrite another owner's source. **Evidence:** command output, paths/terms, counts. **Registry:** `id=PROC-001`; `mode=READ_ONLY`; `review_status=BLOCKED`; `execution_status=EXECUTED_LOCAL`; `required_for_acceptance=true`; `environment=isolated author checkout`; `result=PASS`; `review_evidence=fresh cold review pending`; `execution_evidence=source-baseline and manifest-census outputs`; `limitations=no Cargo resolution or runtime`. [Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Make a local source or ownership change

**Trigger / objective:** Authorized edit to one model/test contract or its four records, with atomic source facts.
**Mode / environment / permission:** `LOCAL_ISOLATED`; isolated checkout, no network/provider/runtime configuration; edit only task-authorized files.
**Inputs / preconditions:** PROC-001 passes; name affected `API`/`INV`/`REL` and target; save patch and recovery plan before public-contract edits.

1. Make the smallest source/test edit requested; do not alter production analogues by assumption.
2. Update exact API/INV, affected target REL and recovery; enumerate every new dependency declaration and inverse consumer.
3. Inspect `git diff -- Cargo.toml Cargo.lock tests/chaos/Cargo.toml tests/chaos/src tests/chaos/tests`; expect only authorized local source paths.
4. If behavior changed, run PROC-003 in its isolated local mode; request independent review for the changed hashes.

**Expected predicate:** Each changed symbol has reference and impact records. **Stop:** Production integration, unknown owner, or undefined rollback. **Recovery:** Restore only author's scoped patch; do not reset shared work. **Evidence:** baseline, IDs, paths, gates. **Registry:** `id=PROC-002`; `mode=LOCAL_ISOLATED`; `review_status=BLOCKED`; `execution_status=EXECUTED_LOCAL`; `required_for_acceptance=true`; `environment=isolated author checkout`; `result=PASS`; `review_evidence=fresh cold review pending`; `execution_evidence=four-path doc diff and source inspection`; `limitations=no behavior code changed`. [Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Validate opt-in in-memory test sources

**Trigger / objective:** A behavior-changing model or test-source patch needs local regression validation.
**Mode / environment / permission:** `LOCAL_ISOLATED`; isolated local checkout only; Rust/Cargo and already-cached dependencies; no secrets or external services. Not authorized in a task marked static-only.
**Inputs / preconditions:** PROC-001 passes; confirm target declarations and source gates; source inspection confirms in-memory tests/no I/O; offline cache and toolchain are available. If any precondition is unknown, stop.

1. Record `rustc -Vv`, `cargo -V`, platform, source SHA, and baseline test-source inventory.
2. Run `cargo test --locked --offline --manifest-path tests/chaos/Cargo.toml --features chaos`.
3. Expect a successful result covering 16 test functions from the 11 declared targets; preserve full output and exit code.

**Failure/stop:** Offline resolution error, target mismatch, count mismatch, failure, or external effect; never retry online or with `--all-features`. **Recovery:** Revert only the isolated patch; retain build output inside that checkout. **Evidence:** command/output, toolchain, target/feature and SHA. **Registry:** `id=PROC-003`; `mode=LOCAL_ISOLATED`; `review_status=BLOCKED`; `execution_status=REVIEWED_NOT_EXECUTED`; `required_for_acceptance=false`; `environment=not run; static-only task`; `result=NOT_EXECUTED`; `review_evidence=fresh cold review pending`; `execution_evidence=[]`; `limitations=no Rust behavior changed; Cargo forbidden for this work package; require for behavior-changing source acceptance`. [Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Escalate any live or production campaign request

**Trigger / objective:** Request would inject real faults, contact external providers/services, change deployment state, incur cost, or alter production.
**Mode / environment / permission:** `AUTHORIZED_OPERATION`; no environment or permission is assumed. This package has no verified operator route or executable integration.
**Inputs / preconditions:** Explicit operation-specific authority, named target and owner, approved risk/recovery plan, change window, and applicable external runbook must be supplied independently.

1. Decline execution under this package ownership record. For an active incident only, follow the scenario-specific on-call route in [RB-CHAOS-CAMPAIGN](../../../../specs/_runbooks/RB-CHAOS-CAMPAIGN.md); it identifies operational responders, not this package's source maintainer or authority to start a campaign. 2. Require the independently authorized operator to select and review the target-specific runbook, scope, expected signals, abort condition, and recovery before acting. If no operation-specific owner/authority is supplied, remain stopped. 3. Return here only with

authorized evidence references; do not copy operations into a test-source claim.

**Expected predicate:** No live operation starts from this manual. **Stop/failure:** Missing authority, target, owner, runbook, or recovery keeps it blocked. **Recovery:** Operator-defined and separately approved; no generic rollback. **Evidence:** authorization and operator record, never credentials. **Registry:** `id=PROC-004`; `mode=AUTHORIZED_OPERATION`; `review_status=BLOCKED`; `execution_status=BLOCKED_FOR_OPERATION`; `required_for_acceptance=false`; `environment=no authorized operation environment`; `result=NOT_EXECUTED`; `review_evidence=fresh cold review and operator route pending`; `execution_evidence=[]`; `limitations=static ownership only; operation-specific authorization absent`. [Procedure index](#m02)

<a id="proc-005"></a>

### PROC-005 — Validate ownership-document structure and scope

**Trigger / objective:** Any of the four owned docs changes; confirm supplied profile-S structural checks and whitespace/path scope.
**Mode / environment / permission:** `LOCAL_ISOLATED`; author checkout, Python 3 and supplied checker; no Cargo or external service.
**Inputs / preconditions:** Set configurable `CHECKER` to the Python interpreter and `DOC_CHECKER` to the current supplied script path; do not persist an import-specific path.

1. Run the four exact `$CHECKER $DOC_CHECKER` commands in M06, one per artifact.
2. Run the M06 baseline `git diff --check` and `git diff --name-only` commands.
3. Expect four `IMPLEMENTED_CHECKS_PASS` results, clean whitespace, and exactly the four owned paths.

**Stop/failure:** Any failed check or extra path blocks closeout. **Recovery:** Correct only an owned doc and repeat all four checks. **Evidence:** command, profile, output/exit code, baseline and path list. **Registry:** `id=PROC-005`; `mode=LOCAL_ISOLATED`; `review_status=BLOCKED`; `execution_status=EXECUTED_LOCAL`; `required_for_acceptance=true`; `environment=isolated checkout; Python /usr/local/bin/python3; supplied v1.3 checker`; `result=PASS`; `review_evidence=prior cold review FIX_FIRST; fresh final-byte review pending`; `execution_evidence=four S checker outputs; diff-check exit 0; four authorized paths`; `limitations=structural checks do not certify semantics or runtime`. [Procedure index](#m02)


<a id="m04"></a>
## M04 — Test and document validation matrix

| Change type | Package / target / feature | Command/procedure and predicate | Current evidence |
|---|---|---|---|
| Manifest/source claim | `chaos-campaign`; all declarations | PROC-001; root membership, full package manifest, lockfile and source baseline reconcile. | Static author inspection only; source commit recorded. |
| Isolated model behavior | `chaos-campaign`; 11 `[[test]]` targets; `chaos` | PROC-003 exact command; success and 16 source-level tests pass. Run only for authorized behavior-changing patch. | `NOT_EXECUTED` in this static-only work. |
| One test target | `chaos-campaign`; `campaign_network_partition_failover`; `chaos` | `cargo test --locked --offline --manifest-path tests/chaos/Cargo.toml --features chaos --test campaign_network_partition_failover`; target passes. | Not run; target selection unobserved. |
| Four ownership documents | Skill, REFERENCE, BLAST_RADIUS, MAINTENANCE; profile S | PROC-005/M06 commands; four `IMPLEMENTED_CHECKS_PASS`, clean diff, exact scope. | Four S checks passed; diff-check exit 0; exact path list recorded. No Cargo run. |
| Live service / production chaos | Not a Cargo test target | PROC-004; no command supplied or run without separate approval. | `BLOCKED_FOR_OPERATION`; current execution unknown. |

Negative cases to preserve: unhealthy failover pair; zero-capacity/saturated pool; silent shadow drop and equal-length divergent contents; absent/mismatched tenant; no available provider; invalid signature; webhook timestamps at exactly 300 seconds and extreme `i64` boundaries; unknown JWKS key; complete after abort. These are source predicates, not test outcomes. Do not use `--all-features`, run ignored/live tests, contact a provider, or deploy as a default validation step.

<a id="m05"></a>
## M05 — Recovery and compatibility

| Surface | Reversible? | State / condition | Safe action | Proof limit |
|---|---|---|---|---|
| In-memory model source | Yes, at source revision level. | Test fixtures have no declared persistence or external I/O. | Recover only the task-owned patch in isolated checkout; review changed API/INV/REL. | Does not roll back real services. |
| Test assertions/target/feature | Usually source-reversible. | A feature/target change can affect other workspace selections; those selections are unknown. | Restore declaration/source gate pair or make reviewed compatibility fix. | Static check cannot prove CI matrix. |
| Docs/check output | Yes, document-only. | Structural check is not semantic or independent review. | Edit affected owned file, rerun relevant S check and baseline diff. | No source/runtime status changes. |
| Real provider/data/deployment | Unknown; this package has no such adapter. | Any external side effect would be outside package and could persist beyond a source revert. | PROC-004; actual service owner must define compensation/roll-forward. | No generic rollback or recovery proof available. |

**No package-owned persistent state:** maps, queues and vectors live in model instances only. Do not infer recovery, replay, cleanup, idempotency, N/N−1 compatibility, or rollback behavior for composition roots. If a source patch changes the model contract, source revert is not evidence that an already-running external system was restored.

<a id="m06"></a>
## M06 — Escalation, evidence, and controlled closeout

| Condition | Responsible route | Minimum evidence | Must not do |
|---|---|---|---|
| Model API or target/feature changes | Package source maintainer/assignment is UNKNOWN; requester must assign the next responsible source owner. Generic CODEOWNERS does not establish package ownership. | Changed source SHA, affected API/INV/REL/PROC IDs, checks actually run and residual risk. | Do not invent an escalation role or claim independent approval. |
| Production analogue or external integration question | Actual implementing package/service owner is UNKNOWN. During an active incident only, follow the scenario-specific on-call route in [RB-CHAOS-CAMPAIGN](../../../../specs/_runbooks/RB-CHAOS-CAMPAIGN.md); this is operational routing, not package ownership. | Its source/config evidence, operation scope, separately authorized operator and operation-specific recovery. | Do not infer ownership or campaign authority from model names/comments or runbook responder roles. |
| Canonical policy question | [OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md), route/reference only. | Exact issue and source paths requiring reconciliation. | Do not copy, redefine, or revalidate OKF. |
| Independent review | Fresh reviewer, separate from author; assignment not included here. | Four final artifact hashes, source baseline, independent per-artifact verdicts. | Do not self-approve or count linter output as cold review. |

For document checks, obtain `CHECKER` (Python 3 interpreter) and `DOC_CHECKER` (the currently supplied checker script path) from the caller/import; never persist a temporary import path as a repository standard:

```sh
"$CHECKER" "$DOC_CHECKER" --kind skill --profile S --root . .claude/skills/own-chaos-campaign/SKILL.md
"$CHECKER" "$DOC_CHECKER" --kind reference --profile S --root . docs/ownership/crates/chaos-campaign/REFERENCE.md
"$CHECKER" "$DOC_CHECKER" --kind blast_radius --profile S --root . docs/ownership/crates/chaos-campaign/BLAST_RADIUS.md
"$CHECKER" "$DOC_CHECKER" --kind maintenance --profile S --root . docs/ownership/crates/chaos-campaign/MAINTENANCE.md
git diff --check cb94e251c0f17382565bf863f517945cbb2a84d6 HEAD
git diff --name-only cb94e251c0f17382565bf863f517945cbb2a84d6 HEAD
```

Expected: each supplied S checker reports `IMPLEMENTED_CHECKS_PASS`; diff-check exits 0; name-only output is exactly the four owned paths. Any other path blocks closeout. Record exact outputs and source SHA, then request fresh review of final bytes. A check pass is not semantic approval, Cargo/test execution, CI selection, current campaign activity, runtime, or deployment evidence.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Start](#m01)
