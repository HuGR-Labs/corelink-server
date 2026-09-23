---
schema: corelink-ownership/1.1
document: reference
package: sbom-publish
manifest: tools/sbom-publish/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: candidate
evidence_set: w013-sbom-publish-source-static-398e586c
---

# sbom-publish — ownership reference

SOURCE-only reference scoped to the package manifest, library, binary, examples,
and tests at the pinned commit. A source statement records code structure or an
implementation path; it is not evidence that a target builds, a command runs,
an SBOM is handled, or an external system is contacted.

[Identity](#r01) · [Inheritance](#r02) · [Targets](#r03) · [Dependencies](#r04) · [Axioms](#r05) · [Tests/examples](#r06) · [Source map](#r07) · [Evidence limits](#r08)

<a id="r01"></a>
## R01 — Package identity invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| `package.name` is `sbom-publish`. | Change or remove `Cargo.toml:1-3`'s name field. |
| `package.description` is present as manifest text. | Remove or change the description field at `Cargo.toml:3`. |

Neither predicate proves the meaning or truth of the description text.

<a id="r02"></a>
## R02 — Workspace inheritance invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| `package.version` is declared as `workspace = true`. | Change/remove `Cargo.toml:4`; the resolved version is UNKNOWN. |
| `package.edition` is declared as `workspace = true`. | Change/remove `Cargo.toml:5`; the resolved edition is UNKNOWN. |
| `package.rust-version` is declared as `workspace = true`. | Change/remove `Cargo.toml:6`; the resolved Rust version is UNKNOWN. |
| `package.license` is declared as `workspace = true`. | Change/remove `Cargo.toml:7`; the resolved license is UNKNOWN. |
| `package.publish` is declared as `workspace = true`. | Change/remove `Cargo.toml:8`; the resolved publish value is UNKNOWN. |
| The package requests workspace lints. | Change/remove `Cargo.toml:10-11`; lint application is UNKNOWN. |

<a id="r03"></a>
## R03 — Declared target invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| A `[[bin]]` declaration names `sbom-publish` and its path `src/main.rs`. | Change/remove either field at `Cargo.toml:13-15`. |
| A `[lib]` declaration names `sbom_publish` and its path `src/lib.rs`. | Change/remove either field at `Cargo.toml:17-19`. |

The declarations do not prove files exist, compile, link, or execute.

<a id="r04"></a>
## R04 — Dependency declaration invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| `[dependencies].async-trait` is declared. | Remove or rename `Cargo.toml:21`'s declaration. |
| `[dependencies].clap` is declared. | Remove or rename `Cargo.toml:23`'s declaration. |
| `[dependencies].hex` is declared. | Remove or rename `Cargo.toml:24`'s declaration. |
| `[dependencies].reqwest` is declared. | Remove or rename `Cargo.toml:25`'s declaration. |
| `[dependencies].serde` is declared. | Remove or rename `Cargo.toml:26`'s declaration. |
| `[dependencies].serde_json` is declared. | Remove or rename `Cargo.toml:27`'s declaration. |
| `[dependencies].sha2` is declared. | Remove or rename `Cargo.toml:28`'s declaration. |
| `[dependencies].thiserror` is declared. | Remove or rename `Cargo.toml:29`'s declaration. |
| `[dependencies].tokio` is declared. | Remove or rename `Cargo.toml:30`'s declaration. |
| `[dependencies].tracing` is declared. | Remove or rename `Cargo.toml:31`'s declaration. |
| `[dependencies].tracing-subscriber` is declared. | Remove or rename `Cargo.toml:32`'s declaration. |
| `[dependencies].url` is declared. | Remove or rename `Cargo.toml:33`'s declaration. |
| `[dependencies].uuid` is declared. | Remove or rename `Cargo.toml:34`'s declaration. |
| `[dependencies].clap` requests `derive`. | Change `Cargo.toml:23`'s feature declaration. |
| `[dependencies].reqwest` disables default features and requests `json`, `multipart`, and `rustls-tls`. | Change `Cargo.toml:25`'s feature/default declaration. |
| `[dependencies].tokio` requests `rt-multi-thread`, `macros`, and `time`. | Change `Cargo.toml:30`'s feature declaration. |
| `[dependencies].uuid` requests `v4` and `serde`. | Change `Cargo.toml:34`'s feature declaration. |
| `[dev-dependencies].proptest` is declared. | Remove or rename `Cargo.toml:36`'s declaration. |
| `[dev-dependencies].sha2` is declared. | Remove or rename `Cargo.toml:37`'s declaration. |
| `[dev-dependencies].tokio` requests `rt` and `macros`. | Remove, rename, or change `Cargo.toml:39`'s declaration. |

Entries are unresolved declarations, not a resolved graph, installed software, or behavior.

<a id="r05"></a>
## R05 — Five falsifiable manifest axioms

| ID | Falsifiable SOURCE axiom | Static falsifier / limit |
|---|---|---|
| AX-SBOM-01 | The package identity is `sbom-publish`. | Change `package.name`; no tool identity outside this manifest follows. |
| AX-SBOM-02 | Five package metadata values inherit from the workspace. | Change a `.workspace` declaration; resolved values remain UNKNOWN. |
| AX-SBOM-03 | Lint configuration inherits from the workspace. | Change `[lints]`; no lint evaluation follows. |
| AX-SBOM-04 | One binary and one library each declare a name and source path. | Change a target field; no file, build, link, or execution claim follows. |
| AX-SBOM-05 | Normal/dev dependency blocks and named test/example blocks are distinct. | Move/remove a block or declaration; no resolution or execution claim follows. |

<a id="r06"></a>
## R06 — Test and example declaration invariant

| Atomic SOURCE predicate | Static falsifier / evidence |
|---|---|
| `[[test]]` declaration `prop_sbom` names path `tests/prop_sbom.rs`. | Change/remove its name or path at `Cargo.toml:41-43`. |
| `[[test]]` declaration `adversarial` names path `tests/adversarial.rs`. | Change/remove its name or path at `Cargo.toml:45-47`. |
| `[[example]]` declaration `generate` names path `examples/generate.rs`. | Change/remove its name or path at `Cargo.toml:49-51`. |
| `[[example]]` declaration `validate_ntia` names path `examples/validate_ntia.rs`. | Change/remove its name or path at `Cargo.toml:53-55`. |
| `[[example]]` declaration `ingest_dt_retry` names path `examples/ingest_dt_retry.rs`. | Change/remove its name or path at `Cargo.toml:57-59`. |

Names and paths do not prove test/example files exist, are selected, compile, or run.

<a id="r07"></a>
## R07 — Source map and implementation boundaries

The following are SOURCE observations of the pinned Rust files. They describe
what the code is written to do; they do not establish that it compiles, runs,
or succeeds.

| Source surface | Bounded SOURCE observation | Static evidence |
|---|---|---|
| Library API | `lib.rs` exposes `dt`, `error`, `metrics`, `ntia`, `publisher`, `purl`, and `tsa`; it re-exports the publisher types, NTIA validator types/function, `SbomError`, and `TsrToken`. | `tools/sbom-publish/src/lib.rs` |
| Publisher API | `SbomPublisher` declares async `generate`, `ingest_dt`, and `validate_ntia`; `DefaultSbomPublisher` stores a Tokio mutex behind `Arc`. `generate` invokes `cargo cyclonedx`, reads JSON, normalises PURLs, ensures a serial number, extracts metadata/count, applies strict NTIA validation, then attempts TSA timestamping. | `src/publisher.rs`: trait and `DefaultSbomPublisher` implementation |
| CLI | `main.rs` declares `generate`, `validate-ntia`, `attest-tsa`, and `ingest-dt`; the dispatcher reads/writes local files and calls library functions. The error mapper assigns the listed exit codes. | `src/main.rs`: `Commands`, `run`, `exit_code_for_error` |
| NTIA | `validate_ntia_json` checks author and timestamp presence, component name/version/supplier/identifier coverage, and a nonempty dependencies array. Its strict predicate requires supplier coverage ≥95% and 100% for name, version, identifier, author, timestamp, and dependency presence; auditor mode emits a report. | `src/ntia.rs`: `validate_ntia_json`, `has_placeholder_supplier` |
| PURL | `normalise_purl` strips existing qualifiers, adds a workspace `vcs_url` discriminator when requested, and forms a `pkg:crates/` alias for `pkg:cargo/`; document normalization adds properties for alias and patched-local status. | `src/purl.rs`: `normalise_purl`, `normalise_sbom_purls`, `is_valid_purl` |
| TSA | `request_tsa_timestamp` hashes SBOM bytes, builds a JSON request with digest and nonce, posts to the supplied URL, and returns response bytes plus binding fields. `verify_tsr_binding` compares the recomputed digest with the token field. | `src/tsa.rs`: request and verification functions |
| Dependency-Track | `ingest_into_dt` posts multipart data to `/api/v1/bom`, retries at delays 0/1/4/16 seconds except for 400/422, then writes bytes under `.sbom_fallback_queue` after exhaustion. The API key wrapper reads a named environment variable and redacts its `Debug` output. | `src/dt.rs`: `DtApiKey`, `ingest_into_dt`, `write_fallback_queue` |
| Metrics and errors | Metrics are process-local atomics emitted through tracing; `SbomError` declares generation, validation, TSA, DT, API-key, I/O, JSON, and HTTP cases. | `src/metrics.rs`, `src/error.rs` |
| Tests and examples | Property tests cover NTIA omissions, PURL handling, component counts, and TSR hash binding; adversarial tests declare six scenarios; three examples contain generator, validator, and DT-ingestion entry points. | `tests/prop_sbom.rs`, `tests/adversarial.rs`, `examples/*.rs` |

The package description at `Cargo.toml:3` remains descriptive text, not separate
evidence for its claims. Where it conflicts with source, preserve both the text
and the source observation as distinct evidence; do not infer intended behavior.

<a id="r08"></a>
## R08 — Evidence limits and explicit unknowns

The source tree is readable at the pinned commit, but workspace resolution and
dependency graph; build, test, and example execution; actual CLI input/output;
remote TSA or Dependency-Track responses; metric export/scraping; deployment,
release effects, and independent-review outcome remain UNKNOWN. Source does not
prove that the described fallback, validation, or timestamp path works under
operational conditions.

Source-local boundaries that must remain visible: the `generate` example calls
the publisher but then writes its own SBOM output; it does not establish CLI
output behavior. Publisher generation falls back to the Unix epoch string when
the SBOM timestamp field is absent. `generate` accepts and logs
`cargo_lock_path`, but its subprocess arguments do not pass that path. It also
creates a `serial_number` value before inserting only when JSON lacks
`serialNumber`, so the returned field may not match a preexisting document value.

The publisher's TSA request failure path records an unavailable outcome and
returns a `SignedSbom` without a token. The CLI has a separate `attest-tsa`
command whose error maps to exit 3. `ingest_dt` passes `spec_version` as the DT
project version.

`dt.rs` describes token/UUID extraction and local fallback persistence; neither
yields a verified DT project UUID nor a production queue. The CLI help text,
source comments, error docs, and implementation do not agree uniformly about
whether TSA/DT failures block publication; runtime release policy is UNKNOWN.

The adversarial test file's header lists five scenarios while the source
declares `scenario_06` as well. Its DT case sets a process environment variable,
uses localhost port 19999, and removes `.sbom_fallback_queue` after the call;
these are test-source side effects, not evidence they occurred here.

The verified canonical OKF route is [ADR-S12-001](../../../knowledge/adr/adr-s12-001-sbom-cyclonedx-ntia-tsa-dt.md), routed only and neither copied nor revalidated here.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Ownership guide](../../../../.claude/skills/own-sbom-publish/SKILL.md#s01)
