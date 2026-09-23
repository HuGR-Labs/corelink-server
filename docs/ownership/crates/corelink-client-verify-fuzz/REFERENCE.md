---
schema: corelink-ownership/1.1
document: reference
package: corelink-client-verify-fuzz
manifest: crates/corelink-client-verify/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: client-verify-fuzz-source-static-20260921
---

# corelink-client-verify-fuzz — ownership reference

This SOURCE-only reference separates harness code from its parent library. A declared binary, feature, workflow command, or static call is not evidence of a build, execution, reachability, or coverage.

[Identity](#r01) · [Ownership](#r02) · [Harnesses](#r03) · [Contracts](#r04) ·
[Axioms](#r05) · [Features](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity and function

`corelink-client-verify-fuzz` is a non-publishable, `UNLICENSED` Cargo package with its own `[workspace]`. It declares two binary harnesses for the client verifier API. The tracked package tree at the source pin contains only its manifest, lockfile, and those two target files; no corpus is tracked there. The package invokes library code by a local path dependency; it owns neither that library implementation nor the workflow that may invoke it.

| Field | Verified value |
|---|---|
| Package / manifest | `corelink-client-verify-fuzz` / `crates/corelink-client-verify/fuzz/Cargo.toml` |
| Targets | Bins `verify_sync`, `verify_ffi`; no library target declared |
| Role | Independent fuzz harness package; not a service or SDK wrapper |
| License / publish | `UNLICENSED` / `false` |
| implemented / wired / runtime_verified | `yes` / `partial — PR-only: `fuzz-smoke` is declared under `pull_request`; `fuzz-nightly` is dormant because `on.schedule` is commented / `unknown`; build and execution not established |

**Evidence:** [S01 manifest](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-client-verify/fuzz/Cargo.toml#L1-L49), [S09 tracked tree and lock](https://github.com/HuGR-Labs/corelink-server/tree/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-client-verify/fuzz).

<a id="r02"></a>
## R02 — Boundaries and owner roles

| Surface | Implementation owner | Contract owner | Composition root | Runtime operator | Review authority |
|---|---|---|---|---|---|
| Fuzz package and callbacks | Package source owns two harness files; named person/team UNKNOWN | Consumed Rust and C-ABI contracts are in `corelink-client-verify` | Workflow declares invocations | UNKNOWN; `corelink-builder` is a runner label, not a person | UNKNOWN; fresh independent reviewer not assigned |
| `ClientVerifier`, `Digest`, and `ffi` | `crates/corelink-client-verify` | Same library source for these symbols | Separate consumers; not this package | UNKNOWN | Parent contract authority; named reviewer UNKNOWN |

**Does not own:** verifier/hash implementation, SDK bindings, workflow configuration, fuzzer runtime, or production data. No verified individual/team escalation route was found in the inspected package sources; do not invent one. Static consumer references to the parent library do not make the fuzz package their composition root. The relevant broader SDK contract is routed through the repository’s existing [SDK reference](../../../knowledge/ops/sdk-reference.md), not copied here.

<a id="r03"></a>
## R03 — Implementation map

| Module / entry | Behavior visible in source | Nature | Evidence |
|---|---|---|---|
| `fuzz_targets/verify_sync.rs`, `fuzz_target!` | Splits a 32-byte claim from body bytes; constructs hex, calls `Digest` and `ClientVerifier::verify`, then asserts match/mismatch outcomes. | Owned binary harness | [S03](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-client-verify/fuzz/fuzz_targets/verify_sync.rs#L1-L44) |
| `fuzz_targets/verify_ffi.rs`, `Claim`, `fuzz_target!` | Maps control bits to constructors, null/length variants, digest codecs and FFI calls; compares return tags and frees the handle. | Owned binary harness; intentional unsafe calls | [S04](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-client-verify/fuzz/fuzz_targets/verify_ffi.rs#L1-L225) |

The sync target imports `ClientVerifier`, `Digest`, `VerifyError`, and `DIGEST_LEN` from the library root; `src/lib.rs` reexports those symbols. The FFI target imports the `ffi` module gated by feature and `Digest` from that root. This fuzz package defines no reexports of its own.

The sync oracle calls `Digest::compute` and `Digest::verify_constant_time`, the same library surface whose verifier result it checks; it is not an independent hash implementation. The FFI target documents a precedence oracle in source; its cases are intended inputs, not observed reachability. Neither file calls the `stream` API although the manifest enables that feature.

<a id="r04"></a>
## R04 — Public contracts

This binary-only package declares no public Rust library API or C ABI of its own. Its `fuzz_target!` callbacks are harness entrypoints, not a reusable package contract. The dependency contracts it consumes are recorded as separate source relations in [B03](BLAST_RADIUS.md#b03); their owners remain the parent library.

<a id="r05"></a>
## R05 — Five falsifiable source axioms

| ID | Predicate at the pin | Static falsifier | Evidence and limit |
|---|---|---|---|
| [INV-001](#inv-001) | Manifest name is `corelink-client-verify-fuzz`; the path is excluded from the root workspace and has its own `[workspace]`. | Change the name, local workspace marker, or root exclusion. | S01, S02; source identity only. |
| [INV-002](#inv-002) | Exactly two bins are declared, each with `test`, `doc`, and `bench` false. | Add/remove/rename a bin or alter one flag. | S01; declaration does not show that either bin ran. |
| [INV-003](#inv-003) | Verifier implementation is external to this package: the manifest path-depends on `corelink-client-verify`, and harnesses call its symbols. | Remove the edge/calls or move implementation into this package. | S01, S03, S04; no resolution or reachability proof. |
| [INV-004](#inv-004) | The dependency enables `stream` and `ffi`; the two checked target files call sync and FFI symbols, not stream symbols. | Alter features or introduce/remove a stream call. | S01, S03, S04, S05; feature selection is not stream coverage. |
| [INV-005](#inv-005) | Harness lint allows unsafe; parent manifest denies unsafe and the parent `ffi` module contains the scoped exception. | Change either lint policy or exception boundary. | S01, S05–S07; lint/build enforcement was not executed. |

<a id="inv-001"></a>
### INV-001 — Independent package identity

**Rule:** package name and local workspace structure, not directory resemblance, define the Cargo package boundary. **Falsifier:** alter the manifest name or workspace/exclusion declarations. **SOURCE:** S01 and [S02](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/Cargo.toml#L384-L394). **Limit:** this does not verify a Cargo metadata result.

[Axiom index](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Targets are declarations

**Rule:** two binary declarations specify `verify_sync` and `verify_ffi`; `test=false`, `doc=false`, and `bench=false` are manifest flags. **Falsifier:** change a target declaration. **SOURCE:** S01. **Limit:** no execution record or coverage follows from these flags.

[Axiom index](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Harness placement does not own verifier code

**Rule:** the package calls symbols supplied by a path dependency; implementation and public contract stay with `corelink-client-verify`. **Falsifier:** remove the dependency/calls or relocate an implementation into this package. **SOURCE:** S01, S03, S04, S06. **Limit:** path declaration and source calls do not establish resolution or runtime reachability.

[Axiom index](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Enabled stream is not exercised here

**Rule:** `stream` and `ffi` are enabled on the path dependency, but only sync and FFI surfaces appear in the two target imports/calls. **Falsifier:** change the feature list or introduce a stream call. **SOURCE:** S01, S03, S04, S05. **Limit:** source search does not establish a selected build or runtime path.

[Axiom index](#r05)

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — Unsafe policy is package-local

**Rule:** this fuzz manifest allows unsafe to drive raw-pointer FFI cases; the library manifest denies unsafe and `src/ffi.rs` scopes its exception locally. **Falsifier:** change any of the three declarations. **SOURCE:** S01, S05, S06, S07. **Limit:** attributes are not lint execution and do not cover other crates.

[Axiom index](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Targets, features, and dependencies

| Declaration | SOURCE state | Limit |
|---|---|---|
| Independent workspace | `[workspace]`; root `Cargo.toml` excludes `crates/corelink-client-verify/fuzz`. | No metadata or resolver result collected. |
| Features | Path dep `corelink-client-verify` enables `stream` and `ffi`; parent default feature set is empty. | Stream feature is enabled but not called by these harness sources. |
| Direct dependencies | `libfuzzer-sys = 0.4`; path `corelink-client-verify`; `hex = 0.4`. | `hex` is declared but is not imported by either inspected target; no graph resolution. |
| Binary flags | Both targets have `test=false`, `doc=false`, `bench=false`. | No inference about cargo-fuzz execution. |
| Locks | Package-local `fuzz/Cargo.lock` is tracked. | Locked versions are not a build or runtime result. |

The harness manifest allows `unsafe_code`; its Clippy declarations deny unwrap, panic, array indexing, and unfinished implementations. The parent library separately declares `unsafe_code = "deny"`, crate-root `#![deny(unsafe_code)]`, and a module-local FFI allowance. Do not copy the harness exception into the library or call it a library-wide allowance.

**Evidence:** [S01](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-client-verify/fuzz/Cargo.toml#L14-L49), [S05](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-client-verify/Cargo.toml#L1-L49), [S06](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-client-verify/src/lib.rs#L46-L80), [S07](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/corelink-client-verify/src/ffi.rs#L27-L46).

<a id="r07"></a>
## R07 — Failure and observability

| Harness outcome | Source condition | Local effect | Limit |
|---|---|---|---|
| Sync callback returns | Input shorter than `DIGEST_LEN`, or digest conversion returns `Err`. | No verifier assertion on that input. | Does not prove how often either branch is reached. |
| Sync assertion failure | Match/mismatch result differs from the oracle or error fields. | Assertion panic/crash candidate for the fuzzer process. | No crash was observed in this review. |
| FFI result tag | Null/oversized pointers, digest encoding, disabled state, or digest comparison select a declared tag. | Assertions compare `rc`, optional out tag, and expected code; a mismatch panics. | Source oracle mirrors the parent precedence tree; no call was executed. |
| Disabled warning path | A control bit selects `corelink_verifier_new_disabled(1)`. | Parent source declares a tracing warning on this constructor path. | No log or metric was observed. |

The callback scratch data and handle are local to one invocation in source; corpus, artifact, timeout, memory, and runner persistence behavior are not established from the package tree. No production signal or service state is owned here.

<a id="r08"></a>
## R08 — Verification and evidence

| ID | Pinned path / lines | Method and observed result | Limit |
|---|---|---|---|
| S01 | `crates/corelink-client-verify/fuzz/Cargo.toml:1-49` | Manifest facts transcribed; SOURCE | No Cargo metadata or lint run |
| S02 | `Cargo.toml:384-394,555-559` | Exclusion and parent workspace dependency read; SOURCE | No resolved graph |
| S03 | `fuzz_targets/verify_sync.rs:1-44` | Sync oracle calls read; SOURCE | No build, reachability, or execution |
| S04 | `fuzz_targets/verify_ffi.rs:1-225` | FFI inputs and assertions read; SOURCE | No unsafe invocation |
| S05 | Parent `Cargo.toml:1-49` | Features/lints read; SOURCE | Only repository URL differs from integration baseline |
| S06 | Parent `src/lib.rs:46-80` | Unsafe lint, gates, and exports read; SOURCE | No selected feature build |
| S07 | Parent `src/ffi.rs:27-46,75-95,119-175,225-318` | Exception, tags, and verifier validation source read; SOURCE | No ABI loading or call |
| S08 | `.github/workflows/corelink-client-verify.yml:1-23,294-398` | PR filter and `fuzz-smoke` declarations read; `fuzz-nightly` is declared but its `on.schedule` trigger is commented at lines 23-33; SOURCE | No GitHub/run history |
| S09 | `fuzz/Cargo.lock:105-111`; tracked package subtree | Lock package and four tracked paths checked; SOURCE | Lock is not build evidence |

**Pin and drift:** all records above use `1177dad2ca2a9f21c29b5a118aa7944b77147798`. Scoped comparison with integration base `8cdc02828132b9b6f03a3b57117b8140325f6762` found only a parent manifest repository URL change (`HuGR-Labs` → `HuGR-dev`); fuzz files and root manifest match. Workflow source contains an active PR smoke lane and a dormant nightly job declaration; the schedule trigger is commented at lines 23-33, and no run result is verified.

**Contradiction:** pinned target code is sync digest/FFI verification, while older pentest and STRIDE documents call this path Merkle/RFC-6962 proof fuzzing. Reconcile that claim before reuse; those reports are not current target evidence.

The work item records a historical fuzz-success claim and a performance report lists workflow job names. Neither was independently validated here. **Unknown:** resolved target/features and reverse graph; actual input corpus, execution, reachability, crashes or coverage; CI event history; SDK behavior; ABI loading; owner names/escalation; runtime, deployment, provider, and production state. A workflow command proves intent only. Continue to [impact](BLAST_RADIUS.md#b01) and [maintenance](MAINTENANCE.md#m01).

[Back to identity](#r01)
