---
schema: corelink-ownership/1.1
document: reference
package: corelink-tenant-path-fuzz
manifest: crates/tenant-path/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w016-tenant-path-fuzz-source-20260921
---

# corelink-tenant-path-fuzz — reference

SOURCE-static account of an independent fuzz package. It describes target intent and assertions, not fuzz execution, cryptographic certification, tenant runtime, or production behavior.

[Identity](#r01) · [Boundaries](#r02) · [Implementation](#r03) · [Contracts](#r04) · [Inputs and invariants](#r05) · [Targets](#r06) · [Failures](#r07) · [Evidence](#r08)

<a id="r01"></a>
## R01 — Package identity

| Field | Source-visible value |
|---|---|
| Cargo package / manifest | `corelink-tenant-path-fuzz` / `crates/tenant-path/fuzz/Cargo.toml` |
| Workspace | Independent `[workspace]`; the parent directory is not package identity |
| Package metadata | `cargo-fuzz=true`; version `0.0.0`; `publish=false`; edition 2021; `UNLICENSED` |
| Targets | Binaries `derive_prefix` and `derive_prefix_extended`; each sets `test`, `doc`, and `bench` false |
| Profile | S; two binaries do not establish a need for H |
| Current evidence | Manifest and target source inspected at source pin; fuzz execution and runtime are unknown |

The description mentions arbitrary paths, but the inspected harnesses consume byte slices as TDK and UUID material; neither parses path strings. [Manifest](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/Cargo.toml) · [Targets](https://github.com/HuGR-Labs/corelink-server/tree/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets)

<a id="r02"></a>
## R02 — Ownership boundaries

The harness package owns the two target entrypoints and their assertions. `corelink-tenant-path` owns `TenantDerivationKey`, `TenantPrefix`, and `derive_prefix`; the fuzzer consumes that API. `libfuzzer-sys`, `uuid`, and `zeroize` are external direct dependencies of the harness.

The targets do not implement a path parser, authorization, tenant lookup, storage adapter, key source, or service entrypoint. There is no source evidence here for a live key, tenant, provider, database, bucket, deployment, or successful fuzz run. The code comment's security language is intent, not proof of the corresponding property.

<a id="r03"></a>
## R03 — Implementation map

| File | Source-visible role | Evidence |
|---|---|---|
| `fuzz_targets/derive_prefix.rs` | Accepts at least 48 bytes; uses bytes 0–31 as TDK and 32–47 as UUID; calls the parent API; checks 16-byte URL-safe output and formats | [Source](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs) |
| `fuzz_targets/derive_prefix_extended.rs` | Accepts at least 80 bytes; takes TDK A, UUID, and TDK B from fixed slices; asserts same-input determinism; checks output shape after one UUID bit flip and a second key | [Source](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs) |
| `Cargo.toml` | Defines two fuzz binary targets, direct dependencies, own workspace, and lint rules | [Manifest](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/Cargo.toml) |

No third target, library module, path parser, corpus file, or generated source is established by this inventory. `fuzz/.gitignore` ignores `target`, `corpus`, `artifacts`, and `coverage`; this checkout's tracked inventory is not evidence about another machine's ignored files.

<a id="r04"></a>
## R04 — Consumed public contracts

The API implementations belong to `corelink-tenant-path`; the fuzzer owns no public library API.

| Record | Symbol and consumed contract |
|---|---|
| [API-001](#api-001) | `TenantDerivationKey::from_bytes(Zeroizing<[u8; 32]>) -> TenantDerivationKey` |
| [API-002](#api-002) | `derive_prefix(&TenantDerivationKey, Uuid) -> TenantPrefix` |
| [API-003](#api-003) | `TenantPrefix::as_str() -> &str`; both targets inspect this output |
| [API-004](#api-004) | `Display` and `Debug` for `TenantPrefix`; the base target formats both |

<a id="api-001"></a>
### API-001 — Construct the derivation key
**Contract:** `from_bytes(Zeroizing<[u8; 32]>) -> TenantDerivationKey`; the harness supplies synthetic bytes, never a real key.

**Errors:** No `Result`; the `[u8; 32]` type is the only input guard.

**Effects:** Copies synthetic bytes into a zeroize-aware key; erasure of every stack copy is unproven.

**Compatibility:** Changing the wrapper, constructor, or key length affects the target boundary.

**INV / REL:** [INV-001](#inv-001) · [REL-004](BLAST_RADIUS.md#rel-004).

**Tests:** [base target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs) · [extended target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs) · [parent property tests](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/tests/prop_tenant_path.rs). No test ran here.
[R04](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Derive the typed prefix
**Contract:** `derive_prefix(&TenantDerivationKey, Uuid) -> TenantPrefix` is infallible and returns exactly `TENANT_PREFIX_LEN = 16` ASCII bytes.

**Errors:** No public error path; typed inputs make the source operation total.

**Effects:** HMAC-SHA256 over UUID bytes produces a new in-memory `TenantPrefix`; the target checks shape only.

**Compatibility:** HMAC-SHA256, UUID bytes, URL-safe base64 without padding, and the first 16 characters align with [OKF/ADR-0043](../../../knowledge/adr/adr-0043-hmac-tenant-prefix-algorithm.md); changes affect [INV-003](#inv-003), [INV-004](#inv-004), and [REL-001](BLAST_RADIUS.md#rel-001).

**INV / REL:** [INV-003](#inv-003) · [INV-004](#inv-004) · [REL-001](BLAST_RADIUS.md#rel-001).

**Tests:** [base target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs) · [extended target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs) · [parent property tests](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/tests/prop_tenant_path.rs). No test ran here.
[R04](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Read the prefix string
**Contract:** `TenantPrefix::as_str() -> &str` exposes the valid `TENANT_PREFIX_LEN = 16` ASCII representation; raw bytes remain private.

**Errors:** UTF-8 conversion is total for the internally constrained alphabet; no `Result` is exposed.

**Effects:** Returns a string view without mutation or raw-byte exposure; both targets inspect length and URL-safe bytes.

**Compatibility:** Changing representation or length affects worker parity and target consumers.

**INV / REL:** [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004) · [REL-001](BLAST_RADIUS.md#rel-001).

**Tests:** [base target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs) · [extended target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs) · [parent property tests](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/tests/prop_tenant_path.rs). No test ran here.
[R04](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Format the prefix
**Contract:** `Display` writes the `as_str()` value; `Debug` writes `TenantPrefix(<prefix>)`. The base target invokes both.

**Errors:** Formatting reports the formatter's `fmt::Result`; no derivation error is introduced.

**Effects:** Formatting does not mutate the prefix or expose the TDK. A `DefaultHasher` construction is not a hash assertion.

**Compatibility:** Changing either textual form affects harness observability and parsing consumers.

**INV / REL:** [INV-002](#inv-002) · [REL-001](BLAST_RADIUS.md#rel-001).

**Tests:** [base target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs) · [parent property tests](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/tests/prop_tenant_path.rs). No test ran here.
[R04](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Input flow and falsifiable invariants

The fuzzer supplies a byte slice. The base target returns without calling the library below 48 bytes; otherwise it constructs a 32-byte TDK and 16-byte `Uuid`. The extended target returns below 80 bytes; otherwise it constructs TDK A, a UUID, and TDK B. `Uuid::from_bytes` accepts those raw bytes without a v7-version check. The manifest's `uuid` v7 feature enables a dependency capability; it does not constrain the target inputs to v7 UUIDs.

| Record | Predicate and source-visible failure |
|---|---|
| [INV-001](#inv-001) | Base target calls derivation only when `data.len() >= 48`; changing its threshold or slices falsifies this record. |
| [INV-002](#inv-002) | Base target requires a 16-byte result whose bytes are ASCII alphanumeric, `-`, or `_`; failure is an assertion. |
| [INV-003](#inv-003) | Extended target requires repeated derivation of the same key/UUID to return the same string and requires the first result's 16-byte shape. |
| [INV-004](#inv-004) | After one data-selected UUID bit flip and with a second TDK, extended target checks output length only; it does not assert injectivity or distinct-key separation. |
| [INV-005](#inv-005) | Both targets derive inputs from raw bytes; no path string or canonical tenant identifier parser is called. |

<a id="inv-001"></a>
### INV-001 — Base input threshold
For inputs shorter than 48 bytes, the base target returns before `derive_prefix`; at 48 bytes or more it copies the first 32 and next 16 bytes. A changed threshold or slice falsifies the predicate. No execution was performed. [Target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs)
[R05](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Base output shape
Each accepted base input must yield a string of `TENANT_PREFIX_LEN` bytes, and every byte must be ASCII alphanumeric, `-`, or `_`, or the harness assertion fails. The source assertion is present; no fuzz run was observed. [Target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets/derive_prefix.rs)
[R05](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Repeated-call determinism
For each accepted extended input, two calls with TDK A and the same UUID must return equal strings. The target asserts equality and length; it does not explore all possible TDK/UUID pairs or certify a PRF. [Target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs)
[R05](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Changed-input limits
The extended target selects one UUID bit using `data[0] & 0x7f` and derives once with TDK B. These paths assert length, not that changed UUIDs or keys yield distinct prefixes. The source comments' injectivity/separation wording exceeds the executable predicate. [Target](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets/derive_prefix_extended.rs)
[R05](#r05)

<a id="inv-005"></a>
[↩](#r01)
### INV-005 — No path parser in the harness
Every consumed byte belongs to TDK or UUID construction; neither target passes a path string to the parent crate. A path-fuzzing claim is falsified by the current target source. [Targets](https://github.com/HuGR-Labs/corelink-server/tree/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/fuzz_targets)
[R05](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Dependencies, targets, and configuration

| Declaration | Role and limit |
|---|---|
| `corelink-tenant-path = { path = ".." }` | Direct local implementation dependency; source reachability is established, runtime is not |
| `libfuzzer-sys = "0.4"` | Fuzz target macro/runtime dependency; resolved version is not claimed here |
| `uuid = { version = "1", features = ["v7"] }` | Direct UUID dependency with declared v7 feature; input uses `Uuid::from_bytes` |
| `zeroize = "1.8"` | Direct wrapper used for TDK construction; does not prove all local copies are scrubbed |
| `unsafe_code = "forbid"` | Manifest lint declaration; no harness code change is made by this document |
| Clippy denies | `unwrap_used`, `panic`, `indexing_slicing`, `todo`, `unimplemented` are declared denied |

The root workspace separately excludes `crates/tenant-path/fuzz` from the outer resolver; the standalone manifest's `[workspace]` makes that package independent. This is a source declaration, not proof that Cargo selected or built it. [Root manifest](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/Cargo.toml)

The standalone lockfile separately pins the inspected package graph: `corelink-tenant-path-fuzz 0.0.0` depends on `corelink-tenant-path`, `libfuzzer-sys 0.4.12`, `uuid 1.23.1`, and `zeroize 1.8.2`; registry checksums are recorded for registry packages. This is lockfile source evidence, not a claim that resolution or execution occurred. [Fuzz lockfile](https://github.com/HuGR-Labs/corelink-server/blob/1177dad2ca2a9f21c29b5a118aa7944b77147798/crates/tenant-path/fuzz/Cargo.lock)

No package feature matrix is declared in the inspected manifest. Dependency resolution, target selection by Cargo, and the exact nightly/cargo-fuzz toolchain in any past run remain unknown.

<a id="r07"></a>
## R07 — Failures and observability

A failed assertion or sanitizer finding is a fuzz-process failure whose useful evidence is the exact target, minimized input, artifact, toolchain, and command/CI run record. The repository source defines artifact/corpus ignore paths; it does not show the state of a runner's local corpus. A source assertion is not an observed failure or pass.

The base target formats `TenantPrefix` using `Display` and `Debug`. It does not print the TDK. Its input bytes are fuzzer-controlled synthetic data; do not replace them with real secrets. A fuzzer crash in parent library code must be reported to the `corelink-tenant-path` implementation boundary with a reproducer; harness ownership does not establish the bug's owner or severity.

<a id="r08"></a>
## R08 — Evidence and unknowns

Source pin: [`1177dad2`](https://github.com/HuGR-Labs/corelink-server/tree/1177dad2ca2a9f21c29b5a118aa7944b77147798). Integration baseline: `8cdc02828132b9b6f03a3b57117b8140325f6762`. Static comparison found no difference in root `Cargo.toml` or the `crates/tenant-path/fuzz/` package tree. Outside the owned package, `crates/tenant-path/Cargo.toml` differs only in its repository URL.

The complete Cargo resolver graph, all target/feature selections, actual corpus contents, fuzzer results, crash history, CI run outcomes, dependency versions resolved in a run, cross-language parity under execution, key inputs, storage compatibility, and runtime wiring are UNKNOWN. Manifest declarations, imports, and workflow YAML are source evidence only. No named escalation owner or independent reviewer was verified.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership skill](../../../../.claude/skills/own-corelink-tenant-path-fuzz/SKILL.md#s01)
