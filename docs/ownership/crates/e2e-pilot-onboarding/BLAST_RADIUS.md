---
schema: corelink-ownership/1.1
document: blast_radius
package: e2e-pilot-onboarding
manifest: tests/e2e-pilot-onboarding/Cargo.toml
source_commit: cb94e251c0f17382565bf863f517945cbb2a84d6
profile: S
state: draft
evidence_set: w014-e2e-pilot-onboarding-source-static-20260921
---

# e2e-pilot-onboarding — blast radius

This graph is static and bounded to repository evidence at the pinned source baseline. It separates Cargo/build, Rust source composition, local state, test assertions, CI command declarations, and documentary references. A relation records three independent directions; none proves command execution, full reverse-consumer completeness, or production runtime.

[Scope](#b01) · [Method/populations](#b02) · [Direct relations](#b03) · [Propagation](#b04) · [Change impact](#b05) · [Coverage/unknowns](#b06).

<a id="b01"></a>
## B01 — Scope and risk summary

Impact is drift in local harness/test contracts; no direct first-party package
dependency exists. Workspace and five target declarations are not resolved facts.
Source state is in memory; build/test/provider/deployment/production behavior is
unobserved, and documentary prose may conflict with source.

Package implementation owner is `UNASSIGNED`; ask a repository administrator to
assign one and stop owner-authorized actions until recorded. CODEOWNERS only requests
review from `@gmhelmold`; it grants no implementation, operations, or independent-approval authority.

**Build selection evaluated:** declared library, five test targets, root workspace membership; no resolved target/features/build artifact observed.
**Environments not observed:** CI execution, local test execution, production, providers, external consumers.

<a id="b02"></a>
## B02 — Method and reconciled populations

| Population | Sources/selection | Result and limit |
|---|---|---|
| Cargo declarations | Root/package manifests read at baseline | Workspace member plus package; no direct first-party dependency; no resolution command |
| Source and inverse imports | Package Rust source, test imports, verifier source/tests | Local module/use and test-source links only; no compile or call observation |
| Runtime/data | Harness fields and method source; repository exact search | Local slug maps/audit vector; no external store, endpoint, or data path established |
| Build/operation | Four workflow files, bounded test script, runbook/backlog | Commands and docs inspected; no invocation, result, or executable effect inferred for docs |
| Documentary cross-check | Three gapmaps, ownership plan/census/status/rollout, billing blast | Conflicts retained as prose evidence; does not override source |

At baseline, path-only `git grep -l -F -- 'e2e-pilot-onboarding' cb94e251c --` returned
20 tracked paths: two inside the package and 18 outside. Separately, manifest/path
enumeration finds nine package manifest/source/target files; it is not a token-hit
count. The path-only query displayed no matching content. Import tracing found two
verifier-test consumers; workflow inspection found four relevant command paths.
These bounded results do not establish external-repository completeness.

<a id="b03"></a>
## B03 — Direct relation index

| ID | Type / endpoints | Surface and activation |
|---|---|---|
| [REL-001](#rel-001) | build-deploy · root manifest → package manifest | workspace membership |
| [REL-002](#rel-002) | reexport · harness module → crate root | public aliases |
| [REL-003](#rel-003) | dependency · `harness.rs` → lifecycle file | path module |
| [REL-004](#rel-004) | data · signup → local tenant state | method call |
| [REL-005](#rel-005) | data · upload → local tenant state | method call |
| [REL-006](#rel-006) | data · read → local CAS maps | method call |
| [REL-007](#rel-007) | data · audit rows → export result | method read |
| [REL-008](#rel-008) | data · verifier ← body/manifest | method call |
| [REL-009](#rel-009) | data · DSR request → tenant state | method call |
| [REL-010](#rel-010) | data · DSR finalizer → tenant state | method call |
| [REL-011](#rel-011) | data · cancel → tenant state | method call |
| [REL-012](#rel-012) | data · offboarding → tenant state | method call |
| [REL-013](#rel-013) | test · test 01 → public harness API | manifest target |
| [REL-014](#rel-014) | test · test 02 → public harness API | manifest target |
| [REL-015](#rel-015) | test · test 03 → public harness API | manifest target |
| [REL-016](#rel-016) | test · test 04 → public harness API | manifest target |
| [REL-017](#rel-017) | test · test 05 → public harness API | manifest target |
| [REL-018](#rel-018) | static-check · B-269 verifier → `harness.rs` | path binding |
| [REL-019](#rel-019) | test · verifier-test → verifier | Python import |
| [REL-020](#rel-020) | static-check · B-126 verifier → `harness.rs` | parent wiring |
| [REL-021](#rel-021) | test · split-verifier-test → verifier | Python import |
| [REL-022](#rel-022) | build-deploy · nightly workflow → runner script | schedule/dispatch |
| [REL-023](#rel-023) | build-deploy · runner script → workspace targets | conditional command |
| [REL-024](#rel-024) | build-deploy · CAS workflow → workspace targets | dispatch |
| [REL-025](#rel-025) | build-deploy · lint workflow → workspace targets | push / workflow edit / dispatch |
| [REL-026](#rel-026) | build-deploy · CodeQL → workspace targets | schedule/dispatch |
| [REL-027](#rel-027) | documentation · draft runbook → package | text only |
| [REL-028](#rel-028) | documentation · backlog B-269 → package | text only |
| [REL-029](#rel-029) | documentation · journeys gapmap → package | text only |
| [REL-030](#rel-030) | documentation · surfaces gapmap → package | text only |
| [REL-031](#rel-031) | documentation · quality gapmap → package | text only |
| [REL-032](#rel-032) | documentation · campaign records → package | inventory/planning |
| [REL-033](#rel-033) | documentation · pilot `lib.rs` → peer billing record `REL-E2E-008` | shared stable fingerprint |
| [REL-034](#rel-034) | dependency · package → `blake3` | normal dependency |
| [REL-035](#rel-035) | dependency · package → `hex` | normal dependency |
| [REL-036](#rel-036) | dependency · package → `serde` | normal dependency |
| [REL-037](#rel-037) | dependency · package → `serde_json` | normal dependency |
| [REL-038](#rel-038) | dependency · package → `thiserror` | normal dependency |
| [REL-039](#rel-039) | data · export → local audit vector | export marker writes |
| [REL-040](#rel-040) | static-check · B-269 verifier → lifecycle source | method markers |
| [REL-041](#rel-041) | static-check · B-126 verifier → lifecycle source | method population |

<a id="rel-001"></a>
### REL-001 — Workspace membership
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-workspace-001`; root `Cargo.toml` → package manifest.
- **Directions:** dependency N/A; data N/A; impact workspace membership → workspace selector visibility.
- **Surface/activation:** member path `tests/e2e-pilot-onboarding`; active when a workspace command selects members.
- **Contract/state:** package remains named by its manifest; no runtime data.
- **Failure/propagation:** removal can omit the package from workspace-wide commands; no result inferred.
- **Containment:** package-specific manifest remains a separate declaration.
- **Validation:** source lines root `Cargo.toml:357`, package manifest; resolution not run.
- **Coordination/evidence:** root workspace owner not verified; source snapshot only. [Relation index](#b03)

<a id="rel-002"></a>
### REL-002 — Public root re-exports
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-reexport-001`; `harness` symbols → `lib.rs` root aliases.
- **Directions:** dependency crate root → local module; data public names outward; impact symbol/type change → root imports/tests.
- **Surface/activation:** `pub mod harness` and exact `pub use harness::{...}`; library compile.
- **Contract/state:** API-001–016; defining owner remains `harness.rs`.
- **Failure/propagation:** removing/renaming alias can break downstream Rust imports.
- **Containment:** only package API/test consumers known; external consumers unknown.
- **Validation:** compare `lib.rs:56–69`, REL-013–017; compile not run.
- **Coordination/evidence:** package source; no peer approval inferred. [Relation index](#b03)

<a id="rel-003"></a>
### REL-003 — Path-included lifecycle module
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-lifecycle-module-001`; `harness.rs` → sibling `harness_lifecycle.rs`.
- **Directions:** dependency consumer module → provider file; data N/A; impact binding/method changes → package methods and verifier checks.
- **Surface/activation:** `#[path = "harness_lifecycle.rs"] mod harness_lifecycle;` and `use super::*;`.
- **Contract/state:** adds lifecycle methods to local `PilotHarness`; no new package edge.
- **Failure/propagation:** missing/wrong path or parent items can break module compilation.
- **Containment:** package boundary; no production lifecycle owner proven.
- **Validation:** REL-018–021 inspect binding/population; static only.
- **Coordination/evidence:** `harness.rs:451–452`; `harness_lifecycle.rs:1–235`. [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Signup state mutation
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-signup-state-001`; `complete_signup` → slug-keyed `TenantState` lifecycle/subscription/checkout fields.
- **Directions:** dependency N/A; data method → local state; impact method/state change → five scenario sources.
- **Surface/activation:** public method after local guards; test targets REL-013–017 call it.
- **Contract/state:** PendingActivation then Active; one in-memory mutex map.
- **Failure/propagation:** CheckoutPending/PendingActivation precede CheckoutSessionCreated audit; typed duplicate error.
- **Containment:** local harness only; no production signup effect.
- **Validation:** `harness.rs:525–585`; test_01 final state/order only.
- **Coordination/evidence:** API-006, INV-003; source ordering is falsifiable. [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Batch upload local state
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-upload-state-001`; `batch_upload_blobs` → slug-keyed `TenantState.cas`.
- **Directions:** dependency N/A; data payload → local digest map; impact upload/state change → tests 02–05.
- **Surface/activation:** local lifecycle exists and subscription Active.
- **Contract/state:** key is digest hex inside slug state; receipt `tenant_prefix` is display text, not key.
- **Failure/propagation:** insert precedes per-blob audit; digest collision mismatch/errors can follow earlier writes.
- **Containment:** one harness instance; no R2 prefix/storage behavior.
- **Validation:** `harness.rs:607–659`; REL-014–017 assert final state.
- **Coordination/evidence:** API-007, INV-004; tests do not assert operation order. [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Tenant-local blob read
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-read-state-001`; `read_blob` → caller/other slug CAS maps.
- **Directions:** dependency N/A; data local map → returned bytes/error; impact map/key change → test 02.
- **Surface/activation:** caller digest first checked in own slug, then other slug maps.
- **Contract/state:** own bytes return; digest found only elsewhere yields `TenantPrefixViolation`.
- **Failure/propagation:** absent digest yields `TenantNotProvisioned`; mutex poison yields `Invariant`.
- **Containment:** local map scan, not a prefix-enforced external store.
- **Validation:** `harness.rs:668–692`; cross-tenant negative assertion in test 02.
- **Coordination/evidence:** API-008, INV-004; no runtime proof. [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Audit rows to export result
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-export-output-001`; selected local audit rows → returned NDJSON/manifest.
- **Directions:** dependency N/A; data state → caller result; impact selection/serialization change → test 03.
- **Surface/activation:** `export_audit_window`, valid half-open window, locally provisioned slug.
- **Contract/state:** reads rows before start marker with timestamps in `[start,end)`; builds body/digest/manifest.
- **Failure/propagation:** invalid/missing state or serialization failure; no output delivery or rollback claim.
- **Containment:** local return value only.
- **Validation:** `harness.rs:717–803`; tamper/window/manifest assertions REL-015.
- **Coordination/evidence:** API-009, INV-005; local source. [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Caller-supplied export verification
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-verify-001`; body/manifest caller → `verify_audit_export`.
- **Directions:** dependency caller → method; data caller bytes/manifest → verifier; impact format change → test 03.
- **Surface/activation:** any byte body and manifest supplied to public method.
- **Contract/state:** recomputes body digest, parses lines, checks count and adjacent links; no state mutation.
- **Failure/propagation:** digest/count/chain/parse error; no recomputation of each row hash.
- **Containment:** verifier return only; no export persisted.
- **Validation:** `harness.rs:805–846`; REL-015 assertions.
- **Coordination/evidence:** API-010, INV-006; source/test text only. [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — DSR request state
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-dsr-request-001`; `request_dsr_erasure` → local request set/time and audit vector.
- **Directions:** dependency N/A; data request ID → local state/receipt; impact state/API change → tests 04/05.
- **Surface/activation:** existing local lifecycle and unique request ID.
- **Contract/state:** audit emission precedes ID/time insert; deadline is fixed time + seven days.
- **Failure/propagation:** missing tenant, duplicate ID, or local audit/lock error.
- **Containment:** instance-local state; no worker/queue/provider.
- **Validation:** `harness_lifecycle.rs:17–56`; REL-016/017 negative cases.
- **Coordination/evidence:** API-011, INV-007; source assertions only. [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — DSR finalization state
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-dsr-finalize-001`; `finalise_dsr_erasure` → local CAS/audit state.
- **Directions:** dependency N/A; local state → cleared state/tombstone; impact drain/order change → tests 04/05.
- **Surface/activation:** existing lifecycle/request; caller `now_ms` ≥ request + seven days.
- **Contract/state:** emits completion, clears CAS/prior rows, reanchors completion row.
- **Failure/propagation:** no request/missing tenant or open SLA; invariant failures may interrupt mutation.
- **Containment:** in-memory only; no multi-backend completion.
- **Validation:** `harness_lifecycle.rs:58–128`; early/success/post-DSR assertions.
- **Coordination/evidence:** API-012, INV-007; no provider result. [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Subscription cancellation state
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-cancel-001`; `cancel_subscription` → local subscription/lifecycle/time and audit.
- **Directions:** dependency N/A; method → local state; impact transition change → test 05.
- **Surface/activation:** local lifecycle exists; source does not require Active state.
- **Contract/state:** emits event, sets Cancelled and fixed-time 30-day grace.
- **Failure/propagation:** unprovisioned or local audit/lock failure; no billing rollback.
- **Containment:** local harness; provider cancellation not established.
- **Validation:** `harness_lifecycle.rs:142–170`; REL-017.
- **Coordination/evidence:** API-013, INV-008; source/test text only. [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Offboarding completion state
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-offboard-001`; `complete_offboarding` → local CAS/audit/lifecycle.
- **Directions:** dependency N/A; method → local cleared state/receipt; impact guard/clear change → test 05.
- **Surface/activation:** local tenant + cancelled time + caller time after 30-day grace.
- **Contract/state:** emits event, clears both maps, sets HardDeleted, checks residuals.
- **Failure/propagation:** missing/cancel/grace/residual or invariant; partial effects not ruled out.
- **Containment:** local state only; no external audit sink field.
- **Validation:** `harness_lifecycle.rs:172–235`; no-cancel/early/post-DSR tests.
- **Coordination/evidence:** API-014, INV-008; external deletion unknown. [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — Signup scenario target
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-test-signup-001`; `test_01_tenant_signup.rs` → exported harness API.
- **Directions:** dependency test consumer → package provider; data fixtures/state → assertions; impact API change → this target.
- **Surface/activation:** manifest `test_01_tenant_signup` path; imports and calls `complete_signup`.
- **Contract/state:** asserts initial/final state, four event kinds/order/hash linkage, duplicate rejection.
- **Failure/propagation:** assertion/source drift may reduce local detection; no test result inferred.
- **Containment:** scenario only; no production signup path.
- **Validation:** manifest lines 21–22; test source; run not observed.
- **Coordination/evidence:** API-006/015; source assertions only. [Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — First-upload scenario target
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-test-upload-001`; `test_02_first_cas_upload.rs` → exported harness API.
- **Directions:** dependency test consumer → package provider; data fixtures → local receipt/map; impact API change → this target.
- **Surface/activation:** manifest target path; calls signup, upload, count, read.
- **Contract/state:** asserts 100 payload digest/round-trip, receipt prefix, event sequence; rejects pre-signup upload and cross-tenant read.
- **Failure/propagation:** source assertion drift affects local coverage only; test not run.
- **Containment:** prefix string and slug maps; no R2 proof.
- **Validation:** manifest lines 24–25; `test_02` source.
- **Coordination/evidence:** API-006–008/015; no ordering assertion. [Relation index](#b03)

<a id="rel-015"></a>
### REL-015 — Audit-export scenario target
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-test-export-001`; `test_03_audit_export_roundtrip.rs` → exported harness API.
- **Directions:** dependency test consumer → package provider; data rows → body/manifest/assertions; impact method/schema change → this target.
- **Surface/activation:** manifest target path; calls signup/upload/export/verify.
- **Contract/state:** asserts row count, body digest, adjacent links; tampers body and supplies invalid window.
- **Failure/propagation:** assertions do not recompute row hashes or prove external audit.
- **Containment:** local output bytes only; not a delivery route.
- **Validation:** manifest lines 27–28; `test_03` source; run not observed.
- **Coordination/evidence:** API-006/007/009/010/015; source assertions only. [Relation index](#b03)

<a id="rel-016"></a>
### REL-016 — Erasure scenario target
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-test-dsr-001`; `test_04_dsr_erasure.rs` → exported harness API.
- **Directions:** dependency test consumer → package provider; data local state → tombstone assertions; impact DSR method change → this target.
- **Surface/activation:** manifest target path; calls signup/upload/request/finalize.
- **Contract/state:** asserts duplicate request, no-signup, early deadline, CAS drain, one reanchored tombstone.
- **Failure/propagation:** source test shape only; no compliance or external erase.
- **Containment:** local harness instance.
- **Validation:** manifest lines 30–31; `test_04` source; not run.
- **Coordination/evidence:** API-006/007/011/012/015; no runtime result. [Relation index](#b03)

<a id="rel-017"></a>
### REL-017 — Offboarding scenario target
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-test-offboard-001`; `test_05_tenant_offboarding.rs` → exported harness API.
- **Directions:** dependency test consumer → package provider; data local state → receipt/assertions; impact lifecycle change → this target.
- **Surface/activation:** manifest target path; calls signup/upload/cancel/offboard and post-DSR path.
- **Contract/state:** asserts no-cancel and early-grace errors, zero residual, HardDeleted.
- **Failure/propagation:** test text cannot prove subscription cancellation or deletion.
- **Containment:** scenario-only, local memory.
- **Validation:** manifest lines 33–34; `test_05` source; not run.
- **Coordination/evidence:** API-006/007/011–015; no external consumer inferred. [Relation index](#b03)

<a id="rel-018"></a>
### REL-018 — B-269 module-path binding check
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-b269-binding-001`; B-269 static verifier → `harness.rs`.
- **Directions:** dependency N/A; data source text → verifier predicates; impact source drift → verifier result if invoked.
- **Surface/activation:** exactly one active lifecycle path binding is required.
- **Contract/state:** static source check; lifecycle method markers are separate REL-040.
- **Failure/propagation:** missing/wrong marker raises `VerificationError`; not run here.
- **Containment:** static text; not Rust compilation/runtime.
- **Validation:** script lines 1–51; verifier test REL-019.
- **Coordination/evidence:** source/tool evidence only; no approval. [Relation index](#b03)

<a id="rel-019"></a>
### REL-019 — B-269 verifier tests
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-b269-test-001`; `tests/test_verify_b269_pilot_harness_module_path.py` → imported B-269 verifier.
- **Directions:** dependency test consumer → script module; data source overrides → assertions; impact checker change → test file.
- **Surface/activation:** Python imports verifier and declares valid/mutation cases.
- **Contract/state:** tests path removal/rename/comment and missing method mutations.
- **Failure/propagation:** catches checker contract regressions only if run; no run observed.
- **Containment:** static checker tests; not source runtime.
- **Validation:** test file lines 1–51; no Python command executed.
- **Coordination/evidence:** source-only known consumer, not UNKNOWN. [Relation index](#b03)

<a id="rel-020"></a>
### REL-020 — B-126 parent wiring verifier
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-b126-parent-001`; B-126 static verifier → `harness.rs`.
- **Directions:** dependency N/A; data source text/population → verifier; impact split/method changes → verifier output if run.
- **Surface/activation:** split spec declares `harness.rs` parent and lifecycle sibling.
- **Contract/state:** static parent/path wiring check; lifecycle population is REL-041.
- **Failure/propagation:** changed population/binding can report error; not invoked here.
- **Containment:** static source census.
- **Validation:** split spec lines 112–121 and verifier source; test REL-021.
- **Coordination/evidence:** no Cargo/test result; known repository consumer. [Relation index](#b03)

<a id="rel-021"></a>
### REL-021 — B-126 verifier tests
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-b126-test-001`; `tests/test_b126_t3_refactor.py` → imported split verifier.
- **Directions:** dependency test consumer → script; data mutations → assertions; impact verifier contract → this test.
- **Surface/activation:** Python imports module; regression/mutation cases include the lifecycle split.
- **Contract/state:** checks missing wiring/fragment and method population in source fixture.
- **Failure/propagation:** results exist only when test executed; not run here.
- **Containment:** static source verifier suite.
- **Validation:** test file lines 1–83; no Python command executed.
- **Coordination/evidence:** source-only known consumer, not UNKNOWN. [Relation index](#b03)

<a id="rel-022"></a>
### REL-022 — Nightly workflow runner selection
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-nightly-script-001`; `.github/workflows/nightly.yml` → `scripts/ci-bounded-workspace-tests.sh`.
- **Directions:** dependency workflow → script; data checkout/environment → runner; impact workflow/script change → scheduled/manual lane.
- **Surface/activation:** workflow schedule or `workflow_dispatch`; job runs bash script.
- **Contract/state:** workflow source declares bounded workspace test job; no invocation inferred.
- **Failure/propagation:** missing/failed script can fail job if activated; runtime unknown.
- **Containment:** GitHub workflow boundary; none executed.
- **Validation:** workflow lines 223–228 and script source; static only.
- **Coordination/evidence:** CI/operator owner not verified. [Relation index](#b03)

<a id="rel-023"></a>
### REL-023 — Bounded script workspace selection
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-workspace-tests-001`; bounded script → workspace all-target test command.
- **Directions:** dependency script → workspace packages; data test binaries → runner; impact package source → potential command result.
- **Surface/activation:** when script runs, nextest available selects workspace/all-targets; otherwise cargo test fallback.
- **Contract/state:** both branches are execution commands, resource/time bounded; package is a workspace member.
- **Failure/propagation:** command failures/timeouts affect job; actual selection/result unknown.
- **Containment:** no package-specific result claimed.
- **Validation:** `scripts/ci-bounded-workspace-tests.sh:1–45`; never run in this work.
- **Coordination/evidence:** REL-001 + REL-022; declared command only. [Relation index](#b03)

<a id="rel-024"></a>
### REL-024 — CAS Foundation workspace lane
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-cas-workspace-001`; `.github/workflows/cas_foundation.yml` → package through workspace commands.
- **Directions:** dependency workflow → workspace target set; data test/build results → job; impact source → potential gate result.
- **Surface/activation:** active `workflow_dispatch`; weekly schedule is commented out; workspace build/test step names all targets.
- **Contract/state:** commands are declared; no package selection/result observed.
- **Failure/propagation:** if dispatched, source/build failure may fail job.
- **Containment:** workflow scope and dispatch gate; none dispatched.
- **Validation:** workflow `on` block and workspace-build-test steps.
- **Coordination/evidence:** workflow ops owner unknown; declaration only. [Relation index](#b03)

<a id="rel-025"></a>
### REL-025 — Workspace lint lane
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-workspace-lint-001`; `.github/workflows/workspace-lint.yml` → Rust workspace targets.
- **Directions:** dependency workflow → workspace clippy targets; data diagnostics → job; impact Rust/manifest changes → potential lint result.
- **Surface/activation:** push to main matching Rust/Cargo paths, workflow-file PR, or dispatch.
- **Contract/state:** `cargo clippy --workspace --all-targets`; no command result observed.
- **Failure/propagation:** compile/lint issue can fail lane when triggered.
- **Containment:** no package-specific result inferred.
- **Validation:** workflow source lines 1–73; not invoked.
- **Coordination/evidence:** CI owner not verified; command text only. [Relation index](#b03)

<a id="rel-026"></a>
### REL-026 — CodeQL workspace build lane
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-codeql-workspace-001`; `.github/workflows/codeql.yml` → Rust workspace targets.
- **Directions:** dependency workflow → workspace build selection; data build/analysis output → job; impact Rust source → potential analysis result.
- **Surface/activation:** daily schedule or protected-main dispatch; Rust matrix uses workspace/all-targets/locked build.
- **Contract/state:** code source declares tracing build; no result or package target resolution observed.
- **Failure/propagation:** build/analysis failures affect workflow if activated.
- **Containment:** no live provider or deploy action established for this package.
- **Validation:** workflow trigger and Rust build step; not invoked.
- **Coordination/evidence:** security/CI owner unverified; declaration only. [Relation index](#b03)

<a id="rel-027"></a>
### REL-027 — Draft runbook reference
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-runbook-doc-001`; `RB-PILOT-ONBOARDING-E2E.md` → package procedures/claims.
- **Directions:** dependency N/A; data prose → reader; impact prose can mislead future operator.
- **Surface/activation:** document is marked DRAFT; includes Cargo commands and audit-order assertions.
- **Contract/state:** documentary evidence only, not executable relation or run result.
- **Failure/propagation:** claims conflict with source on audit ordering and are not runtime truth.
- **Containment:** keep conflict explicit; do not run commands under static-only scope.
- **Validation:** runbook lines 1–168 against methods/tests; no execution.
- **Coordination/evidence:** ops ownership not verified. [Relation index](#b03)

<a id="rel-028"></a>
### REL-028 — Backlog B-269 reference
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-backlog-269-001`; `BACKLOG.md` B-269 → module-path package source.
- **Directions:** dependency N/A; data locator/verification prose → reader; impact source locator drift → backlog accuracy.
- **Surface/activation:** item identifies path module and lists Cargo/verifier evidence commands.
- **Contract/state:** backlog text is historical/management evidence, not a command result here.
- **Failure/propagation:** stale locator can misdirect review; no runtime call.
- **Containment:** compare current source before relying on prose.
- **Validation:** `BACKLOG.md:908–932`; not executed.
- **Coordination/evidence:** backlog owner not verified. [Relation index](#b03)

<a id="rel-029"></a>
### REL-029 — Journey gapmap reference
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-gapmap-journeys-001`; journey gapmap → package scenario interpretation.
- **Directions:** dependency N/A; data prose → coverage reader; impact wording can overstate composition.
- **Surface/activation:** describes in-memory D1/R2 and internal crates.
- **Contract/state:** documentary statement conflicts with no direct first-party dependency and source-local maps.
- **Failure/propagation:** unsupported composition narrative could misclassify evidence.
- **Containment:** reference only as contradictory prose; source/manifests govern static facts.
- **Validation:** `docs/testing/2026-06-23-gapmap-journeys.md:14,19–22`.
- **Coordination/evidence:** no runtime finding inferred. [Relation index](#b03)

<a id="rel-030"></a>
### REL-030 — Surface gapmap reference
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-gapmap-surfaces-001`; surface gapmap → package coverage interpretation.
- **Directions:** dependency N/A; data prose → coverage reader; impact wording can overstate R2 reach.
- **Surface/activation:** labels scenario as internal/R2-direct.
- **Contract/state:** source map is slug/digest keyed; no actual R2 connection established.
- **Failure/propagation:** unsupported assertion may distort coverage classification.
- **Containment:** record conflict; do not infer implementation or runtime from it.
- **Validation:** `docs/testing/2026-06-23-gapmap-surfaces.md:15,128`.
- **Coordination/evidence:** source and manifest only. [Relation index](#b03)

<a id="rel-031"></a>
### REL-031 — Quality gapmap reference
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-gapmap-quality-001`; quality gapmap → package audit/CAS interpretation.
- **Directions:** dependency N/A; data prose → reader; impact prose affects evidence classification.
- **Surface/activation:** describes an in-memory R2 characterization.
- **Contract/state:** prose does not alter source; concrete external backend claim is unsupported.
- **Failure/propagation:** treating it as runtime fact is a false-positive relation.
- **Containment:** documentary contradiction only.
- **Validation:** `docs/testing/2026-06-23-gapmap-quality.md:283–291`.
- **Coordination/evidence:** not runtime evidence. [Relation index](#b03)

<a id="rel-032"></a>
### REL-032 — Ownership campaign inventory
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-ownership-index-001`; WAVE_014_PLAN, CARGO_CENSUS, CAMPAIGN_STATUS, ROLLOUT → package identity/planning.
- **Directions:** dependency N/A; data package locator → campaign reader; impact path/name changes → inventory accuracy.
- **Surface/activation:** plan/census/index mentions manifest, profile, package or wave.
- **Contract/state:** documentary identity/planning only, not consumer or runtime edges.
- **Failure/propagation:** stale inventory can route work incorrectly.
- **Containment:** update only through campaign integration owner.
- **Validation:** four named docs; static content.
- **Coordination/evidence:** package identity agrees with manifest; no execution evidence. [Relation index](#b03)

<a id="rel-033"></a>
### REL-033 — Billing-flow coverage reference
- **Fingerprint/endpoints:** shared stable ID `repo:1232040291:relation:e2e-pilot-onboarding-to-e2e-billing-flow-coverage-008` (peer record [REL-E2E-008](../e2e-billing-flow/BLAST_RADIUS.md#rel-e2e-008)); `tests/e2e-pilot-onboarding/src/lib.rs` → peer coverage record.
- **Type/directions:** documentation only; dependency none. The onboarding crate root says `e2e-billing-flow` pins subsystem internals while onboarding pins cross-subsystem journey shape. No call/runtime edge and neither relation proves test execution.
- **Propagation/validation:** either endpoint or the shared coverage claim changes, reconcile both records. Effect stops at documentary readers/reconcilers of this coverage map. Source `lib.rs` comment; peer REL-E2E-008.
- **Ownership:** pilot package owner `UNASSIGNED`; peer doc owner not established here. Ask repository administrator to assign source owner; route peer edit to that package's assigned owner. CODEOWNERS only requests review. Do not edit peer artifact here. [Relation index](#b03)

<a id="rel-034"></a>
### REL-034 — `blake3` normal dependency
- **Identity/endpoints:** `repo:1232040291:dependency:e2e-pilot-onboarding-blake3-001`; package → workspace dependency alias `blake3`.
- **Directions:** dependency consumer → provider; data bytes → digest/hash; impact API/version change → hash outputs/build.
- **Surface/activation:** normal manifest entry; fixture, CAS digest, audit chain, export.
- **Contract/state:** exact resolved version/features not established.
- **Failure/propagation:** API/resolution/build incompatibility may affect methods.
- **Containment:** stops at dependency interface; no transitive graph.
- **Validation:** manifest lines 8–12 and source uses; no resolve/build.
- **Coordination/evidence:** upstream owner/version unknown. [Relation index](#b03)

<a id="rel-035"></a>
### REL-035 — `hex` normal dependency
- **Identity/endpoints:** `repo:1232040291:dependency:e2e-pilot-onboarding-hex-001`; package → workspace dependency alias `hex`.
- **Directions:** dependency consumer → provider; data encoded digest/hash ↔ bytes; impact version/API → tombstone parsing/build.
- **Surface/activation:** normal manifest entry; lifecycle tombstone decode/encode code.
- **Contract/state:** resolved version/features not established.
- **Failure/propagation:** unavailable/changed API affects source build or conversion.
- **Containment:** no external call.
- **Validation:** manifest lines 8–12; lifecycle source; no resolve/build.
- **Coordination/evidence:** upstream owner/version unknown. [Relation index](#b03)

<a id="rel-036"></a>
### REL-036 — `serde` normal dependency
- **Identity/endpoints:** `repo:1232040291:dependency:e2e-pilot-onboarding-serde-001`; package → workspace dependency alias `serde`.
- **Directions:** dependency consumer → provider; data derive/serialization types ↔ package records; impact API/version → wire-form source behavior.
- **Surface/activation:** normal manifest entry; public audit/export derives.
- **Contract/state:** resolved version/features not established.
- **Failure/propagation:** derive/trait incompatibility can break source compile.
- **Containment:** package-local serialized model; no network/wire consumer established.
- **Validation:** manifest lines 8–12 and `harness.rs`; no build.
- **Coordination/evidence:** upstream owner/version unknown. [Relation index](#b03)

<a id="rel-037"></a>
### REL-037 — `serde_json` normal dependency
- **Identity/endpoints:** `repo:1232040291:dependency:e2e-pilot-onboarding-serde-json-001`; package → workspace dependency alias `serde_json`.
- **Directions:** dependency consumer → provider; data JSON ↔ audit detail/body; impact version/API → export/hash outputs.
- **Surface/activation:** normal manifest entry; local audit append, NDJSON, tombstone recomposition.
- **Contract/state:** resolved version/features not established.
- **Failure/propagation:** serialization/deserialization returns local invariant errors on selected paths.
- **Containment:** no delivery or external protocol.
- **Validation:** manifest lines 8–12 and source; no resolve/build.
- **Coordination/evidence:** upstream owner/version unknown. [Relation index](#b03)

<a id="rel-038"></a>
### REL-038 — `thiserror` normal dependency
- **Identity/endpoints:** `repo:1232040291:dependency:e2e-pilot-onboarding-thiserror-001`; package → workspace dependency alias `thiserror`.
- **Directions:** dependency consumer → provider; data error variants → display/convert API; impact derive/API change → callers.
- **Surface/activation:** normal manifest entry; public error enums and conversion variants.
- **Contract/state:** exact resolved version/features not established.
- **Failure/propagation:** derive incompatibility or error API change affects compile/callers.
- **Containment:** errors remain Rust values; no telemetry sink.
- **Validation:** manifest lines 8–12 and error definitions; no build.
- **Coordination/evidence:** upstream owner/version unknown. [Relation index](#b03)

<a id="rel-039"></a>
### REL-039 — Audit export marker writes
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-export-markers-001`; `export_audit_window` → local `TenantState.audit_rows`.
- **Directions:** dependency N/A; data method → audit vector; impact marker-emission change → export sequence assertions.
- **Surface/activation:** valid local export call; emits `AuditExportStarted` and `AuditExportSealed`.
- **Contract/state:** marker events share local `emit_audit`; output selection is separate REL-007.
- **Failure/propagation:** audit/lock failure may leave earlier marker; rollback is not established.
- **Containment:** per-slug in-memory vector only.
- **Validation:** `harness.rs:717–803`; test 03 asserts final sequence/count.
- **Coordination/evidence:** API-009, INV-005; no external delivery. [Relation index](#b03)

<a id="rel-040"></a>
### REL-040 — B-269 lifecycle method marker check
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-b269-lifecycle-001`; B-269 static verifier → `harness_lifecycle.rs`.
- **Directions:** dependency N/A; source text → marker checks; impact method spelling/count → verifier result if run.
- **Surface/activation:** requires request, finalise, cancel markers; does not require offboarding completion.
- **Contract/state:** static evidence only; no invocation or Rust runtime.
- **Failure/propagation:** missing required marker raises checker error.
- **Containment:** lifecycle source only; population limitation remains explicit.
- **Validation:** script lines 29–42; test REL-019; neither run here.
- **Coordination/evidence:** known local verifier consumer. [Relation index](#b03)

<a id="rel-041"></a>
### REL-041 — B-126 lifecycle population check
- **Identity/endpoints:** `repo:1232040291:boundary:e2e-pilot-onboarding-b126-lifecycle-001`; B-126 static verifier → `harness_lifecycle.rs`.
- **Directions:** dependency N/A; method text population → verifier; impact method set → verifier/test result.
- **Surface/activation:** split spec requires request, finalise, cancel, complete-offboarding methods.
- **Contract/state:** static inventory only; no runtime effect or target selection.
- **Failure/propagation:** changed method population can report mismatch if invoked.
- **Containment:** source checker and its tests.
- **Validation:** verifier split spec/population check; test REL-021; not run here.
- **Coordination/evidence:** source-only known consumer. [Relation index](#b03)

<a id="b04"></a>
## B04 — Transitive propagation

| Destination | Witness path | Condition | Causal effect | Containment / validation |
|---|---|---|---|---|
| Five scenario sources | REL-002 → REL-013–017 | target source imports root aliases | public symbol/type changes can break local assertions | inspect every target; compile/result unknown |
| All five scenarios | REL-004 → REL-013–017 | tests call signup helper | state/order change propagates to local journey setup | compare each scenario and INV-003 |
| Tests 02–05 | REL-005 → REL-014–017 | tests create local blobs | CAS key/receipt/state change propagates to upload assertions | method-specific negatives, no external R2 |
| Test 02 | REL-006 → REL-014 | target reads own and other tenant digest | lookup/error change impacts local isolation assertions | cross-tenant case in source |
| Test 03 | REL-007/008/039 → REL-015 | target exports and verifies body | filter/markers/manifest/verifier changes alter coverage | tamper/window source cases |
| Tests 04–05 | REL-009/010 → REL-016/017 | DSR calls, including post-DSR offboarding | request/deadline/drain changes propagate to assertions | duplicate/no-signup/early/success source cases |
| Test 05 | REL-011/012 → REL-017 | cancel/grace/offboard path | guard/clear changes alter local residual assertions | no-cancel/early/post-DSR cases |
| Static verifier inputs | REL-003 → REL-018–021/040/041 | module path or lifecycle method population changes | verifier may report source drift; test outcome unknown | check script/test source, not Rust runtime |
| Workspace test lane | REL-001 → REL-023–024 | workflow selects workspace targets | package may be in global command's target population | target resolution/actual invocation unknown |
| Workspace lint/build | REL-001 → REL-025/026 | workflow selects all targets | package source may affect global gate | static workflow only; no result |
| Narrative consumers | REL-027–033 | docs are read or updated | unsupported prose may skew evidence/coverage interpretation | reconcile to source, no executable propagation |
| Dependency providers | REL-034–038 | dependency resolution/build | API/version changes may affect hashes, serialization, errors | version/resolution and inverse consumers unknown |

**Reached internal nodes:** crate root, harness module, lifecycle module, local tenant
map, audit rows, five target sources, two verifier modules, CI runner/workspace
commands, and documentary consumers. **Alternative material paths:** dependency
resolution and outside-repository consumers are not established. The first
path-witnesses show potential static propagation only; no runtime observation.

<a id="b05"></a>
## B05 — Change → impact → validation

| Change | API/INV/REL | Consumers/state | Required validation | Coordination/recovery |
|---|---|---|---|---|
| Public export/type/signature | API-001–016; REL-002, REL-013–017 | every target import/call | inspect all target uses; future isolated compile if authorized | preserve public compatibility; no consumer graph completeness |
| Signup state/event code | API-006; INV-003; REL-004, REL-013–017 | five target setup paths, local state/audit | assert final state, event set/order, duplicate negative; test operation ordering separately if required | align comments and runbook evidence; no production signup route |
| Upload/read/keying | API-007/008; INV-004; REL-005/006, REL-014–017 | tests 02–05; local CAS map | digest, mismatch/eligibility, own/cross-slug/missing cases and audit effects | receipt prefix is text; do not claim storage prefix |
| Export/verify | API-009/010; INV-005/006; REL-007/008/039, REL-015 | test 03; local body/manifest/markers | valid, tamper, invalid-window; add checks for unverified fields only by request | no external audit delivery claim |
| DSR methods | API-011/012; INV-007; REL-009/010, REL-016/017 | tests 04/05; local map/tombstone | duplicate, missing, early, exact deadline, success and post-DSR path | no legal/provider owner inferred; route canonical policy via OKF |
| Cancel/offboarding | API-013/014; INV-008; REL-011/012, REL-017 | test 05; local grace/residuals | no-cancel, early, success, zero residual | no provider cancel/delete/sink; external recovery unknown |
| Target declaration/source | REL-013–017 plus affected method RELs | one selected scenario | reconcile manifest path and all API calls/assertions | target selection unobserved |
| Verifier/workflow/doc text | REL-018–033/040/041 | static gates and documentary readers | verify relationship type and exact source claim; never treat prose as execution | update peer doc only under its owner |
| Dependency declaration | REL-034–038 | package compile/resolution | package/target resolution and tests only in authorized later task | no version update or lockfile edit in static ownership work |

Validation commands/results remain UNKNOWN unless a future authorized procedure
records them. Current work uses the documentary checks in M03 only.

<a id="b06"></a>
## B06 — Coverage and unknowns

| Population | Discovered | Documented as relation | Excluded with reason | Unknown |
|---|---:|---:|---:|---:|
| Package manifest/source/targets (manifest/path enumeration) | 9 | 9 | 0 | 0 within tracked package |
| Outside-package exact-token paths | 18 | 13 | 5 | 0 within literal result set |
| Verifier-test import consumers | 2 | 2 | 0 | 0 repository-local imports |
| Workspace command workflows | 4 | 4 | 0 | 0 inspected workflow paths |
| Direct first-party Cargo dependencies | 0 | 0 | 0 | 0 declarations |
| Resolved Cargo target/dependency graph | 0 | 0 | 0 | 1 uncollected population |
| Runtime/external/other-repository consumers beyond known in-repo docs | 0 | 0 | 0 | 1 unbounded population |

That exact path-only query returned 20 tracked paths: package hits
`tests/e2e-pilot-onboarding/Cargo.toml` and `tests/e2e-pilot-onboarding/src/lib.rs`,
plus 18 outside-package hits.

**Thirteen outside hits are documented above:** root `Cargo.toml`, `BACKLOG.md`, `docs/ownership/CAMPAIGN_STATUS.md`,
`CARGO_CENSUS.md`, `ROLLOUT.md`, `WAVE_014_PLAN.md`, peer billing blast, three
gapmaps, runbook, and two verifier scripts. **Five are excluded from consumer-edge
claims:** `Cargo.lock` (no resolver result), LOC report (not a contract), two sealed
historical audits (not current graph evidence), and `.sbom/cyclonedx-rust.json`
(component + Cargo PURL inventory metadata, not a consumer/call edge). The SBOM is
discovered and documented as inventory, but excluded from executable impact graph.

**Unsupported prose:** the gapmaps' internal-crate/R2 descriptions and audit-before-
every-mutation wording are contradicted or unsupported by the manifest/source.
Source order specifically mutates signup PendingActivation before checkout audit
and inserts each upload into CAS before its BlobPutCommitted row. Tests assert final
state/event order, not operation order. Keep this contradiction visible.

Unknown populations are not declared complete. Full external inverses, Cargo
resolution, CI invocation/results, production composition/runtime, provider state,
durability, auth, and operational recovery remain UNKNOWN. Equal counts do not prove
search completeness.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership skill](../../../../.claude/skills/own-e2e-pilot-onboarding/SKILL.md#s01)
