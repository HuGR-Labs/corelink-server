---
schema: corelink-ownership/1.1
document: maintenance
package: e2e-resilience
manifest: tests/e2e-resilience/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w015-e2e-resilience-source-1177dad2
---

# e2e-resilience — maintenance

Procedures below separate documentary validation from package execution. The current ownership task ran only structural-document checks; no Cargo or Rust command was run.

[Preparation](#m01) · [Select](#m02) · [Procedures](#m03) ·
[Change matrix](#m04) · [Recovery](#m05) · [Escalation and record](#m06).

<a id="m01"></a>
## M01 — Safe preparation

**Source and checkout:** pin `1177dad2ca2a9f21c29b5a118aa7944b77147798`; verify `[package].name`, manifest, root workspace membership, and assigned diff scope before relying on this manual. The package-local commands below assume repository root and a pinned Cargo lockfile.

**Environment:** package has no declared feature; native target support and resolved versions were not inspected. Do not add feature flags by inference. For the v1.3 candidate check, set `OWNERSHIP_CHECKER` to the lead-supplied, hash-verified `tools/check_docs.py` path; do not persist a temporary extraction path in repository policy.

**Limits:** never put secrets or customer data in fixtures. Do not execute live providers, CI dispatch, deployments, or operational Tower paths as a package test. Stop if an offline command needs network/cache mutation, the source pin drifts, or the selected target is unknown.

<a id="m02"></a>
## M02 — Procedure selection

| Change or question | Procedure | Mode | Scope |
|---|---|---|---|
| Edit these four ownership artifacts | [PROC-001](#proc-001) | `LOCAL_ISOLATED` | Structural checks and whitespace only |
| Change package dependency, `Cargo.lock`, target, or workspace declaration | [PROC-003](#proc-003) then re-read [R06](REFERENCE.md#r06) and affected [RELs](BLAST_RADIUS.md#b03) | `READ_ONLY` | Manifest/lockfile facts; resolved graph remains UNKNOWN |
| Change or select a package feature | [PROC-004](#proc-004) and the feature row in [M04](#m04) | `READ_ONLY` | This manifest declares no package features; stop if a new feature is proposed |
| Change package library or `scenarios` source | [PROC-002](#proc-002) | `LOCAL_ISOLATED` | Offline package test; not run in W015 authoring |
| Change a direct upstream API or production wiring | Stop and route through [B03](BLAST_RADIUS.md#b03) | Review only until owner/target known | This package cannot validate production behavior |

<a id="m03"></a>
## M03 — Procedures

**Index:** [PROC-001](#proc-001) · [PROC-002](#proc-002) · [PROC-003](#proc-003) · [PROC-004](#proc-004) · [PROC-005](#proc-005)

<a id="proc-001"></a>
### PROC-001 — Validate ownership documents

**Trigger / predicate:** one or more of the four owned files change; each passes its S-profile structural checker and the scoped whitespace check. **Mode / environment / permission:** local static files only; repository root; checker path supplied per run. **Inputs / preconditions:** verify the exact four paths and pinned source before checking.

1. Run `python "$OWNERSHIP_CHECKER" .claude/skills/own-e2e-resilience/SKILL.md --kind skill --profile S --root .`.
2. Run the same command for `REFERENCE.md --kind reference`, `BLAST_RADIUS.md --kind blast_radius`, and `MAINTENANCE.md --kind maintenance` under `docs/ownership/crates/e2e-resilience/`.
3. Run `git diff --check 1177dad2ca2a9f21c29b5a118aa7944b77147798 -- .claude/skills/own-e2e-resilience/SKILL.md docs/ownership/crates/e2e-resilience/REFERENCE.md docs/ownership/crates/e2e-resilience/BLAST_RADIUS.md docs/ownership/crates/e2e-resilience/MAINTENANCE.md`.

**Stop / recovery:** fix the relevant artifact and rerun all four checks after any byte change; do not bypass a failure. **review_status:** `BLOCKED` pending independent cold review. **execution_status:** `EXECUTED_LOCAL`; **result:** `PASS` only when all outputs pass. **required_for_acceptance:** `true`. Structural success is not semantic approval. Evidence: final checker output and scoped diff check.

[Procedure index](#m03)

<a id="proc-002"></a>

### PROC-002 — Validate the declared scenarios after a Rust change

**Trigger / predicate:** package source, dependency, or scenario changes; the explicit `scenarios` target passes from a clean, pinned checkout. **Mode / environment / permission:** local offline Cargo; no providers or live services. **Inputs / preconditions:** manifest and `Cargo.lock` match the reviewed baseline; the required dependencies are already available offline.

1. Run `cargo test --locked --offline --package e2e-resilience --test scenarios` from the repository root.
2. Preserve the full command and result; expect the declared target to complete successfully.

**Stop / recovery:** on missing offline dependencies or changed lock/source, stop as `BLOCKED_ENVIRONMENT`; do not fetch or mutate the lockfile. Restore only the reviewed source change through the repository's normal change process, then rerun this procedure in an authorized environment. **review_status:** `BLOCKED` pending independent review. **execution_status:** `REVIEWED_NOT_EXECUTED`; **result:** `NOT_EXECUTED` in W015. **required_for_acceptance:** `true` for a Rust target change; not an assertion that it ran here. Evidence needed: command output and exact source/lock baseline.

[Procedure index](#m03)

<a id="proc-003"></a>

### PROC-003 — Reconcile dependency, target, and lockfile declarations

**Trigger / predicate:** a manifest, workspace membership, target, direct dependency, or `Cargo.lock` change is proposed. Pass when the pinned source records the exact package/target/dependency declarations and the lockfile entries are captured without claiming resolution or execution. **Mode:** `READ_ONLY`; no Cargo command, network, lockfile mutation, or external service. **review_status:** `BLOCKED` pending cold review; **execution_status:** `EXECUTED_LOCAL`; **result:** `PASS` only when the static census is complete.

1. Run `git show 1177dad2ca2a9f21c29b5a118aa7944b77147798:tests/e2e-resilience/Cargo.toml` and record package, target, dependency class, and feature facts.
2. Run `git show 1177dad2ca2a9f21c29b5a118aa7944b77147798:Cargo.lock` and inspect the `e2e-resilience`, `corelink-ratelimit`, `corelink-rate-headers`, `uuid`, and `thiserror` package entries.
3. Run `rg -n 'corelink-ratelimit|corelink-rate-headers|e2e-resilience' --glob 'Cargo.toml'` and reconcile direct/inverse declarations with [BLAST B02–B03](BLAST_RADIUS.md#b02).
4. Preserve the exact outputs and mark resolved target/features/runtime reachability `UNKNOWN`.

**Stop / recovery:** stop on source drift, missing lock entry, or a request for actual resolution; route that request to an authorized Cargo-validation task. Do not edit manifests or lockfiles in this procedure. **required_for_acceptance:** `true` for manifest/dependency/target/lockfile claims. Evidence: baseline SHA, command output, affected REL IDs, and unknowns.

[Procedure index](#m03)

<a id="proc-004"></a>

### PROC-004 — Reconcile feature and required-target declarations

**Trigger / predicate:** a package feature or target `required-features` claim changes. Pass when the manifest and explicit `scenarios` target are inspected and the absence/presence of feature gates is recorded exactly. **Mode:** `READ_ONLY`; no Cargo feature selection, build, network, or lockfile mutation. **review_status:** `BLOCKED` pending cold review; **execution_status:** `EXECUTED_LOCAL`; **result:** `PASS` only when the static declaration is captured.

1. Run `git show 1177dad2ca2a9f21c29b5a118aa7944b77147798:tests/e2e-resilience/Cargo.toml`.
2. Run `rg -n '^\[features\]|required-features|^\[\[test\]\]|^name = "scenarios"|^path = "tests/scenarios.rs"' tests/e2e-resilience/Cargo.toml`.
3. Record that no package `[features]` or target `required-features` declaration is present at the source pin; do not infer selection from `--all-features`.

**Stop / recovery:** stop if a feature is requested but the authoritative source diff is unavailable, or if the question requires Cargo resolution/execution; route to the source owner and PROC-003. **required_for_acceptance:** `true` for feature/target-gate claims. Evidence: manifest output, exact source pin, affected REL IDs, and explicit UNKNOWNs.

[Procedure index](#m03)

<a id="proc-005"></a>

### PROC-005 — Validate package lint after a source change

**Trigger / predicate:** library, scenario, manifest, or lint-sensitive source changes and an authorized Rust-validation task exists. Pass when package clippy completes with `-D warnings`. **Mode:** `LOCAL_ISOLATED`; clean pinned checkout, offline cache only, no providers or live services. **review_status:** `BLOCKED` pending cold review; **execution_status:** `REVIEWED_NOT_EXECUTED`; **result:** `NOT_EXECUTED` in W015.

1. Run `cargo clippy --locked --offline --package e2e-resilience --all-targets -- -D warnings` from repository root.
2. Preserve the complete output, exact source/lock baseline, selected target/features, and exit result.

**Stop / recovery:** missing cache, source/lock drift, or requested network mutation is `BLOCKED_ENVIRONMENT`; do not fetch or change the lockfile. Route production/provider claims to M06. **required_for_acceptance:** `true` for lint-sensitive source changes; this procedure does not certify runtime behavior.

[Procedure index](#m03)


<a id="m04"></a>
## M04 — Change matrix

| Change type | Package / target / features | Command | Expected predicate | Environment / evidence |
|---|---|---|---|---|
| Manifest, direct dependency, or target declaration | `e2e-resilience`; library + `scenarios`; normal path deps `corelink-ratelimit`/`corelink-rate-headers`; workspace `uuid`/`thiserror` | **PROC-003**: `git show <source>:tests/e2e-resilience/Cargo.toml`; `rg ... --glob 'Cargo.toml'` | Name, manifest, dependency classes, target declarations and inverse census match reviewed source | Read-only; resolved selection remains UNKNOWN; not run in W015 |
| Lockfile or resolved dependency change | Same package; lockfile-selected versions/features | **PROC-003** static `git show <source>:Cargo.lock`; authorized Cargo resolution is a separate gate | Lock entries and affected RELs are recorded; no inference from `--target all` | Static procedure executed; Cargo resolution not run in W015 |
| Feature declaration/selection change | Package has no `[features]`; `scenarios` has no `required-features` | **PROC-004**: `git show <source>:tests/e2e-resilience/Cargo.toml`; `rg` feature/target selectors | Feature/required-feature facts match source; absent feature explicit | Read-only; no feature selection in W015; do not use `--all-features` as proof |
| Library or scenario logic | `e2e-resilience`; `--test scenarios`; no feature flags | **PROC-002**: `cargo test --locked --offline --package e2e-resilience --test scenarios` (authorized only) | Target exits successfully with declared assertions | Local offline cache; not run in W015 |
| Lint-sensitive source | `e2e-resilience`; all package targets; no feature flags | **PROC-005**: `cargo clippy --locked --offline --package e2e-resilience --all-targets -- -D warnings` | No warning/error under selected package targets | Not run in W015; no runtime claim |
| Ownership-document edit | Four owned files; profile S | PROC-001 | Four structural checks and scoped `git diff --check` pass | Lead-supplied checker path; outputs retained; run in W015 |
| Workspace workflow edit | Workspace targets, workflow-specific selection | Inspect exact workflow source and authorized run evidence | Target and cadence are identified; result is not inferred from YAML | CI access/approval required; not performed here |

**Negative cases:** `BackPressureQueue` rejects at capacity; armed audit failure returns a typed error before response; circuit audit failure leaves the asserted test snapshot Closed with no trip count (`tests/scenarios.rs:348-380,403-475`). These are source assertions, not results from this task. Do not use `--all-features`: no package feature set is declared.

<a id="m05"></a>
## M05 — Compatibility and recovery

| Surface | Reversible / state | Safe recovery | Proof and limit |
|---|---|---|---|
| Clock, queue, response helpers | Local object memory only; no package-owned persistence shown | Recreate fixture/process after a failed local scenario | No external state restore required by this source |
| Manifest, local API, scenario assertions | Source compatibility between the library and its target | Restore reviewed source through normal VCS review, then rerun the selected package checks | A source revert does not establish CI or runtime recovery |
| Limiter and circuit contract | Owned by upstream crates; their production persistence/wiring is outside this package | Route to `corelink-ratelimit` or `corelink-rate-headers`; identify actual composition root before operational action | Harness cannot roll back provider state or prove production recovery |

The inspected package has no database schema, migration, remote storage key, or wire protocol. Git rollback can restore source only; if a real system has already changed state, its composition-root and operator recovery procedure are UNKNOWN and must be located before action.

<a id="m06"></a>
## M06 — Escalation and maintenance record

| Condition | Verified route | Minimum evidence | Prohibited action |
|---|---|---|---|
| Local API or assertion mismatch | Package source and its `scenarios` target; repo review is requested by `.github/CODEOWNERS` default `@gmhelmold`, not enforced (`:18-27,45-50`) | Baseline, exact predicate, command and output | Claim independent approval or runtime evidence |
| Rate-limit contract mismatch | `corelink-ratelimit` implementation/contract owner | Affected REL, symbol, consumer, source predicate | Move implementation ownership to this harness |
| Circuit/header contract mismatch | `corelink-rate-headers` implementation/contract owner | Affected REL, symbol, consumer, source predicate | Treat the local queue as production middleware |
| Production, provider, audit delivery, or incident recovery | **BLOCKED/UNKNOWN:** this package names no composition root or on-call route. Send repository review/documentation questions to `@gmhelmold` through the normal PR/review route; send limiter/circuit contract questions to `corelink-ratelimit` / `corelink-rate-headers` owners; obtain the actual deployment/operator route before any operation | Actual composition root, environment, effect, authorized operator, and handoff acknowledgement | Guess an owner, treat CODEOWNERS as an incident route, or invoke an operational command |

**Escalation boundary:** `@gmhelmold` in `.github/CODEOWNERS` is a review-routing fact only, not production authority. Provider/API owner routes are package-level coordination routes, not proof of runtime reachability. Until a composition-root and on-call owner are identified, operational acceptance is `BLOCKED` and the required evidence is the missing owner, environment, requested authority, and source relation.

**W015 author record:** integration baseline `ab7137cd178f0e6cb282f3e944f9f7b58d0f5540`; source pin `1177dad2ca2a9f21c29b5a118aa7944b77147798`. In-scope paths are `.claude/skills/own-e2e-resilience/SKILL.md`, `REFERENCE.md`, `BLAST_RADIUS.md`, and `MAINTENANCE.md` under this package. All four S-profile structural checks and scoped `git diff --check` passed. `PROC-002` remains `NOT_EXECUTED`; no independent cold review was performed. These results are documentary only.

After maintenance, record source baseline, changed paths, result and unknowns; remove only task-created temporary files; reconcile affected ownership claims; send final hashes to a cold reviewer. Authors do not self-approve.

[Reference](REFERENCE.md#r01) · [Impact map](BLAST_RADIUS.md#b01) · [Start](#m01)
