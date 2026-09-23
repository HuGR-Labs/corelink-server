---
schema: corelink-ownership/1.1
document: blast_radius
package: sbom-publish
manifest: tools/sbom-publish/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: candidate
evidence_set: w013-sbom-publish-source-static-398e586c
---

# sbom-publish — blast radius

Static relation inventory for the manifest and package source at the pinned
commit. Manifest arrows record declarations; source arrows record visible
call/data-flow edges. Neither proves target resolution, execution, success,
remote effects, release impact, or deployment behavior.

[Scope](#b01) · [Method](#b02) · [Target relations](#b03) · [Dependency relations](#b04) · [Test/example relations](#b05) · [Coverage](#b06) · [Source flow](#b07)

<a id="b01"></a>
## B01 — Scope and failure boundary

The reviewed population is `Cargo.toml`, `src/{lib,main,publisher,ntia,purl,tsa,dt,metrics,error}.rs`, two integration-test files, and three example files. The manifest declares package/workspace metadata, lint block, two primary targets, 13 normal dependencies, four featured declarations, three dev dependencies, two test targets, and three examples. Source reading covered public surfaces, internal calls, documented local I/O and HTTP paths, metric updates, tests, and examples. No Cargo graph, reverse consumer census, command execution, external endpoint, or environment state was inspected.

<a id="b02"></a>
## B02 — Relation method

Each manifest relation is one directed declaration arrow; each source relation
is one direct code-level call or data-flow edge and names its source files.
Changing the named declaration/code falsifies that static relation. Activation,
compatibility, resolved dependency graph, runtime state, external effects,
failures, and reverse consumers remain UNKNOWN unless separately established.

<a id="b03"></a>
## B03 — Atomic target relations

Index: [REL-001](#rel-001) · [REL-002](#rel-002).

<a id="rel-001"></a>
### REL-001 — Package declaration → binary target

`package.name` → `[[bin]].name/path`: `sbom-publish` declares a binary of the
same name at `src/main.rs`. Changing either field at `Cargo.toml:2,13-15`
falsifies this arrow. It does not prove that path exists, is selected, builds,
links, executes, or performs an external action. [Relation index](#b03)


<a id="rel-002"></a>
### REL-002 — Package declaration → library target

`package.name` → `[lib].name/path`: `sbom-publish` declares library name
`sbom_publish` at `src/lib.rs`. Changing either field at `Cargo.toml:2,17-19`
falsifies this arrow. It does not prove that path exists, is selected, builds,
links, executes, or exposes a usable interface. [Relation index](#b03)


<a id="b04"></a>
## B04 — Atomic dependency relations

Index: [REL-003](#rel-003) · [REL-004](#rel-004) · [REL-005](#rel-005) · [REL-006](#rel-006) · [REL-007](#rel-007) · [REL-008](#rel-008) · [REL-009](#rel-009) · [REL-010](#rel-010) · [REL-011](#rel-011) · [REL-012](#rel-012) · [REL-013](#rel-013) · [REL-014](#rel-014) · [REL-015](#rel-015) · [REL-016](#rel-016) · [REL-017](#rel-017) · [REL-018](#rel-018) · [REL-019](#rel-019) · [REL-020](#rel-020) · [REL-021](#rel-021) · [REL-022](#rel-022).

<a id="rel-003"></a>
### REL-003 — Package → `async-trait` declaration

`sbom-publish` → `[dependencies].async-trait`: `Cargo.toml:21` declares this
normal dependency. Removing or renaming that declaration falsifies this arrow.
It does not prove resolution, download, inclusion, linkage, use, or interaction.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-004"></a>
### REL-004 — Package → `clap` declaration

`sbom-publish` → `[dependencies].clap`: `Cargo.toml:23` declares this normal
dependency. Removing or renaming that declaration falsifies this arrow. It does
not prove resolution, parsing, linkage, use, or runtime behavior.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-005"></a>
### REL-005 — Package → `hex` declaration

`sbom-publish` → `[dependencies].hex`: `Cargo.toml:24` declares this normal
dependency. Removing or renaming that declaration falsifies this arrow. It does
not prove resolution, decoding, linkage, use, or runtime behavior.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-006"></a>
### REL-006 — Package → `reqwest` declaration

`sbom-publish` → `[dependencies].reqwest`: `Cargo.toml:25` declares this
normal dependency. Removing or renaming that declaration falsifies this arrow.
It does not prove resolution, transport, linkage, use, or network interaction.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-007"></a>
### REL-007 — Package → `serde` declaration

`sbom-publish` → `[dependencies].serde`: `Cargo.toml:26` declares this normal
dependency. Removing or renaming that declaration falsifies this arrow. It does
not prove resolution, serialization, linkage, use, or runtime behavior.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-008"></a>
### REL-008 — Package → `serde_json` declaration

`sbom-publish` → `[dependencies].serde_json`: `Cargo.toml:27` declares this
normal dependency. Removing or renaming that declaration falsifies this arrow.
It does not prove resolution, JSON handling, linkage, use, or runtime behavior.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-009"></a>
### REL-009 — Package → `sha2` declaration

`sbom-publish` → `[dependencies].sha2`: `Cargo.toml:28` declares this normal
dependency. Removing or renaming that declaration falsifies this arrow. It does
not prove resolution, hashing, linkage, use, or runtime behavior.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-010"></a>
### REL-010 — Package → `thiserror` declaration

`sbom-publish` → `[dependencies].thiserror`: `Cargo.toml:29` declares this
normal dependency. Removing or renaming that declaration falsifies this arrow.
It does not prove resolution, error handling, linkage, use, or runtime behavior.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-011"></a>
### REL-011 — Package → `tokio` declaration

`sbom-publish` → `[dependencies].tokio`: `Cargo.toml:30` declares this normal
dependency. Removing or renaming that declaration falsifies this arrow. It does
not prove resolution, scheduling, linkage, use, or runtime behavior.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-012"></a>
### REL-012 — Package → `tracing` declaration

`sbom-publish` → `[dependencies].tracing`: `Cargo.toml:31` declares this
normal dependency. Removing or renaming that declaration falsifies this arrow.
It does not prove resolution, telemetry, linkage, use, or runtime behavior.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-013"></a>
### REL-013 — Package → `tracing-subscriber` declaration

`sbom-publish` → `[dependencies].tracing-subscriber`: `Cargo.toml:32` declares
this normal dependency. Removing or renaming that declaration falsifies this
arrow. It does not prove resolution, telemetry, linkage, use, or runtime behavior.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-014"></a>
### REL-014 — Package → `url` declaration

`sbom-publish` → `[dependencies].url`: `Cargo.toml:33` declares this normal
dependency. Removing or renaming that declaration falsifies this arrow. It does
not prove resolution, URL handling, linkage, use, or runtime behavior.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-015"></a>
### REL-015 — Package → `uuid` declaration

`sbom-publish` → `[dependencies].uuid`: `Cargo.toml:34` declares this normal
dependency. Removing or renaming that declaration falsifies this arrow. It does
not prove resolution, identifiers, linkage, use, or runtime behavior.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-016"></a>
### REL-016 — Package → `clap` feature declaration

`sbom-publish` → `[dependencies].clap.features`: `Cargo.toml:23` requests
`derive`. Changing that feature declaration falsifies this arrow. It does not
prove feature resolution, parsing, linkage, use, or runtime behavior.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-017"></a>
### REL-017 — Package → `reqwest` feature declaration

`sbom-publish` → `[dependencies].reqwest.features/default-features`:
`Cargo.toml:25` requests `json`, `multipart`, and `rustls-tls` and disables
default features. Changing those values falsifies this arrow. It does not prove
feature resolution, transport, linkage, use, or network interaction.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-018"></a>
### REL-018 — Package → `tokio` feature declaration

`sbom-publish` → `[dependencies].tokio.features`: `Cargo.toml:30` requests
`rt-multi-thread`, `macros`, and `time`. Changing that feature declaration
falsifies this arrow. It does not prove resolution, scheduling, use, or runtime.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-019"></a>
### REL-019 — Package → `uuid` feature declaration

`sbom-publish` → `[dependencies].uuid.features`: `Cargo.toml:34` requests `v4`
and `serde`. Changing that feature declaration falsifies this arrow. It does not
prove resolution, identifier generation, linkage, use, or runtime behavior.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-020"></a>
### REL-020 — Package → `proptest` development declaration

`sbom-publish` → `[dev-dependencies].proptest`: `Cargo.toml:36` declares this
development dependency. Removing or renaming it falsifies this arrow. It does
not prove a test build, input generation, linkage, use, or an observed result.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-021"></a>
### REL-021 — Package → `sha2` development declaration

`sbom-publish` → `[dev-dependencies].sha2`: `Cargo.toml:37` declares this
development dependency. Removing or renaming it falsifies this arrow. It does
not prove a test build, hashing, linkage, use, or an observed result.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="rel-022"></a>
### REL-022 — Package → `tokio` development declaration

`sbom-publish` → `[dev-dependencies].tokio`: `Cargo.toml:39` requests `rt` and
`macros`. Removing, renaming, or changing it falsifies this arrow. It does not
prove a test build, scheduling, linkage, use, or an observed result.

[Dependency relation index](#b04) [Relation index](#b03)

<a id="b05"></a>
## B05 — Atomic test and example relations

Index: [REL-023](#rel-023) · [REL-024](#rel-024) · [REL-025](#rel-025) · [REL-026](#rel-026) · [REL-027](#rel-027). [Relation index](#b03)

<a id="rel-023"></a>
### REL-023 — Package → `prop_sbom` test declaration

`sbom-publish` → `[[test]] prop_sbom`: `Cargo.toml:41-43` maps that name to
`tests/prop_sbom.rs`. Changing its name or path falsifies this arrow. It does
not prove the file exists, is selected, compiles, runs, or validates behavior.

[Test/example relation index](#b05) [Relation index](#b03)

<a id="rel-024"></a>
### REL-024 — Package → `adversarial` test declaration

`sbom-publish` → `[[test]] adversarial`: `Cargo.toml:45-47` maps that name to
`tests/adversarial.rs`. Changing its name or path falsifies this arrow. It does
not prove the file exists, is selected, compiles, runs, or validates behavior.

[Test/example relation index](#b05) [Relation index](#b03)

<a id="rel-025"></a>
### REL-025 — Package → `generate` example declaration

`sbom-publish` → `[[example]] generate`: `Cargo.toml:49-51` maps that name to
`examples/generate.rs`. Changing its name or path falsifies this arrow. It does not
prove file existence, execution, generated output, or external interaction.

[Test/example relation index](#b05) [Relation index](#b03)

<a id="rel-026"></a>
### REL-026 — Package → `validate_ntia` example declaration

`sbom-publish` → `[[example]] validate_ntia`: `Cargo.toml:53-55` maps that name
to `examples/validate_ntia.rs`. Changing its name or path falsifies this arrow. It does not
prove file existence, execution, output handling, or external interaction.

[Test/example relation index](#b05) [Relation index](#b03)

<a id="rel-027"></a>
### REL-027 — Package → `ingest_dt_retry` example declaration

`sbom-publish` → `[[example]] ingest_dt_retry`: `Cargo.toml:57-59` maps that
name to `examples/ingest_dt_retry.rs`. Changing its name or path falsifies this arrow. It does
not prove file existence, execution, output handling, or external interaction.

[Test/example relation index](#b05) [Relation index](#b03)

<a id="b06"></a>
## B06 — Coverage and unknowns

The 27 manifest arrows cover two targets, 13 normal declarations, four featured
declarations, three development declarations, two tests, and three examples.
Workspace metadata/lints are predicates in R02 rather than cross-surface arrows.
The verified [ADR-S12-001](../../../knowledge/adr/adr-s12-001-sbom-cyclonedx-ntia-tsa-dt.md) is an OKF route only, not a relation. Unknown: all resolution, reverse consumers, source behavior, test execution, outputs, external interaction, release, runtime, deployment, and review.

This count describes only REL-001–REL-027; source relations are separately
indexed in B07 and do not change the manifest relation count.

<a id="b07"></a>
## B07 — Source flow relations

These arrows reflect direct static call/data-flow paths in the reviewed Rust
files. They are not evidence that calls execute or that their stated effects
succeed.

| ID | Static source arrow | Bounded impact surface | Evidence boundary |
|---|---|---|---|
| SRC-001 | CLI `run(Generate)` → `DefaultSbomPublisher::generate` → `cargo cyclonedx` subprocess → reads `sbom.cdx.json` under the manifest directory. | Local subprocess and filesystem path; error on spawn, nonzero status, or read/JSON failure. | `src/main.rs`, `src/publisher.rs`, `SbomError`; subprocess was not run. |
| SRC-002 | `generate` → `normalise_sbom_purls` → component PURL/property mutation. | Mutates the in-memory SBOM value according to configured workspace/patched crate lists. | `src/publisher.rs`, `src/purl.rs`; no fixture execution here. |
| SRC-003 | `generate` and trait `validate_ntia` → `validate_ntia_json(Strict)` → `NtiaOutcome` metric update or validation error. | Strict predicate gates the generate return; validator method returns an error on noncompliance. | `src/publisher.rs`, `src/ntia.rs`, `src/metrics.rs`, `src/error.rs`; no test run here. |
| SRC-004 | `generate` → serializes normalized JSON → `request_tsa_timestamp`; TSA error is converted into `None`, not returned from `generate`. | TSA request uses the configured URL/client and yields an optional token in `SignedSbom`. | `src/publisher.rs`, `src/tsa.rs`; remote request and policy outcome UNKNOWN. |
| SRC-005 | CLI `AttestTsa` → reads input bytes → requests TSA → verifies hash binding → writes `.tsr` bytes. | Local input/output plus remote TSA request; source maps TSA error to exit code 3. | `src/main.rs`, `src/tsa.rs`, `src/error.rs`; command was not run. |
| SRC-006 | `ingest_dt` and CLI `IngestDt` → `ingest_into_dt` → multipart POST `/api/v1/bom` with up to four attempts; 400/422 return early; exhaustion writes `.sbom_fallback_queue`. | Remote request, key header, local queue path, and outcome metrics. | `src/publisher.rs`, `src/main.rs`, `src/dt.rs`, `src/metrics.rs`; no request or write was performed. |
| SRC-007 | CLI `ValidateNtia` and validator example → `validate_ntia_json` → JSON report; strict CLI mode returns validation error while auditor mode does not. | Local SBOM read and stdout; exit mapping is code-defined. | `src/main.rs`, `src/ntia.rs`, `examples/validate_ntia.rs`; no command was run. |
| SRC-008 | CLI entry point → initializes tracing subscriber; exit path → `METRICS.emit_summary`. | Process-local metric atomics are logged; no exporter or scrape endpoint is defined in these files. | `src/main.rs`, `src/metrics.rs`; log output and external metrics delivery UNKNOWN. |
| SRC-009 | Property/adversarial test source → public validators, PURL helpers, TSR binding, and DT ingestion. | Declared cases cover NTIA omissions/placeholders, malformed PURLs, component counts, hash binding/replay, unavailable DT endpoint, and workspace PURL discrimination. | `tests/prop_sbom.rs`, `tests/adversarial.rs`; assertions are source text, no test result is claimed. |
| SRC-010 | Examples → publisher generation, NTIA validation, or DT ingestion. | Example source shows separate entry points and some local file / environment handling. | `examples/*.rs`; examples are not verified to compile or run. |

Source limitations and conflicting statements are recorded in [R08](REFERENCE.md#r08). The known source path is not a verified external dependency or runtime graph. [Reference](REFERENCE.md#r07) · [Maintenance](MAINTENANCE.md#m01) · [Back to scope](#b01)

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Back to scope](#b01)
