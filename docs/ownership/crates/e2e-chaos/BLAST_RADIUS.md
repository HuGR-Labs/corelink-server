---
schema: corelink-ownership/1.1
document: blast_radius
package: e2e-chaos
manifest: tests/e2e-chaos/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: e2e-chaos-static-source-20260921
---

# e2e-chaos — blast radius

Static declaration, source-call, and workflow-configuration relationships only. No resolved target selection, command execution, provider effect, or production reachability is asserted. The [verified OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md) is the designated routing reference.

[Boundary](#b01) · [Census/method](#b02) · [Relations](#b03) · [Propagation](#b04) · [Change matrix](#b05) · [Coverage/unknowns](#b06)

<a id="b01"></a>
## B01 — Boundary and summary

Manifest declares a library, twelve integration targets, a scheduler normal dependency, and a proptest dev dependency. The library constructs experiments and local telemetry/lock fakes; some targets call scheduler APIs. Workspace CI names commands; no run established. External wiring/reverse consumers unknown. [Reference](REFERENCE.md#r06) lists exact target paths.

**Declared package selection reviewed:** manifest/tracked configuration only; no Cargo selection or runtime artifact. **Known boundary:** manifest, source/tests, root workspace declaration, matched workflows. **Not observed:** workflow events, resolved graph, caller reachability, providers, or production state.

<a id="b02"></a>
## B02 — Discovery method and populations

| Population | Static source/method | Found/reviewed | Limit |
|---|---|---|---|
| Manifest package/dependencies/targets | Read `tests/e2e-chaos/Cargo.toml:1-68`, root `Cargo.toml:289`; traced paths | 1 package; 1 normal dep; 1 dev dep; 12 targets | Declarations, not resolved/executed |
| Reverse Cargo consumers | Literal `e2e-chaos` / `tests/e2e-chaos` search in `Cargo.toml` | No external match; workspace member | Untracked/out-of-repository consumers and runtime wiring not found |
| Internal source calls and imports | Search package `src/`, `tests/`; inspect callers/unit tests | Library, 12 targets, four unit tests | Source is not execution evidence |
| Direct calls to package-local public APIs | Search target/unit sources for API-001–017; classify API/consumer | Eight scenario, three adversarial, one property targets, four unit tests; REL-028–048 | Authored calls; selection/execution unknown |
| Workspace CI configuration | Search `.github/workflows` for Cargo commands; inspect matches | `cas_foundation`, `workspace-lint`, `codeql`, `rustfmt`, `nightly` | Config only; filters/events/targets/runs uncertified |
| Per-PR mutation workflow | Inspect mutation-pr filter and `cargo mutants --in-diff pr.diff` | One config relation (REL-027); Rust/Cargo changes trigger | Mutation execution/result unknown |
| Tracked D1 chaos schema | Inspect migration and search package refs | One migration defines `chaos_runs`, `chaos_results`, FK, audit comments; excluded from package RELs | Deployment/runtime/schema consumers unknown |
| Runtime/data/external contracts | Inspect manifest, library, tests, dependencies | No provider/network dep; local fakes present | Does not disprove deployment/external consumers |

R06 lists twelve targets and four separate unit tests. Welcome/template examples are prose-only exclusions (`.github/workflows/welcome-first-pr.yml:98-101`; `.github/workflows/_TEMPLATE.yml.md:213`). Build graphs, CI history, deployment manifests, and out-of-repository consumers were not collected.

<a id="b03"></a>
## B03 — Atomic relationship index

| ID | Type / producer → consumer endpoints | Surface and activation | Contract owner |
|---|---|---|---|
| [REL-001](#rel-001) | dependency: scheduler → e2e-chaos | normal manifest dependency; selected package build | scheduler package |
| [REL-002](#rel-002) | dependency: proptest → property target | dev dependency; property target build | external crate owner unknown |
| [REL-003](#rel-003) | runtime-call: catalog API → library constructor | mapped-ID catalog hit returns clone | scheduler package |
| [REL-004](#rel-004) | test: scheduler types → scenario targets | eight declared scenario sources | scheduler package |
| [REL-005](#rel-005) | test: scheduler runner → staging-only target | production-target snapshots | scheduler package |
| [REL-006](#rel-006) | test: scheduler seed API → replay property | same-input seed equality | scheduler package |
| [REL-007](#rel-007) | runtime-call: runner → fake audit callback | safe-mode abort branch | scheduler callback trait |
| [REL-008](#rel-008) | runtime-call: runner → fake Started audit | non-abort start branch | scheduler callback trait |
| [REL-009](#rel-009) | test: package callers → local lock | adversarial target and library unit test | local package |
| [REL-010](#rel-010) | config: foundation workflow → fmt command | workspace formatting config | repository workflow owner unknown |
| [REL-011](#rel-011) | config: workspace-lint workflow → clippy command | all-target clippy command | repository workflow owner unknown |
| [REL-012](#rel-012) | config: CodeQL workflow → build command | locked all-target build command | repository workflow owner unknown |
| [REL-013](#rel-013) | config: rustfmt workflow → format command | fmt-all check command | repository workflow owner unknown |
| [REL-014](#rel-014) | config: nightly workflow → mutation command | workspace mutation command | repository workflow owner unknown |
| [REL-015](#rel-015) | runtime-call: scheduler runner API → wrapper | `run_clean_staging` call | scheduler package |
| [REL-016](#rel-016) | test: scheduler runner → SEV-1 defer target | active-SEV-1 snapshot | scheduler package |
| [REL-017](#rel-017) | test: scheduler seed API → distinct-ID property | different run IDs, same experiment | scheduler package |
| [REL-018](#rel-018) | telemetry: runner → fake pre-capture | non-abort pre-state callback | scheduler callback trait |
| [REL-019](#rel-019) | telemetry: runner → fake post-capture | non-abort post-state callback | scheduler callback trait |
| [REL-020](#rel-020) | telemetry: runner → fake SLO measurement | non-abort impact callback | scheduler callback trait |
| [REL-021](#rel-021) | runtime-call: runner → fake Completed audit | non-abort completion branch | scheduler callback trait |
| [REL-022](#rel-022) | config: foundation workflow → clippy command | workspace all-target clippy config | repository workflow owner unknown |
| [REL-023](#rel-023) | config: foundation workflow → test command | workspace all-target test config | repository workflow owner unknown |
| [REL-024](#rel-024) | config: foundation workflow → doc command | workspace documentation config | repository workflow owner unknown |
| [REL-025](#rel-025) | test: scheduler runner → SEV-1 resume target | cleared-SEV-1 snapshot | scheduler package |
| [REL-026](#rel-026) | runtime-call: package constructor → local fallback arm | catalog miss | local package |
| [REL-027](#rel-027) | config: mutation-pr workflow → cargo-mutants changed-line step | Rust-diff path filter and `--in-diff` | repository workflow owner unknown |
| [REL-028](#rel-028) | test: eight scenario targets → local constructor | `make_experiment` call sites | local package |
| [REL-029](#rel-029) | test: adversarial targets → local constructor | `make_experiment` call sites | local package |
| [REL-030](#rel-030) | test: property target → local constructor | `make_experiment` call site | local package |
| [REL-031](#rel-031) | test: unit test → local constructor | charter unit-test call | local package |
| [REL-032](#rel-032) | test: eight scenario targets → local runner wrapper | `run_clean_staging` calls | local package |
| [REL-033](#rel-033) | property target → local runner wrapper | two replay calls | local package |
| [REL-034](#rel-034) | unit test → local runner wrapper | breach-counter unit-test call | local package |
| [REL-035](#rel-035) | test: eight scenario targets → rollback helper | `assert_rollback_within_5min` calls | local package |
| [REL-036](#rel-036) | unit test → rollback helper | charter unit-test loop | local package |
| [REL-037](#rel-037) | test: eight scenario targets → audit helper | `assert_canonical_audit_sequence` calls | local package |
| [REL-038](#rel-038) | test: adversarial targets → audit helper | `assert_canonical_audit_sequence` calls | local package |
| [REL-039](#rel-039) | test: eight scenario targets → telemetry getter | `audit_events` calls | local package |
| [REL-040](#rel-040) | test: adversarial targets → telemetry getter | `audit_events` calls | local package |
| [REL-041](#rel-041) | test: eight scenario targets → telemetry getter | `slo_violation_total` calls | local package |
| [REL-042](#rel-042) | test: adversarial targets → telemetry getter | `slo_violation_total` calls | local package |
| [REL-043](#rel-043) | unit test → telemetry getter | breach-counter assertion | local package |
| [REL-044](#rel-044) | adversarial targets → telemetry constructor | direct `ChaosTestTelemetry::new` calls | local package |
| [REL-045](#rel-045) | local wrapper → telemetry constructor | `run_clean_staging` initialization | local package |
| [REL-046](#rel-046) | property target → drill mapper | direct `experiment_id` calls | local package |
| [REL-047](#rel-047) | unit test → drill mapper | distinct-ID unit test | local package |
| [REL-048](#rel-048) | test: `OpsEventKind::label` → adversarial target | `held_by.label()` asserts `dr_drill` | local package |

Every B03 arrow names producer → consumer relationship endpoints; it is not a Cargo dependency arrow. Each fiche separately records Cargo dependency direction (consumer → provider, or none), data-flow direction, and impact direction.

<a id="rel-001"></a>
### REL-001 — Normal scheduler dependency
**Identity:** `repo:1232040291:boundary:e2e-chaos-scheduler-normal-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** N/A; this is a declaration edge. **Impact:** provider contract changes may affect consumer compilation or behavior.
**Surface:** normal path dependency in `tests/e2e-chaos/Cargo.toml:16-17`.
**Activation:** only a selected/resolved package build; selection unknown.

**Contract:** declaration, not consumed API; API calls are REL-003–005.
**State / effects:** no manifest-owned runtime state.
**Failure / propagation:** missing/incompatible selected package can block compilation.
**Containment:** package build boundary; artifacts/reachability unknown.
**Validation:** source inspection only; no resolution/build.
**Coordination / sources:** scheduler contract owner; human route unknown; manifest above. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Property-test development dependency
**Identity:** `repo:1232040291:boundary:e2e-chaos-proptest-dev-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → proptest. **Data flow (producer → consumer):** N/A; this is a dev-dependency declaration. **Impact:** an incompatible crate can affect selected test-target compilation.
**Surface:** dev dependency at `tests/e2e-chaos/Cargo.toml:19-20`; consumer target is `prop_seed_replay_identical`.
**Activation:** property target selected for build/run; neither is known.

**Contract:** dev-only declaration, separate from scheduler `derive_seed` API (REL-006).
**State / effects:** no package runtime state.
**Failure / propagation:** missing/incompatible crate can block selected target compilation.
**Containment:** dev target, not production dependency.
**Validation:** manifest and target source inspection; no build/run.
**Coordination / sources:** external crate owner unknown; manifest `:19-20,66-68`; target `tests/e2e-chaos/tests/prop_seed_replay_identical.rs:25,36-54`. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Catalog-hit clone
**Identity:** `repo:1232040291:boundary:e2e-chaos-scheduler-catalog-hit-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** mapped drill ID → matching catalog record → local constructor clone. **Impact:** catalog fields affect returned record.
**Surface:** `canonical_catalog()` in `make_experiment`; import at `tests/e2e-chaos/src/lib.rs:52-55,113-120`.
**Activation:** mapped ID matches catalog entry at `:113-120`.

**Contract:** matching record is cloned and returned; misses are REL-026.
**State / effects:** returned upstream record; no package persistence.
**Failure / propagation:** API drift breaks compilation; changed upstream record changes output.
**Containment:** constructor return boundary; catalog contents/hits unknown.
**Validation:** trace match branch; no catalog execution.
**Coordination / sources:** scheduler catalog owner; `tests/e2e-chaos/src/lib.rs:52-55,113-203`. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Scheduler types consumed by scenario targets
**Identity:** `repo:1232040291:boundary:e2e-chaos-scheduler-scenario-types-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** scheduler types → scenario target imports/assertions. **Impact:** type/API changes may break selected target compilation or assertions.
**Surface:** direct `ChaosAuditEvent`, `ChaosKind`, `ChaosOutcome` imports in `chaos_network_partition_recovers`, `chaos_cpu_pressure_sustained`, `chaos_memory_pressure_oomk_avoided`, `chaos_r2_disk_fill_quarantine`, `chaos_latency_injection_p99_bounded`, `chaos_dns_failure_dual_resolver_failover`, `chaos_clerk_outage_grace_period`, `chaos_kv_d1_cold_start_below_sla`.
**Activation:** each target's declaration/path in manifest R06; execution unknown.

**Contract:** public types are pattern-matched/asserted by target source; details remain scheduler-owned.
**State / effects:** test-local returned values only; durable effects unknown.
**Failure / propagation:** incompatible variants/types or expectations fail selected target.
**Containment:** test target source; no production inference.
**Validation:** inspect imports/assertions; no target run.
**Coordination / sources:** scheduler owner; paths `tests/e2e-chaos/tests/chaos_*.rs`; manifest `tests/e2e-chaos/Cargo.toml:22-61`. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Production-target staging-only rejection
**Identity:** `repo:1232040291:boundary:e2e-chaos-scheduler-staging-reject-test-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** local Production snapshot → runner → returned run/fake audit event. **Impact:** runner branch changes may falsify this target's predicate.
**Surface:** direct `run_experiment` calls in `adversarial_chaos_staging_only`.
**Activation:** supplied Production target snapshots at `tests/e2e-chaos/tests/adversarial_chaos_staging_only.rs:41-49,75-83`.

**Contract:** source asserts `prod_target_violation`; first case also asserts returned Production target and local Aborted event.
**State / effects:** local fake state only; external effects not established.
**Failure / propagation:** runner priority/abort drift changes assertions.
**Containment:** declared test source; not a production drill.
**Validation:** inspect inputs/assertions; target not run.
**Coordination / sources:** scheduler runner owner; test path above and `crates/corelink-chaos-scheduler/src/runner.rs:137-147`. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Same-input replay seed property
**Identity:** `repo:1232040291:boundary:e2e-chaos-scheduler-derive-seed-test-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** identical run/experiment IDs → `derive_seed` calls → property assertions. **Impact:** API or seed-algorithm changes may falsify the replay predicate.
**Surface:** `derive_seed` expected-seed call in `tests/e2e-chaos/tests/prop_seed_replay_identical.rs:22-24,70-78`; definition `crates/corelink-chaos-scheduler/src/runner.rs:84`.
**Activation:** `prop_seed_replay_identical`; target run/cases/result unknown.

**Contract:** source asserts identical-input seed equality; scheduler defines algorithm.
**State / effects:** seed is returned test data; no persistent state.
**Failure / propagation:** changed signature/behavior can falsify the equality assertions.
**Containment:** this property function; no result claimed.
**Validation:** inspect source only; property test not run.
**Coordination / sources:** scheduler seed owner; test `:70-78`, definition `crates/corelink-chaos-scheduler/src/runner.rs:84`. [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Abort audit callback
**Identity:** `repo:1232040291:boundary:e2e-chaos-runner-abort-audit-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** runner `Aborted` event → local fake event vector. **Impact:** abort-path changes may falsify the local audit assertion.
**Surface:** `run_experiment` emits `Aborted` before return; fake `emit_audit` appends event.
**Activation:** scheduler abort branch at `crates/corelink-chaos-scheduler/src/runner.rs:137-147`.

**Contract:** event is appended; fake does not count capture callback invocations.
**State / effects:** instance-local event vector; no sink/persistence.
**Failure / propagation:** missing/different event changes test-source predicate.
**Containment:** local fake/test; not a production audit result.
**Validation:** staging test observes returned target, `Aborted`, event list and zero counter (`tests/e2e-chaos/tests/adversarial_chaos_staging_only.rs:31-60`); not run.
**Coordination / sources:** scheduler callback owner; runner `:137-147`, fake `tests/e2e-chaos/src/lib.rs:248-256`. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Started audit callback
**Identity:** `repo:1232040291:boundary:e2e-chaos-runner-started-audit-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** runner `Started` event → local fake event vector. **Impact:** call/order changes may falsify the local audit-sequence assertion.
**Surface:** `emit_audit(pending, Started)` and fake event append.
**Activation:** runner's non-abort path at `crates/corelink-chaos-scheduler/src/runner.rs:158-168`.

**Contract:** appends `Started`; local fake does not increment the counter for this event.
**State / effects:** instance-local `RefCell<Vec<_>>`; no durable sink.
**Failure / propagation:** omitted/reordered event changes authored sequence predicate.
**Containment:** local test seam only; not audit delivery.
**Validation:** runner/fake source trace; no run.
**Coordination / sources:** scheduler trait owner; runner `:158-168`, fake `tests/e2e-chaos/src/lib.rs:248-256`. [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Local lock test callers
**Identity:** `repo:1232040291:boundary:e2e-chaos-local-lock-test-001`.
**Cargo dependency (consumer → provider):** none. **Data flow (producer → consumer):** local test caller → local lock instance. **Impact:** lock behavior changes may falsify local assertions.
**Surface:** `new`, `try_acquire`, guard release/drop; adversarial target also calls `held_by.label()` at `:35-37` (API-009; REL-048); and `ops_lock_basic_acquire_release` unit test.
**Activation:** source calls `tests/e2e-chaos/tests/adversarial_ops_exclusivity.rs:27-61`; unit calls `tests/e2e-chaos/src/lib.rs:498-512`.

**Contract:** one in-memory `Mutex<Option<OpsEventKind>>` per lock value.
**State / effects:** process-local, not region-keyed or durable.
**Failure / propagation:** overlap/reacquisition assertions change; no D1/cross-process implication.
**Containment:** package-local model only.
**Validation:** inspect both test sources; neither run.
**Coordination / sources:** local package owner; source paths above and `tests/e2e-chaos/src/lib.rs:371-447`. [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Foundation workspace formatting configuration
**Identity:** `repo:1232040291:boundary:e2e-chaos-cas-foundation-fmt-config-001`.
**Cargo dependency (consumer → provider):** none. **Configuration direction (producer → consumer):** workflow configuration → workspace format command. **Data flow:** none. **Impact:** formatting drift may fail the configured step if selected/executed.
**Surface:** `cargo fmt --all -- --check` in `.github/workflows/cas_foundation.yml:158`.
**Activation:** workflow/job conditions and event not evaluated here.

**Contract:** configured format check only; no package-specific result.
**State / effects:** CI job state external and unobserved.
**Failure / propagation:** if run, formatting drift in selected workspace surface may fail the step.
**Containment:** workflow boundary; run/result unknown.
**Validation:** YAML inspection only; no GitHub action.
**Coordination / sources:** workflow owner unknown; path above; package membership `Cargo.toml:289`. [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Workspace lint configuration
**Identity:** `repo:1232040291:boundary:e2e-chaos-workspace-lint-config-001`.
**Cargo dependency (consumer → provider):** none. **Configuration direction (producer → consumer):** workflow configuration → workspace clippy command. **Data flow:** none. **Impact:** diagnostics in selected members may affect job status if run.
**Surface:** `cargo clippy --workspace --all-targets --message-format=short -- -D warnings`.
**Activation:** workflow trigger/job conditions not evaluated.

**Contract:** command configuration in `.github/workflows/workspace-lint.yml:73`; not a result.
**State / effects:** CI status unobserved.
**Failure / propagation:** selected code warnings/errors may fail the job.
**Containment:** workspace lint job; package-specific selection unknown.
**Validation:** workflow text inspection; no CI run.
**Coordination / sources:** workflow owner unknown; path above; member `Cargo.toml:289`. [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — CodeQL workspace build configuration
**Identity:** `repo:1232040291:boundary:e2e-chaos-codeql-build-config-001`.
**Cargo dependency (consumer → provider):** none. **Configuration direction (producer → consumer):** workflow configuration → workspace build command. **Data flow:** none. **Impact:** compile failures in selected members may fail the configured job if run.
**Surface:** `cargo build --workspace --all-targets --locked` at `.github/workflows/codeql.yml:211-214`.
**Activation:** workflow trigger/job conditions not evaluated.

**Contract:** command configuration, not proof of build or analysis.
**State / effects:** CI artifacts/status unobserved.
**Failure / propagation:** selected compile failure can stop the configured step.
**Containment:** workflow boundary; package inclusion/result unknown.
**Validation:** source config inspection; no GitHub call/build.
**Coordination / sources:** workflow owner unknown; path above; member `Cargo.toml:289`. [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — Workspace rustfmt configuration
**Identity:** `repo:1232040291:boundary:e2e-chaos-rustfmt-config-001`.
**Cargo dependency (consumer → provider):** none. **Configuration direction (producer → consumer):** workflow configuration → workspace formatting command. **Data flow:** none. **Impact:** source formatting drift may affect the configured job if run.
**Surface:** `cargo fmt --all --check` at `.github/workflows/rustfmt.yml:104-105`.
**Activation:** workflow path filters and events not evaluated.

**Contract:** whole-workspace format check configuration, not a result.
**State / effects:** no package runtime state.
**Failure / propagation:** formatting drift in selected surface can fail the job.
**Containment:** workflow gate; execution unknown.
**Validation:** YAML inspection only; no CI call.
**Coordination / sources:** workflow owner unknown; path above; member list `Cargo.toml:289`. [Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — Nightly workspace mutation configuration
**Identity:** `repo:1232040291:boundary:e2e-chaos-nightly-mutation-config-001`.
**Cargo dependency (consumer → provider):** none. **Configuration direction (producer → consumer):** workflow configuration → workspace mutation command. **Data flow:** none. **Impact:** selected mutant/test outcomes may affect the job if run.
**Surface:** `cargo mutants --workspace --no-shuffle --minimum-test-timeout=600` at `.github/workflows/nightly.yml:490`.
**Activation:** nightly workflow/job conditions not evaluated.

**Contract:** workspace command text only; no mutation/test result.
**State / effects:** CI result and generated artifacts unobserved.
**Failure / propagation:** selected code/test failure may affect the workflow job.
**Containment:** workflow boundary; target selection unknown.
**Validation:** YAML inspection only; no workflow/tool execution.
**Coordination / sources:** workflow owner unknown; path above; member `Cargo.toml:289`. [Relation index](#b03)

<a id="rel-015"></a>
### REL-015 — Library wrapper runner call
**Identity:** `repo:1232040291:boundary:e2e-chaos-scheduler-wrapper-runner-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** wrapper experiment/run ID, snapshot, and fake → runner → returned `ChaosRun`/local callbacks. **Impact:** runner changes may alter wrapper output or local telemetry.
**Surface:** `run_experiment` in `run_clean_staging`; import `tests/e2e-chaos/src/lib.rs:52-55`.
**Activation:** calls reach `:295`; caller selection unknown.

**Contract:** passes scheduler inputs; returns `ChaosRun` plus local fake, not `Result`.
**State / effects:** stack-local inputs/output and callbacks; persistence unknown.
**Failure / propagation:** incompatible API breaks compile; branch/callback change alters returned values or telemetry.
**Containment:** package wrapper boundary; runtime calls unknown.
**Validation:** static call trace only; no runner invocation.
**Coordination / sources:** scheduler runner owner; `tests/e2e-chaos/src/lib.rs:283-297`. [Relation index](#b03)

<a id="rel-016"></a>
### REL-016 — Active SEV-1 deferral test
**Identity:** `repo:1232040291:boundary:e2e-chaos-scheduler-sev1-runner-test-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** local active-SEV-1 snapshot → runner → local result/events. **Impact:** branch changes may falsify the authored deferral predicate.
**Surface:** direct `run_experiment` call in `adversarial_sev1_drill_pause`.
**Activation:** `prod_sev1_active=true` at `tests/e2e-chaos/tests/adversarial_sev1_drill_pause.rs:37-53`.

**Contract:** source asserts Aborted trigger `prod_sev1_active`; runner semantics scheduler-owned.
**State / effects:** local telemetry fake only; external effects not established.
**Failure / propagation:** changed trigger/outcome/event assertions falsify authored case.
**Containment:** test source; no observed run.
**Validation:** inspect snapshots/assertions; target not run.
**Coordination / sources:** scheduler runner owner; test `:37-60`. [Relation index](#b03)

<a id="rel-017"></a>
### REL-017 — Distinct-run-ID seed property
**Identity:** `repo:1232040291:boundary:e2e-chaos-scheduler-distinct-seed-test-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** distinct run IDs and one experiment ID → `derive_seed` calls → property assertions. **Impact:** API or seed-algorithm changes may falsify the distinct-seed predicate.
**Surface:** calls in `tests/e2e-chaos/tests/prop_seed_replay_identical.rs:83-99`; definition `crates/corelink-chaos-scheduler/src/runner.rs:84`.
**Activation:** `prop_distinct_run_ids_distinct_seeds`, after `run_id_a != run_id_b`; execution/case count unknown.

**Contract:** source asserts the returned seeds differ; scheduler defines algorithm.
**State / effects:** seed values are test-local; no persistence.
**Failure / propagation:** signature or collision behavior can falsify the property.
**Containment:** property function; result unknown.
**Validation:** source inspection only; property not run.
**Coordination / sources:** scheduler seed owner; paths above. [Relation index](#b03)

<a id="rel-018"></a>
### REL-018 — Pre-state capture callback
**Identity:** `repo:1232040291:boundary:e2e-chaos-runner-pre-capture-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** runner pending run → local pre-capture fake → fixed digest. **Impact:** callback return changes may alter downstream measurement/outcome.
**Surface:** `capture_state_pre(&pending) -> u64` at `crates/corelink-chaos-scheduler/src/runner.rs:160-161`.
**Activation:** non-abort path after `Started`.

**Contract:** local fake returns constant `0x0000_a11c_e5ee_d000`; it does not capture external state.
**State / effects:** no callback-owned mutation or durability.
**Failure / propagation:** changed return affects downstream measurement inputs.
**Containment:** in-memory fake boundary; invocation count is not observed by fake.
**Validation:** runner call and fake source trace; no runtime call.
**Coordination / sources:** scheduler trait owner; runner/fake `tests/e2e-chaos/src/lib.rs:261-263`. [Relation index](#b03)

<a id="rel-019"></a>
### REL-019 — Post-state capture callback
**Identity:** `repo:1232040291:boundary:e2e-chaos-runner-post-capture-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** runner pending run → local post-capture fake → fixed digest. **Impact:** callback return changes may alter downstream measurement/outcome.
**Surface:** `capture_state_post(&pending) -> u64` at `crates/corelink-chaos-scheduler/src/runner.rs:160-162`.
**Activation:** non-abort path after pre-state callback.

**Contract:** local fake returns constant `0x0000_b0bb_1b1b_0000`; it does not capture external state.
**State / effects:** no callback-owned mutation or durability.
**Failure / propagation:** changed return affects downstream measurement inputs.
**Containment:** in-memory fake boundary; invocation count is not observed by fake.
**Validation:** runner call and fake source trace; no runtime call.
**Coordination / sources:** scheduler trait owner; runner/fake `tests/e2e-chaos/src/lib.rs:265-267`. [Relation index](#b03)

<a id="rel-020"></a>
### REL-020 — SLO impact measurement callback
**Identity:** `repo:1232040291:boundary:e2e-chaos-runner-impact-callback-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** runner digests → local impact fake → configured basis-point result. **Impact:** return/threshold changes may alter the local outcome branch.
**Surface:** `measure_slo_impact(&pending, pre, post) -> u32` at `crates/corelink-chaos-scheduler/src/runner.rs:160-163`.
**Activation:** non-abort path after both capture callbacks.

**Contract:** fake ignores run/digests and returns configured impact in bps.
**State / effects:** reads constructor field; no persistence.
**Failure / propagation:** changed impact or threshold behavior can alter final outcome.
**Containment:** local fake/outcome only; no real SLO observation.
**Validation:** runner/fake source trace; no invocation.
**Coordination / sources:** scheduler trait owner; fake `tests/e2e-chaos/src/lib.rs:269-271`. [Relation index](#b03)

<a id="rel-021"></a>
### REL-021 — Completed audit callback
**Identity:** `repo:1232040291:boundary:e2e-chaos-runner-completed-audit-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** final run/event → local fake vector and counter. **Impact:** callback/outcome changes may alter authored local predicates.
**Surface:** `emit_audit(final_run, Completed)` at `crates/corelink-chaos-scheduler/src/runner.rs:184-185`.
**Activation:** non-abort path after verdict and final record construction.

**Contract:** appends `Completed`; fake saturating-increments only for `SteadyStateBreached`.
**State / effects:** instance-local event vector/counter; no exporter or persistence.
**Failure / propagation:** missing event or altered outcome gate changes local predicates.
**Containment:** test fake only; no delivered metric/audit is established.
**Validation:** runner/fake and unit assertion source; authored, not run.
**Coordination / sources:** scheduler callback owner; runner/fake `tests/e2e-chaos/src/lib.rs:248-258,514-528`. [Relation index](#b03)

<a id="rel-022"></a>
### REL-022 — Foundation workspace clippy configuration
**Identity:** `repo:1232040291:boundary:e2e-chaos-cas-foundation-clippy-config-001`.
**Cargo dependency (consumer → provider):** none. **Configuration direction (producer → consumer):** workflow configuration → workspace clippy command. **Data flow:** none. **Impact:** compile/lint diagnostics in selected members may fail this step if run.
**Surface:** `cargo clippy --workspace --all-targets -- -D warnings` at `.github/workflows/cas_foundation.yml:160-161`.
**Activation:** workflow/job conditions and event not evaluated.

**Contract:** lint command text only; no package-specific result.
**State / effects:** CI job state unobserved.
**Failure / propagation:** if run and member selected, diagnostics may fail the step.
**Containment:** workspace step; target selection unknown.
**Validation:** YAML inspection; no CI or Cargo run.
**Coordination / sources:** workflow owner unknown; path above; membership `Cargo.toml:289`. [Relation index](#b03)

<a id="rel-023"></a>
### REL-023 — Foundation workspace test configuration
**Identity:** `repo:1232040291:boundary:e2e-chaos-cas-foundation-test-config-001`.
**Cargo dependency (consumer → provider):** none. **Configuration direction (producer → consumer):** workflow configuration → workspace test command. **Data flow:** none. **Impact:** failures in selected tests may fail this step if run.
**Surface:** `cargo test --workspace --all-targets` at `.github/workflows/cas_foundation.yml:163-164`.
**Activation:** workflow/job conditions and event not evaluated.

**Contract:** test command text only; no test result or target log.
**State / effects:** CI status and test effects unobserved.
**Failure / propagation:** failing selected tests can fail the configured step.
**Containment:** workflow test step; package selection/result unknown.
**Validation:** YAML inspection; no CI/Cargo/test run.
**Coordination / sources:** workflow owner unknown; path above; membership `Cargo.toml:289`. [Relation index](#b03)

<a id="rel-024"></a>
### REL-024 — Foundation workspace documentation configuration
**Identity:** `repo:1232040291:boundary:e2e-chaos-cas-foundation-doc-config-001`.
**Cargo dependency (consumer → provider):** none. **Configuration direction (producer → consumer):** workflow configuration → workspace doc command. **Data flow:** none. **Impact:** rustdoc errors in selected members may fail this step if run.
**Surface:** `cargo doc --no-deps --workspace` at `.github/workflows/cas_foundation.yml:166-169`.
**Activation:** workflow/job conditions and event not evaluated.

**Contract:** doc command text only; no generated docs or result observed.
**State / effects:** workflow artifacts/status unknown.
**Failure / propagation:** selected rustdoc error may fail configured step.
**Containment:** workflow documentation step; package selection/result unknown.
**Validation:** YAML inspection only; no GitHub/Cargo call.
**Coordination / sources:** workflow owner unknown; path above; member `Cargo.toml:289`. [Relation index](#b03)

<a id="rel-025"></a>
### REL-025 — Cleared SEV-1 resume test
**Identity:** `repo:1232040291:boundary:e2e-chaos-scheduler-sev1-resume-test-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** local cleared-SEV-1 snapshot → runner → local result/events. **Impact:** branch changes may falsify the authored resume predicate.
**Surface:** direct `run_experiment` call in `adversarial_sev1_drill_pause`.
**Activation:** `prod_sev1_active=false` at `tests/e2e-chaos/tests/adversarial_sev1_drill_pause.rs:71-87`.

**Contract:** source asserts Passed and `[Started, Completed]`; runner semantics scheduler-owned.
**State / effects:** local telemetry fake only; external effects not established.
**Failure / propagation:** altered outcome/event assertions falsify authored case.
**Containment:** test source; no observed run.
**Validation:** inspect snapshot/assertions; target not run.
**Coordination / sources:** scheduler runner owner; test `:71-88`. [Relation index](#b03)

<a id="rel-026"></a>
### REL-026 — Catalog-miss local fallback
**Identity:** `repo:1232040291:boundary:e2e-chaos-catalog-miss-fallback-001`.
**Cargo dependency (consumer → provider):** e2e-chaos → scheduler. **Data flow (producer → consumer):** catalog miss → local fallback arm. **Impact:** arm/value changes can alter constructor results.
**Surface:** miss branch of `make_experiment` after `canonical_catalog()` lookup (`tests/e2e-chaos/src/lib.rs:113-120,122-203`).
**Activation:** mapped ID not found; catalog contents/hits unknown.

**Contract:** six local variant records; network/latency share one synthetic arm.
**State / effects:** returns a local `ChaosExperiment`; no persistence.
**Failure / propagation:** API drift breaks compilation; fallback mismatch can falsify local caller assertions.
**Containment:** library constructor boundary; no scheduler mutation.
**Validation:** trace match arms; no call or test executed.
**Coordination / sources:** local constructor owner unknown; source above. [Relation index](#b03)

<a id="rel-027"></a>
### REL-027 — Per-PR changed-line mutation configuration
**Identity:** `repo:1232040291:boundary:e2e-chaos-mutation-pr-in-diff-001`.
**Cargo dependency:** none. **Configuration direction:** PR filter → mutation step. **Data flow:** diff → `pr.diff` → cargo-mutants. **Impact:** survivor may fail gate; result unknown.
**Surface:** `.github/workflows/mutation-pr.yml:45-54,147-168,170-190`; uses `cargo mutants --in-diff pr.diff`.
**Activation:** `.rs`, `Cargo.toml`, or `Cargo.lock` path matches; no-Rust diff exits early.

**Contract:** config-only; mutation scope is changed lines.
**State / effects:** CI status/artifacts external; unobserved.
**Failure / propagation:** command/survivor gate may fail job; outcome unknown.
**Containment:** workflow job; no package/runtime effect established.
**Validation:** YAML only; no GitHub/Cargo/mutation run.
**Coordination / sources:** workflow owner unknown; path and lines above. [Relation index](#b03)

<a id="rel-028"></a>
### REL-028 — Scenario callers of `make_experiment`
**Identity:** `repo:1232040291:boundary:e2e-chaos-scenario-constructor-callers-001`.
**Cargo dependency:** none. **Flow:** eight scenario sources → API-002. **Impact:** API/output drift can break assertions.
**Surface:** direct calls in all eight scenario paths enumerated in [R06](REFERENCE.md#r06), around `:23-24`.
**Activation:** each caller target selected; execution unknown.

**Contract:** drill in; `ChaosExperiment` out.
**State / effects:** test-local value only.
**Failure / propagation:** signature drift fails compile; field drift may fail assertions.
**Containment:** eight declared sources; runtime reachability unknown.
**Validation:** calls/source assertions inspected; not run.
**Coordination / sources:** package owner unknown; paths at R06. [Relation index](#b03)

<a id="rel-029"></a>
### REL-029 — Adversarial callers of `make_experiment`
**Identity:** `repo:1232040291:boundary:e2e-chaos-adversarial-constructor-callers-001`.
**Cargo dependency:** none. **Flow:** adversarial targets → API-002. **Impact:** altered input may change runner predicates.
**Surface:** staging-only `:32,68`; SEV-1 `:37,71`.
**Activation:** each listed branch; execution unknown.

**Contract:** drill in; experiment value out.
**State / effects:** local test value only.
**Failure / propagation:** API drift fails compile; field drift affects assertions.
**Containment:** two target sources; external effects unknown.
**Validation:** calls/inputs inspected; not run.
**Coordination / sources:** package owner unknown; paths in R06. [Relation index](#b03)

<a id="rel-030"></a>
### REL-030 — Property caller of `make_experiment`
**Identity:** `repo:1232040291:boundary:e2e-chaos-property-constructor-caller-001`.
**Cargo dependency:** none. **Flow:** generated drill → API-002 → ID comparison. **Impact:** mapping drift can falsify property.
**Surface:** `prop_seed_replay_identical.rs:83-85`.
**Activation:** property case runs; selection/count unknown.

**Contract:** returns experiment for generated drill.
**State / effects:** case-local value.
**Failure / propagation:** changed ID may fail property assertion.
**Containment:** one property; result unknown.
**Validation:** source inspected; not run.
**Coordination / sources:** package owner unknown; path in R06. [Relation index](#b03)

<a id="rel-031"></a>
### REL-031 — Unit caller of `make_experiment`
**Identity:** `repo:1232040291:boundary:e2e-chaos-unit-constructor-caller-001`.
**Cargo dependency:** none. **Flow:** unit loop → API-002 → API-008. **Impact:** changed cap input may fail assertion.
**Surface:** unit call at `tests/e2e-chaos/src/lib.rs:493`.
**Activation:** charter unit test selected; not run.

**Contract:** each drill yields an experiment.
**State / effects:** test-local values only.
**Failure / propagation:** changed output may fail helper assertion.
**Containment:** one unit test; result unknown.
**Validation:** loop/helper source inspected; not run.
**Coordination / sources:** package owner unknown; library source above. [Relation index](#b03)

<a id="rel-032"></a>
### REL-032 — Scenario callers of `run_clean_staging`
**Identity:** `repo:1232040291:boundary:e2e-chaos-scenario-wrapper-callers-001`.
**Cargo dependency:** none. **Flow:** scenario inputs → API-006 → local run/fake. **Impact:** wrapper drift may fail predicates.
**Surface:** eight R06 scenario paths; CPU has calls `:30,47`, latency `:32,48`.
**Activation:** each caller branch; selection/execution unknown.

**Contract:** drill, run-ID text, impact bps → run and telemetry.
**State / effects:** returned local values; no `Result`.
**Failure / propagation:** signature drift fails compile; output drift may fail assertions.
**Containment:** declared test sources; external effects unknown.
**Validation:** calls inspected against R06; not run.
**Coordination / sources:** package/scheduler owners; paths in R06. [Relation index](#b03)

<a id="rel-033"></a>
### REL-033 — Property caller of `run_clean_staging`
**Identity:** `repo:1232040291:boundary:e2e-chaos-property-wrapper-caller-001`.
**Cargo dependency:** none. **Flow:** generated case → two API-006 calls → replay comparisons. **Impact:** drift may falsify property.
**Surface:** property calls at `prop_seed_replay_identical.rs:66,68`.
**Activation:** each generated case; case count from R06 config.

**Contract:** same inputs return comparable run/fake pairs.
**State / effects:** case-local; fake discarded.
**Failure / propagation:** differing run fields fail assertions.
**Containment:** one property; result unknown.
**Validation:** source inspected; not run.
**Coordination / sources:** package/scheduler owners; path in R06. [Relation index](#b03)

<a id="rel-034"></a>
### REL-034 — Unit caller of `run_clean_staging`
**Identity:** `repo:1232040291:boundary:e2e-chaos-unit-wrapper-caller-001`.
**Cargo dependency:** none. **Flow:** unit inputs → API-006 → fake counter check. **Impact:** changed callbacks may fail predicate.
**Surface:** `tests/e2e-chaos/src/lib.rs:518`.
**Activation:** telemetry unit test selected; unknown.

**Contract:** latency drill, fixed ID, 600 bps → run/fake.
**State / effects:** stack-local only.
**Failure / propagation:** changed counter fails assertion.
**Containment:** one unit test; no production metric.
**Validation:** source assertion inspected; not run.
**Coordination / sources:** package owner unknown; library source. [Relation index](#b03)

<a id="rel-035"></a>
### REL-035 — Scenario callers of rollback helper
**Identity:** `repo:1232040291:boundary:e2e-chaos-scenario-rollback-callers-001`.
**Cargo dependency:** none. **Flow:** eight scenario sources → API-008. **Impact:** cap result affects setup assertions.
**Surface:** eight R06 scenario paths, calls near `:25-26`.
**Activation:** each helper call reached; not run.

**Contract:** reads rollback seconds; `Ok` iff ≤300.
**State / effects:** read-only; performs no rollback.
**Failure / propagation:** above-cap `Err` fails caller unwrap.
**Containment:** assertion only; no rollback inference.
**Validation:** helper/calls inspected; not run.
**Coordination / sources:** package owner unknown; paths in R06. [Relation index](#b03)

<a id="rel-036"></a>
### REL-036 — Unit caller of rollback helper
**Identity:** `repo:1232040291:boundary:e2e-chaos-unit-rollback-caller-001`.
**Cargo dependency:** none. **Flow:** unit loop → API-008 → unwrap. **Impact:** return drift changes test result.
**Surface:** `tests/e2e-chaos/src/lib.rs:493-494`.
**Activation:** charter unit test selected; not run.
**Contract:** eight experiment caps must be ≤300 seconds.
**State / effects:** test-local; no rollback.
**Failure / propagation:** any `Err` panics via unwrap.
**Containment:** one unit test.
**Validation:** source inspected; not run.
**Coordination / sources:** package owner unknown; library source. [Relation index](#b03)

<a id="rel-037"></a>
### REL-037 — Scenario callers of audit-shape helper
**Identity:** `repo:1232040291:boundary:e2e-chaos-scenario-audit-helper-callers-001`.
**Cargo dependency:** none. **Flow:** eight scenario snapshots → API-007. **Impact:** shape drift may fail assertions.
**Surface:** helper calls in eight R06 scenario paths after `audit_events()`.
**Activation:** event assertion reached; not run.

**Contract:** only `[Aborted]` or `[Started, Completed]` returns `Ok`.
**State / effects:** reads slice; no mutation.
**Failure / propagation:** invalid shape returns `Err`; callers unwrap.
**Containment:** local helper; emission edges are REL-007/008/021.
**Validation:** calls/assertions inspected; not run.
**Coordination / sources:** package owner unknown; paths in R06. [Relation index](#b03)

<a id="rel-038"></a>
### REL-038 — Adversarial callers of audit-shape helper
**Identity:** `repo:1232040291:boundary:e2e-chaos-adversarial-audit-helper-callers-001`.
**Cargo dependency:** none. **Flow:** adversarial fake events → API-007. **Impact:** helper drift affects abort assertions.
**Surface:** staging `:56`; SEV-1 `:59`.
**Activation:** safe-mode assertion branch; not run.

**Contract:** accepts canonical sequence only.
**State / effects:** borrows local events.
**Failure / propagation:** noncanonical shape returns `Err`; caller unwraps.
**Containment:** two local targets; no audit delivery.
**Validation:** call source inspected; not run.
**Coordination / sources:** package owner unknown; paths in R06. [Relation index](#b03)

<a id="rel-039"></a>
### REL-039 — Scenario callers of audit-event getter
**Identity:** `repo:1232040291:boundary:e2e-chaos-scenario-audit-getter-callers-001`.
**Cargo dependency:** none. **Flow:** eight scenario fakes → API-004 clone. **Impact:** snapshot drift may fail predicates.
**Surface:** `audit_events()` at eight R06 paths; CPU/latency have extra checks.
**Activation:** each call reached; selection unknown.

**Contract:** clone FIFO events for this fake.
**State / effects:** read only; `RefCell` conflict may panic.
**Failure / propagation:** panic or changed snapshot affects assertions.
**Containment:** fake-local; no audit sink.
**Validation:** call census/source inspected; not run.
**Coordination / sources:** package owner unknown; paths in R06. [Relation index](#b03)

<a id="rel-040"></a>
### REL-040 — Adversarial callers of audit-event getter
**Identity:** `repo:1232040291:boundary:e2e-chaos-adversarial-audit-getter-callers-001`.
**Cargo dependency:** none. **Flow:** two adversarial fakes → API-004. **Impact:** snapshot drift may alter safe-mode assertions.
**Surface:** staging `:55`; SEV-1 `:58,86` (paths in R06).
**Activation:** stated branches reached; not run.

**Contract:** clone each fake's FIFO vector.
**State / effects:** read-only local vector.
**Failure / propagation:** borrow panic or changed events affects assertion.
**Containment:** local targets; no sink claim.
**Validation:** direct calls inspected; not run.
**Coordination / sources:** package owner unknown; paths in R06. [Relation index](#b03)

<a id="rel-041"></a>
### REL-041 — Scenario callers of SLO counter getter
**Identity:** `repo:1232040291:boundary:e2e-chaos-scenario-counter-getter-callers-001`.
**Cargo dependency:** none. **Flow:** eight scenario fakes → API-005 `u64`. **Impact:** counter drift may fail checks.
**Surface:** getter at eight R06 paths; CPU/latency add branch checks.
**Activation:** assertion reached; not run.

**Contract:** reads instance counter; no mutation.
**State / effects:** local `u64`; borrow conflict may panic.
**Failure / propagation:** panic or changed value fails comparison.
**Containment:** no export/persistence.
**Validation:** calls/assertions inspected; not run.
**Coordination / sources:** package owner unknown; paths in R06. [Relation index](#b03)

<a id="rel-042"></a>
### REL-042 — Adversarial callers of SLO counter getter
**Identity:** `repo:1232040291:boundary:e2e-chaos-adversarial-counter-getter-callers-001`.
**Cargo dependency:** none. **Flow:** adversarial fake → API-005. **Impact:** value drift may alter assertion.
**Surface:** staging `:60`; SEV-1 `:64`.
**Activation:** these assertion branches; not run.

**Contract:** reads local counter without mutation.
**State / effects:** fake-local; borrow conflict may panic.
**Failure / propagation:** panic/value drift affects assertion.
**Containment:** no metric export.
**Validation:** calls/assertions inspected; not run.
**Coordination / sources:** package owner unknown; paths in R06. [Relation index](#b03)

<a id="rel-043"></a>
### REL-043 — Unit caller of SLO counter getter
**Identity:** `repo:1232040291:boundary:e2e-chaos-unit-counter-getter-caller-001`.
**Cargo dependency:** none. **Flow:** unit fake → API-005. **Impact:** counter drift fails test assertion.
**Surface:** getter at `tests/e2e-chaos/src/lib.rs:527`.
**Activation:** telemetry unit test selected; unknown.

**Contract:** reads counter; source expects 1 after breach setup.
**State / effects:** fake-local; no export.
**Failure / propagation:** wrong value fails assertion.
**Containment:** one unit test.
**Validation:** source inspected; not run.
**Coordination / sources:** package owner unknown; library source. [Relation index](#b03)

<a id="rel-044"></a>
### REL-044 — Adversarial telemetry constructors
**Identity:** `repo:1232040291:boundary:e2e-chaos-adversarial-telemetry-new-001`.
**Cargo dependency:** none. **Flow:** adversarial inputs → API-003. **Impact:** init drift changes fake predicates.
**Surface:** staging `:33,69`; SEV-1 `:38,72`.
**Activation:** constructor calls; targets unrun.

**Contract:** empty events, zero count, supplied bps.
**State / effects:** new instance-local cells and field.
**Failure / propagation:** signature drift fails compile; state drift affects checks.
**Containment:** local fake; no metric service.
**Validation:** calls/constructor inspected; not run.
**Coordination / sources:** package owner unknown; paths in R06. [Relation index](#b03)

<a id="rel-045"></a>
### REL-045 — Wrapper telemetry constructor
**Identity:** `repo:1232040291:boundary:e2e-chaos-wrapper-telemetry-new-001`.
**Cargo dependency:** none. **Flow:** wrapper impact bps → API-003. **Impact:** init drift alters returned fake.
**Surface:** internal call at `tests/e2e-chaos/src/lib.rs:289`.
**Activation:** wrapper call; caller selection unknown.

**Contract:** initializes empty fake and stores bps.
**State / effects:** wrapper-local lifetime; no durability.
**Failure / propagation:** API drift breaks wrapper compilation.
**Containment:** local helper; no production sink.
**Validation:** source trace only; not invoked.
**Coordination / sources:** package owner unknown; library source. [Relation index](#b03)

<a id="rel-046"></a>
### REL-046 — Property caller of drill mapper
**Identity:** `repo:1232040291:boundary:e2e-chaos-property-experiment-id-caller-001`.
**Cargo dependency:** none. **Flow:** generated drill → API-001 ID. **Impact:** mapping drift may fail seed/ID comparisons.
**Surface:** `experiment_id()` at property `:71,97-98`.
**Activation:** property cases; env count/execution unknown.

**Contract:** fixed static string per variant.
**State / effects:** pure; no mutation.
**Failure / propagation:** changed ID alters assertions.
**Containment:** one property target; result unknown.
**Validation:** call/assertions inspected; not run.
**Coordination / sources:** package owner unknown; path in R06. [Relation index](#b03)

<a id="rel-047"></a>
### REL-047 — Unit caller of drill mapper
**Identity:** `repo:1232040291:boundary:e2e-chaos-unit-experiment-id-caller-001`.
**Cargo dependency:** none. **Flow:** eight variants → API-001 strings. **Impact:** duplicate mapping fails predicate.
**Surface:** iterator call at `tests/e2e-chaos/src/lib.rs:473`.
**Activation:** `drill_ids_unique` selected; unknown.

**Contract:** variant maps to static ID.
**State / effects:** local collection only.
**Failure / propagation:** duplicates may fail assertion.
**Containment:** one unit test; not runtime proof.
**Validation:** source inspected; not run.
**Coordination / sources:** package owner unknown; library source. [Relation index](#b03)

<a id="rel-048"></a>
### REL-048 — Adversarial held-kind label consumer
**Identity:** `repo:1232040291:boundary:e2e-chaos-adversarial-label-001`.
**Cargo dependency:** none. **Data flow (producer → consumer):** `OpsEventKind::label(held_by)` → `dr_drill` assertion. **Impact:** label/enum drift can fail this predicate.
**Surface / activation:** `tests/e2e-chaos/tests/adversarial_ops_exclusivity.rs:35-37`, after the chaos attempt returns `LockHeld { held_by }`; target selection/run unknown.

**Contract:** held kind is `DrDrill`, helper returns `"dr_drill"`; pure.
**State / effects:** reads enum and compares a local string; no mutation/external sink.
**Failure / propagation:** changed label/kind or missing `LockHeld` fails assertion; no production audit effect established.
**Boundary:** one package adversarial target; no runtime evidence.
**Validation:** call/assertion inspected; target not run.
**Coordination / evidence:** package owner unknown; implementation `tests/e2e-chaos/src/lib.rs:336-369`, consumer above. [Relation index](#b03)

<a id="b04"></a>
## B04 — Propagation and ownership

| Potential path | Condition | Impact boundary | Evidence/status |
|---|---|---|---|
| scheduler catalog API → package constructor (`REL-001,003`) | `make_experiment` call with matching ID | cloned upstream record | hit branch traced; catalog hit not observed |
| package constructor → local fallback (`REL-026`) | `make_experiment` lookup misses | six local records or shared network/latency fallback | source arms traced; miss not observed |
| scheduler runner/API → wrapper and direct test callers (`REL-001,004,005,015,016,025`) | wrapper or individual adversarial target called | returned run and authored assertions | call sites/snapshots traced; no run |
| runner → local fake callbacks (`REL-007,008,018–021`) | abort, Started, capture, measurement, or Completed branch | only instance-local fake events/counter/constants | branch/callback source traced; no delivery or runtime observation |
| scheduler seed API + `proptest` → property target (`REL-002,006,017`) | property target runs each separate property | same-input replay equality or distinct-ID inequality predicate | source only; execution/case count unknown |
| lock API → adversarial/unit assertions (`REL-009`); label helper → adversarial assertion (`REL-048`) | test caller executes | one local lock and held-label comparison | source predicates only; shared lock/runtime label unknown |
| package test/unit sources → local public API calls (`REL-028–048`) | individually listed caller target/function runs | constructor/wrapper/helper results and local fake assertions | call sites inventoried; targets and unit tests unrun |
| workspace workflow commands → member package (`REL-010–014,022–024`) | workflow trigger and selection include workspace member | distinct format/lint/test/doc/build/mutation job steps | configuration only; event, selection, and outcome unobserved |
| mutation-pr path filter → changed-line mutation command (`REL-027`) | PR matches workflow filter and includes Rust diff | configured cargo-mutants run over diff lines | YAML only; no workflow/action/command executed |

Scheduler owns runner, catalog, types, and seed algorithm; this package owns drill enum, constructors, callbacks, assertions, lock model, and targets. These relations establish no `ops_event_lock` D1 row, production adapter, audit exporter, or provider call. Canonical routing is only the [verified OKF profile](../../../../docs/internal/okf-wiki/01-okf-corelink-profile.contract.md).

<a id="b05"></a>
## B05 — Change-to-impact and validation matrix

| Change | Affected contracts / RELs | Consumer/state consequence | Required static validation (not performed here) | Coordination / recovery boundary |
|---|---|---|---|---|
| Dependency/target declaration | REL-001/002; [R06](REFERENCE.md#r06) | declared graph changes | inspect manifest and reverse search | scheduler/proptest owner; manifest out of scope |
| Catalog mapping/fallback | API-002; REL-003/026 | cloned record or local value changes | inspect hit/miss arms and callers | scheduler catalog owner; keep paths distinct |
| Runner signature/snapshot | API-006; REL-005/015/016/025 | returned run/assertions may change | inspect calls, inputs, and assertions | scheduler runner owner; no result inferred |
| Telemetry method/branch | API-003–007, API-013–016; REL-007/008/018–021 | local events/digests/counter may change | trace runner callback and fake method | scheduler callback owner; no delivery claim |
| Local API/caller | API-001–017; REL-028–048 | enumerated test predicate may change | reconcile direct callers and assertions | package owner unknown; acceptance/operation blocked |
| Seed API/property | REL-006/017 | replay/distinct-seed predicates may change | inspect inputs and both properties | scheduler seed owner; not executed |
| Local lock | API-010–012, API-017; INV-006; REL-009 | instance-exclusion assertions may change | inspect mutex and guard lifecycle | package owner; no shared-lock claim |
| Workspace workflow | REL-010–014/022–024 | configured workspace step may differ | inspect exact trigger/command/filter | workflow owner; no run/deploy claim |
| Per-PR mutation workflow | REL-027 | Rust diff may reach cargo-mutants gate | inspect filter, no-Rust guard, survivor handling | workflow owner unknown; config only |
| D1 chaos migration | B06 exclusion; no direct package REL | scheduler schema may change; package link unproved | inspect `migrations/d1/0033_chaos_runs.sql` and package refs | schema owner unknown; no harness-write claim |

<a id="b06"></a>
## B06 — Population counts and unknowns

| Population | Discovered | Documented | Excluded with reason | Unknown |
|---|---:|---:|---:|---:|
| Package manifest direct dependency declarations | 2 | 2 (REL-001/002) | 0 | 0 |
| Declared integration test targets | 12 | 12 (REFERENCE R06) | 0 | 0 |
| Tracked Cargo.toml matches for external `e2e-chaos` consumers | 0 found | 0 | 0 | 0 within searched manifests |
| Package-local API/callback/lock call groups | 37 | 37 (REL-003–009, REL-015–021, REL-025–026, REL-028–048) | 0 | 0 within inspected package source/tests |
| Executable workspace-wide workflow command groups | 8 | 8 (REL-010–014, REL-022–024) | 0 | 0 in matched workflow files |
| Per-PR changed-line mutation workflow | 1 | 1 (REL-027) | 0 | 0 in inspected workflow source; runtime result unknown |
| Tracked D1 chaos migration candidates | 1 | 0 direct package RELs | 1 (migration has no package manifest/source link) | 1 deployment/runtime-use status |
| Prose-only workflow examples | 2 | 0 | 2 (not executable config) | 0 |
| Out-of-repository callers, runtime wiring, resolved/selected targets, execution results | — | — | — | UNKNOWN |

Excluded from direct RELs: B02 prose examples and unlinked migration `0033`, which defines `chaos_runs`, `chaos_results`, their FK, and audit comments. Package source/manifest has no direct consumer; deployment/production use remains unestablished. Target census matches [R06](REFERENCE.md#r06). Five workspace-command workflows plus `mutation-pr` are config only.

Unknown: out-of-repository consumers, composition, CI selection/results, resolved graph/features, providers, persistence, rollback effects, telemetry delivery, and test outcomes. New source/config matches require atomic RELs for endpoints, activation, contract, failure, and validation.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#b01)
