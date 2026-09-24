---
schema: corelink-ownership/1.1
document: reference
package: corelink-cli
manifest: tools/cli/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: candidate
evidence_set: corelink-cli-source-static-20260921
---

# corelink-cli — ownership reference

S-profile SOURCE reference. Each invariant is a source-falsifiable predicate at the pinned commit, never a claim that the `corelink` binary can run, a command is reachable, a file is changed, a request is made, a URL resolves, telemetry arrives, or a package is released.

[Identity](#r01) · [Facades](#r02) · [Command declarations](#r03) · [Config/output](#r04) · [Five axioms](#r05) · [Telemetry source](#r06) · [Dependencies](#r07) · [Evidence boundary](#r08)

<a id="r01"></a>
## R01 — Manifest identity invariant

**Predicate:** `Cargo.toml` names `corelink-cli`, declares library name `corelink_cli` at `src/lib.rs`, and declares binary name `corelink` at `src/main.rs`; its Rust lint table forbids unsafe code. **Falsifier:** alter/remove any named package, target, path, or lint declaration. **SOURCE:** package and target declarations: `tools/cli/Cargo.toml:1-16`; unsafe-code lint: `tools/cli/Cargo.toml:18-19`. **Unknown:** target selection, compilation, binary existence, execution, platform compatibility, publishing, signing, and release.

<a id="r02"></a>
## R02 — Library facade invariant

**Predicate:** library root declares public `auth`, `config`, `error`, and `output` modules and path-based public `audit_export`, `verify_ndjson`, and `verify_ndjson_http` modules. **Falsifier:** remove/rename a listed public declaration or `#[path]` attribute. **SOURCE:** `tools/cli/src/lib.rs:19-48`. **Unknown:** external imports, consumer compilation, API compatibility, module initialization, or execution.

<a id="r03"></a>
## R03 — Command declaration invariant

**Predicate:** binary source declares `Cli` with optional global `OutputFormat`, a `Commands` subcommand field, and `Commands` variants including `Ls`, `Get`, `Put`, `Stat`, `Bench`, `Doctor`, `Version`, and `Config`. **Falsifier:** alter/remove a listed field, annotation, or variant. **SOURCE:** `tools/cli/src/main.rs:48-134`. **Unknown:** Clap macro expansion, accepted arguments, generated help, invocation, dispatch, exit status, or command reachability.

<a id="r04"></a>
## R04 — Configuration and output invariant

**Predicate:** `CorelinkConfig` has `auth`, `defaults`, and `telemetry` fields; `AuthConfig::redacted_pat` replaces a suffix with `***`; `OutputFormat` has `Text` default and `Json`; `Formatter::emit` selects `serde_json::to_string_pretty` for `Json` and `println!` for `Text`. **Falsifier:** alter a named field, branch, default annotation, replacement literal, or call. **SOURCE:** `src/config.rs:21-79`; `src/output.rs:11-58`. **Unknown:** config file contents, serialization outcome, stdout, redaction completeness, or user-visible output.

<a id="r05"></a>
## R05 — Five source axioms

**Axiom 1 — unsafe prohibition.** `src/lib.rs` declares `#![forbid(unsafe_code)]`. **Falsifier:** remove/alter that attribute. **SOURCE:** `src/lib.rs:19`. **Unknown:** expanded/dependency/generated code safety.

**Axiom 2 — resolver order.** `resolve_pat` reads nonempty `CORELINK_PAT` before `load_from_config`. **Falsifier:** reorder/remove the environment branch or fallback. **SOURCE:** `src/auth.rs:13-24`. **Unknown:** process environment or resolved credential.

**Axiom 3 — parser delegation.** `validate_pat_shape` calls `corelink_pat::parse_plaintext` and maps errors to `CliError::PatMalformed`. **Falsifier:** replace either call or mapping. **SOURCE:** `src/auth.rs:35-44`. **Unknown:** parser behavior, cryptographic verification, or authentication.

**Axiom 4 — default-off config.** `TelemetryConfig::default` sets `enabled: false`. **Falsifier:** alter that field literal. **SOURCE:** `src/config.rs:107-129`. **Unknown:** materialized config or user setting.

**Axiom 5 — false guard.** `emit_if_enabled` returns when `enabled` is false before `tokio::spawn`. **Falsifier:** remove/reorder the guard or spawn. **SOURCE:** `src/telemetry.rs:86-111`. **Unknown:** task scheduling, request creation, network, or delivery.

<a id="r06"></a>
## R06 — Telemetry payload source invariant

**Predicate:** `TelemetryEvent` declares exactly the listed fields `cli_version`, `os`, `subcommand`, `outcome`, `duration_ms`, and `anonymized_id`; `try_emit` text constructs a `reqwest` client with `TIMEOUT_MS` and calls `post(TELEMETRY_ENDPOINT).json(event).send().await`. **Falsifier:** alter a field or named call chain. **SOURCE:** `src/telemetry.rs:23-65,113-125`. **Unknown:** serialized payload, task execution, DNS, TLS, endpoint identity/reachability, receipt, retention, or telemetry effect.

<a id="r07"></a>
## R07 — Direct dependency declaration invariant

**Predicate:** manifest directly declares `corelink-pat`, `clap`, `tokio`, `serde`, `serde_json`, `reqwest`, and `tracing` among its dependencies. **Falsifier:** remove/rename a named dependency declaration. **SOURCE:** `tools/cli/Cargo.toml:28-64`. **Unknown:** lock resolution, selected versions/features, transitive graph, compiled code, dependency behavior, or licensing outcome.

<a id="r08"></a>
## R08 — Evidence boundary invariant

**Predicate:** this record uses only the pinned manifest and checked-in Rust text as SOURCE evidence. **Falsifier:** present it as proof of a runnable/released CLI, build, test, filesystem change, command behavior, credential, HTTP transaction, endpoint reachability, telemetry delivery, deployment, ownership assignment, or independent review. The verified canonical OKF route is [CLI reference](../../../knowledge/ops/cli-reference.md); it is routed only, not copied or revalidated. **Unknown:** all operational and review claims outside this static boundary.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Back to identity](#r01)
