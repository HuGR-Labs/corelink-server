---
schema: corelink-ownership/1.1
document: maintenance
package: e2e-pilot-onboarding
manifest: tests/e2e-pilot-onboarding/Cargo.toml
source_commit: cb94e251c0f17382565bf863f517945cbb2a84d6
profile: S
state: draft
evidence_set: w014-e2e-pilot-onboarding-source-static-20260921
---

# e2e-pilot-onboarding — maintenance

This manual separates source inspection, future isolated Rust behavior checks, and documentary artifact validation. It does not authorize Cargo/build/test/fuzz execution in a static-only ownership task, nor network, GitHub, providers, deployment, production, customer-data, or irreversible operations. Signup implementation routes to its production owner; canonical policy routes only to the designated OKF concept in [R02](REFERENCE.md#r02).

Package implementation owner is `UNASSIGNED`; request assignment from a repository
administrator and stop owner-authorized changes until recorded. `.github/CODEOWNERS`
only requests review from `@gmhelmold`; it grants no implementation/operations
authority or proof of reviewer independence.

[Preparation](#m01) · [Select](#m02) · [Procedures](#m03) · [Tests](#m04) · [Recovery](#m05) · [Escalation](#m06).

<a id="m01"></a>
## M01 — Safe preparation

**Mode:** `READ_ONLY`. **Checkout/toolchain/targets:** confirm repository checkout
and baseline `cb94e251c0f17382565bf863f517945cbb2a84d6`; package manifest is
`tests/e2e-pilot-onboarding/Cargo.toml`. Five test target paths are declared, but
resolved targets/features/toolchain were not observed. **Prerequisite:** task scope
identifies one of the exact package/API/target/artifact surfaces in R01/R03.

Use [PROC-001](#proc-001) before source conclusions. Stop if HEAD/source differs
from the requested snapshot, the changed-path population is broader, or a claim
needs a resolved graph or execution. No secrets, fixture data, or provider setup
are needed. `target/` is not read or changed for a documentary task.

<a id="m02"></a>
## M02 — Procedure selection and relation routing

| Situation | Procedure | Mode | Allowed effect | Extra authority? |
|---|---|---|---|---|
| Confirm package, source, API, or local state | [PROC-001](#proc-001) | `READ_ONLY` | Inspect pinned text | No; stop on baseline drift |
| Change Rust behavior and validate a declared scenario | [PROC-002](#proc-002) | `LOCAL_ISOLATED` | Future isolated test execution only | Separate task authorization; not this static-only task |
| Assess a proposed change to a declared dependency | [PROC-004](#proc-004) | `READ_ONLY` | Static declaration/use/impact review only | Owner assignment first; resolver/lock/build needs separate authority |
| Edit ownership docs and check structure/scope | [PROC-003](#proc-003) | `READ_ONLY` | Read-only checker/diff | No external authority; fresh review still required |

Route target and method changes to all affected atomic relations, not a generic
scenario arrow: test 01 → REL-013 + signup REL-004; test 02 → REL-014 + signup,
upload, and read REL-004–006; test 03 → REL-015 + signup/upload/export/verify
REL-004/005/007/008/039.

Test 04 → REL-016 + signup/upload/DSR REL-004/005/009/010; test 05 → REL-017 +
signup/upload/DSR/cancel/offboard REL-004/005/009–012. An export change uses
output relation REL-007, marker-write REL-039, and target REL-015.

A method/state change uses its API/INV and every affected relation in B03–B05,
including all scenarios that call the method.
Do not route every issue to B03 or B04–B06 indiscriminately.

<a id="m03"></a>
## M03 — Procedures

<a id="proc-001"></a>
### PROC-001 — Pin and inspect package source

**id:** `PROC-001`. **objective/trigger:** source, target, API, or state claim; establish matching manifest/baseline and exact contract. **mode:** `READ_ONLY`. **environment requirement:** local checkout; no network/Cargo. **inputs:** assigned commit, manifest, changed paths.
**preconditions:** snapshot equals R01 baseline; target/source identified.

1. Read manifest, `lib.rs`, affected `harness.rs`/`harness_lifecycle.rs`, and called target source; map exact API/INV and RELs.
2. Check each claimed negative branch and whether the assertion is source text or executed evidence.

**failures/stop:** baseline mismatch, unresolved consumer claim, contradictory order, or runtime need; record UNKNOWN and owner/evidence.

**recovery:** return to assigned snapshot; no source mutation in this procedure.

**review_status:** `BLOCKED`; fresh independent review of final bytes is pending.

**execution_status:** `EXECUTED_LOCAL`; **required_for_acceptance:** `false`.

**environment:** local checkout. **result:** `PASS` for source inspection only.

**review_evidence:** prior FIX_FIRST finding, not a review of final bytes.

**execution_evidence:** source paths and predicates cited in R03–R08.

**limitations:** no resolution, test, provider, or runtime evidence.

[Procedure index](#m02)

<a id="proc-002"></a>

### PROC-002 — Validate a changed local scenario

**id:** `PROC-002`. **objective/trigger:** a separately authorized Rust behavior change needs a declared target predicate. **mode:** `LOCAL_ISOLATED`. **environment requirement:** disposable local checkout and caller-validated empty target directory; offline toolchain/cache only, no network/provider. **inputs:** exactly one affected manifest target (lines 21–37); no package feature is declared.
**preconditions:** package owner assigned; separate task explicitly permits Cargo; baseline recorded; offline toolchain/dependencies exist. Forbidden in the current static-only task.

1. Verify `ISOLATED_TARGET` names a new task-owned empty directory; use the exact target-specific command in M04 with default features and no other package environment.
2. Compare output/state assertions and negative cases with R04/R05 and REL-013–017; preserve command, environment, and output.

**failures/stop:** missing offline cache/toolchain, baseline drift, failed assertion, or live/provider request stops; do not download or widen features.

**recovery:** preserve logs; revert only this task's code or roll forward a reviewed fix; a test cannot undo external effects.

**review_status:** `BLOCKED`. **execution_status:** `REVIEWED_NOT_EXECUTED`. **required_for_acceptance:** `false`.

**environment:** not executed; isolated target not created. **result:** `NOT_EXECUTED`.
**review_evidence:** earlier FIX_FIRST finding, not review of final bytes.
**execution_evidence:** none. **limitations:** no current run; future tests establish local predicates only.

[Procedure index](#m02)

<a id="proc-003"></a>

### PROC-003 — Validate ownership documents and exact scope

**id:** `PROC-003`. **objective/trigger:** check all four ownership docs; **mode:** `READ_ONLY`. **environment requirement:** local checkout, `python3`, no network/Cargo. **inputs:** baseline `cb94e251c0f17382565bf863f517945cbb2a84d6`, configurable `CHECKER` path.

**trust/pin:** this task's supplied copy is `/tmp/corelink-ownership-v13-source/corelink-ownership-v1.3/tools/check_docs.py`, CO-1 v1.3 SHA-256 `84d093bc9e95b79644fac56ca35e34b4e0ced9cd2b24ec1704485343236585b8` (`python3` 3.14.5). This `/tmp` delivery path is ephemeral; use configurable `CHECKER`, not a permanent absolute path.
**preconditions:** set `CHECKER`; require `test -n "${CHECKER:-}" && test -f "$CHECKER" && test -r "$CHECKER"`; verify `test "$(sha256sum "$CHECKER" | cut -d ' ' -f1)" = "84d093bc9e95b79644fac56ca35e34b4e0ced9cd2b24ec1704485343236585b8"`. Stop if absent, unreadable, or hash differs.

1. Run `python3 "$CHECKER" <path> --kind <kind> --profile S --root .` for `.claude/skills/own-e2e-pilot-onboarding/SKILL.md|skill`, `docs/ownership/crates/e2e-pilot-onboarding/REFERENCE.md|reference`, `docs/ownership/crates/e2e-pilot-onboarding/BLAST_RADIUS.md|blast_radius`, and `docs/ownership/crates/e2e-pilot-onboarding/MAINTENANCE.md|maintenance`.
2. Run `git diff --check "$BASELINE" HEAD`; require `git diff --name-only "$BASELINE" HEAD` to equal those four paths.

**failures/stop:** bad checker/hash, failed check, broken link, or scope/whitespace drift. **recovery:** preserve output; patch only assigned docs; rerun all four.

**review_status:** `BLOCKED` pending fresh independent review. **execution_status:** `EXECUTED_LOCAL`. **required_for_acceptance:** `true`.
**environment:** local checkout, Python 3.14.5, pinned checker. **result:** `PASS`.
**review_evidence:** prior FIX_FIRST finding, not final-byte review. **execution_evidence:** four S checks, each exit 0 / `IMPLEMENTED_CHECKS_PASS` / no errors; exact four-path baseline diff and clean `git diff --check` in handoff.
**limitations:** structural output is not semantic approval, runtime evidence, or independent review.

[Procedure index](#m02)

<a id="proc-004"></a>

### PROC-004 — Assess a declared dependency change

**id:** `PROC-004`. **objective/trigger:** proposed manifest change to `blake3`, `hex`, `serde`, `serde_json`, or `thiserror`. **mode:** `READ_ONLY`. **environment requirement:** local pinned checkout; no Cargo, network, or lockfile rewrite. **inputs:** proposal, manifest/source use, API/INV/REL consumers.
**preconditions:** package owner is assigned and proposal identifies the exact alias, version/source, features, and compatibility claim. Otherwise stop and request assignment from repository administrator; owner is `UNASSIGNED` until recorded.

1. Trace the exact `[dependencies]` declaration and uses in `harness.rs`; map affected API/invariants, target relations, and documented reverse consumers.
2. Record whether the proposal changes public output, hash bytes, serialization, or build declarations; resolution and indirect consumers remain UNKNOWN without authorized evidence.

**failures/stop:** missing owner/proposal/source facts, need to resolve versions, modify Cargo.lock, fetch, build, test, or assert compatibility; request a separately scoped authorized task.
**recovery:** no dependency file is edited by this read-only procedure; preserve analysis and reopen only after authority/scope is recorded.
**review_status:** `BLOCKED`. **execution_status:** `REVIEWED_NOT_EXECUTED`. **required_for_acceptance:** `false`.
**environment:** not executed. **result:** `NOT_EXECUTED`.
**review_evidence:** prior FIX_FIRST finding, not review of final bytes.
**execution_evidence:** none. **limitations:** no edit, resolver, build, lockfile, or dependency compatibility evidence.

[Procedure index](#m02)


<a id="m04"></a>
## M04 — Target-specific validation matrix

Every Rust command below is a future PROC-002 example only: current static-only
work forbids running it.

Package identity is `e2e-pilot-onboarding`; manifest is
`tests/e2e-pilot-onboarding/Cargo.toml`; one library and five `[[test]]` targets are
declared. No `[features]` table or package env reads are established; commands use
the default feature set, no `--features`, no `RUSTFLAGS`, and no external env.
For each authorized local run, set `CARGO_TARGET_DIR="$ISOLATED_TARGET"` to a new,
task-owned empty directory after validation. No network/provider access.

| Change/scenario | Exact target and command (offline) | Static predicate to compare | Environment/evidence boundary |
|---|---|---|---|
| Signup (`test_01_tenant_signup`) | `--test test_01_tenant_signup`; `cargo test --offline --manifest-path tests/e2e-pilot-onboarding/Cargo.toml --test test_01_tenant_signup` | Empty pre-state; success IDs/Active/four-row audit chain; duplicate returns AlreadyProvisioned without new rows. Test asserts final state/event sequence, not operation order. | `CARGO_TARGET_DIR` only; no package env; local memory. |
| Upload/read (`test_02_first_cas_upload`) | `--test test_02_first_cas_upload`; `cargo test --offline --manifest-path tests/e2e-pilot-onboarding/Cargo.toml --test test_02_first_cas_upload` | 100 fixture digests/count/round trips; receipt prefix text; no-signup rejection/no audit; cross-slug read rejected. Insert-before-audit source discrepancy remains. | `CARGO_TARGET_DIR` only; local digest-key map, no external CAS. |
| Export/verify (`test_03_audit_export_roundtrip`) | `--test test_03_audit_export_roundtrip`; `cargo test --offline --manifest-path tests/e2e-pilot-onboarding/Cargo.toml --test test_03_audit_export_roundtrip` | Fixed half-open 24h selection, body digest/count/link round-trip; tampered body and invalid window rejected. Verifier does not recompute row hashes. | `CARGO_TARGET_DIR` only; local NDJSON; no export delivery. |
| DSR (`test_04_dsr_erasure`) | `--test test_04_dsr_erasure`; `cargo test --offline --manifest-path tests/e2e-pilot-onboarding/Cargo.toml --test test_04_dsr_erasure` | Missing-signup/duplicate/early negatives; seven-day deadline; success drains CAS and leaves one genesis-anchored completion row. | `CARGO_TARGET_DIR` only; fixed local times/maps, not a DSR operation. |
| Offboarding (`test_05_tenant_offboarding`) | `--test test_05_tenant_offboarding`; `cargo test --offline --manifest-path tests/e2e-pilot-onboarding/Cargo.toml --test test_05_tenant_offboarding` | Missing-cancel/early negatives; 30-day grace; success has zero residuals; post-DSR path remains clean. | `CARGO_TARGET_DIR` only; local maps, no provider cancellation/deletion. |
| Fixture/shared harness (`--lib`) | `--lib`; `cargo test --offline --manifest-path tests/e2e-pilot-onboarding/Cargo.toml --lib` | 3 source-local tests: tenant determinism, 4-KiB payload determinism, four signup audit rows. Select when changed code is exercised there. | `CARGO_TARGET_DIR` only; compile/test output not observed here. |
| Root/module/API or multi-target source | Each affected target above plus `--lib` if its source test applies; run only the exact selected commands, not workspace-wide. | Update API/INV and every direct/transitive relation per M02; preserve negative assertions and source-vs-operation discrepancy. | Resolution/consumer completeness still UNKNOWN. |
| One of five direct dependency declarations | No Cargo command in PROC-004; static declaration/use/API/REL map only. | Trace exact alias (`blake3`, `hex`, `serde`, `serde_json`, `thiserror`); assess public outputs/hash/serialization. | No lockfile edit, resolution, fetch, build, or compatibility claim. |
| Four ownership artifacts | PROC-003: four S checker commands, baseline name list, `git diff --check`. | Exact four assigned paths; structural output only. | Pinned checker hash; DOCUMENTARY, fresh review separate. |
| Static verifiers/workflows/docs | B-269/B-126 REL-018–021; workflow REL-022–026 | Inspect named source path/predicate; docs are not executable behavior. | Known source consumers; not run here. |

Do not run `--all-features` (no feature table), live/ignored suites, fuzz, deploy,
CI, workspace-wide test selection, or any row during this static-only assignment.

<a id="m05"></a>
## M05 — Compatibility and recovery

| Surface | Reversible? | Existing state/condition | Safe action | Recovery proof |
|---|---|---|---|---|
| Four documentation artifacts | Yes, source-control bytes only | no package runtime state | revert only owned paths in the authorized branch; re-run PROC-003 | exact four-path diff and checks |
| Public Rust names/types/errors | Source-compatible change may break callers | five known test importers; external inverses unknown | preserve root re-exports and `#[non_exhaustive]` contracts; enumerate consumers | future compile/tests under PROC-002; resolution still needed |
| Local harness maps and timestamps | Instance-local and volatile | no durable data/schema in checked source | rebuild a new harness instance in test scope | assert local initial/final state; not provider recovery |
| Test outputs/target artifacts | Local generated files if future tests run | isolated target directory only | retain evidence; remove only an exact directory created by that task after path validation | no deletion or cleanup in current static task |
| Provider, customer, or deployed state | Unknown/not owned | no provider relation established | no rollback/roll-forward procedure is authorized here | escalate to verified operator; require external evidence |

No DB migration, wire protocol, deploy artifact, or package-owned durable state is
established. `git revert` recovers authored bytes, not results or effects of an
external command. Source/API compatibility is distinct from test success; N/N-1
provider compatibility is not applicable absent a proven provider boundary.

<a id="m06"></a>
## M06 — Escalation and maintenance record

| Condition | Verified route | Minimum evidence | Prohibited action |
|---|---|---|---|
| Package source/API/target mismatch | Owner `UNASSIGNED`; request assignment from repository administrator. Stop owner-authorized changes until assignment is recorded. | baseline, exact path/symbol, API/INV/REL and failing predicate | infer ownership from package name or CODEOWNERS reviewer |
| Provider, customer-data, deployment, or production operation | No authority is established by this package; stop and request the separately assigned operator/owner | exact system/action, scope, operator authority, rollback/evidence plan | act through this in-memory harness or infer authority from reviewer route |
| Production signup implementation | [own-corelink-signup skill](../../../../.claude/skills/own-corelink-signup/SKILL.md) routes implementation ownership | production source/surface and its owner authority | use this harness as proof or redefine OKF policy |
| Canonical onboarding policy | designated [OKF concept](../../../../docs/knowledge/launch/signup-onboarding.md); [okf-context skill](../../../../.claude/skills/okf-context/SKILL.md) | canonical concept/decision and affected package contract | copy policy into this package reference |
| CI/verifier ownership or contradictory docs | Owner `UNASSIGNED`; request assignment through repository administrator | REL ID, source lines, contradiction, no asserted execution | treat prose or CODEOWNERS match as implementation authority |
| Final artifact review | `.github/CODEOWNERS` catch-all `@gmhelmold` only requests review; ask repository administrator to arrange an independent, authorized reviewer. Independence/authority remain unverified until evidenced. | hashes/baseline, all four artifacts, sources/check output, prior finding closure | self-approve or infer independence from a requested review |

After the task, record baseline, exact paths, API/INV/REL/PROC IDs, execution state
per procedure, outputs, remaining unknowns, and reviewer status. Preserve results;
clean only task-created temporaries after validating their exact path. A draft,
checker pass, CODEOWNERS match, or author handoff is not cold approval.

[Reference](REFERENCE.md#r01) · [Blast radius](BLAST_RADIUS.md#b01) · [Ownership skill](../../../../.claude/skills/own-e2e-pilot-onboarding/SKILL.md#s01)
