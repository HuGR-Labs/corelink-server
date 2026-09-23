---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-cli
manifest: tools/cli/Cargo.toml
source_commit: 398e586ccef712477f2a4ce51e026443b67e5747
profile: S
state: candidate
evidence_set: corelink-cli-source-static-20260921
---

# corelink-cli — blast radius

Each relation is one atomic, directed SOURCE arrow at the pinned commit. It records only a local declaration/call seam and its textual falsifier. No arrow establishes selected targets or dependencies, compilation, binary execution, reverse consumers, filesystem mutation, HTTP, endpoint reachability, telemetry delivery, release, runtime, deployment, or independent review.

[Targets](#b01) · [Facade](#b02) · [PAT parser](#b03) · [Config/output](#b04) · [Telemetry](#b05) · [Closure](#b06)

<a id="b01"></a>
## B01 — Library target declaration → library root relation

`tools/cli/Cargo.toml` `[lib]` declaration → `src/lib.rs`: the manifest text assigns the library target name and path. Changing that name or path can change this source-declaration seam. **Falsifier:** remove/alter the `[lib]` name or path. **Evidence:** `Cargo.toml:10-12`. **Failure boundary:** no target is selected, built, linked, installed, signed, released, or executable by this relation.

### B01b — Binary target declaration → binary root relation

`tools/cli/Cargo.toml` `[[bin]]` declaration → `src/main.rs`: the manifest text assigns the binary target name and path. Changing that name or path can change this source-declaration seam. **Falsifier:** remove/alter the `[[bin]]` name or path. **Evidence:** `Cargo.toml:13-16`. **Failure boundary:** no target is selected, built, linked, installed, signed, released, or executable by this relation.

<a id="b02"></a>
## B02 — Library root → local module relation

`src/lib.rs` public declarations → local auth/config/error/output and path-based audit/verify modules: the facade text exposes the named module declarations. Changing one declaration can change the static library surface. **Falsifier:** remove/rename one `pub mod` declaration or its `#[path]` source. **Evidence:** `src/lib.rs:24-48`. **Failure boundary:** no importer, downstream consumer, compatibility conclusion, module load, or call is observed.

<a id="b03"></a>
## B03 — Auth resolver → local validator relation

`src/auth.rs::resolve_pat` → `validate_pat_shape`: the resolver passes its selected `raw` value to the local validator. Changing that call can alter this source seam. **Falsifier:** remove/replace the `validate_pat_shape(&raw)` call. **Evidence:** `src/auth.rs:13-24`. **Failure boundary:** no environment value, config value, parser execution, token validity, cryptographic verification, authentication, or server acceptance is observed.

### B03b — Local validator → PAT parser relation

`src/auth.rs::validate_pat_shape` → `corelink_pat::parse_plaintext`: the local validator calls the named dependency parser. Changing that call or its error mapping can alter this source seam. **Falsifier:** replace the parser call or `CliError::PatMalformed` mapping. **Evidence:** `src/auth.rs:40-44`; `Cargo.toml:34-36`. **Failure boundary:** no parser execution, token validity, cryptographic verification, authentication, or server acceptance is observed.

<a id="b04"></a>
## B04 — Command output field → output type relation

`src/main.rs::Cli.output` → `src/output.rs::OutputFormat`: the command field names the output type. Changing that field type can change this static seam. **Falsifier:** remove/alter the `output` field or its `OutputFormat` type. **Evidence:** `src/main.rs:53-62`; `src/output.rs:11-20`. **Failure boundary:** no argument parsing, serialization, terminal output, process exit, or compatibility result is observed.

### B04b — Config auth field → auth type relation

`src/config.rs::CorelinkConfig.auth` → `src/config.rs::AuthConfig`: the configuration field names the auth-section type. Changing that field type can change this static seam. **Falsifier:** remove/alter the `auth` field or its `AuthConfig` type. **Evidence:** `src/config.rs:21-35,49-60`. **Failure boundary:** no config read/write, credential value, authentication, or compatibility result is observed.

### B04c — Config defaults field → defaults type relation

`src/config.rs::CorelinkConfig.defaults` → `src/config.rs::DefaultsConfig`: the configuration field names the defaults-section type. Changing that field type can change this static seam. **Falsifier:** remove/alter the `defaults` field or its `DefaultsConfig` type. **Evidence:** `src/config.rs:21-35,82-102`. **Failure boundary:** no config read/write, default materialization, endpoint use, or compatibility result is observed.

### B04d — Config telemetry field → telemetry type relation

`src/config.rs::CorelinkConfig.telemetry` → `src/config.rs::TelemetryConfig`: the configuration field names the telemetry-section type. Changing that field type can change this static seam. **Falsifier:** remove/alter the `telemetry` field or its `TelemetryConfig` type. **Evidence:** `src/config.rs:21-35,107-129`. **Failure boundary:** no config materialization, task spawn, HTTP request, endpoint reachability, receipt, or delivery is observed.

<a id="b05"></a>
## B05 — Config flag → telemetry guard relation

`TelemetryConfig.enabled` → `emit_if_enabled(enabled, event)`: main-source text reads `cfg.telemetry.enabled` and passes it to the telemetry guard; the guard returns before `tokio::spawn` for false. Changing the field, call argument, guard, or spawn order can change this static relation. **Falsifier:** alter the false literal, call argument, early return, or spawn placement. **Evidence:** `src/config.rs:107-129`; `src/main.rs:520-530`; `src/telemetry.rs:86-111`. **Failure boundary:** no config materialization, event construction, spawned task, HTTP request, DNS, TLS, endpoint reachability, receipt, or delivery is observed.

<a id="b06"></a>
## B06 — Relation closure and unknowns

Coverage ends at B01–B05 direct manifest/source arrows. Shared relation `repo:1232040291:boundary:cli-fuzz-parent-001` links this CLI facade to `corelink-cli-fuzz` REL-001; the peer must backlink this B06 record. The verified canonical [CLI reference](../../../knowledge/ops/cli-reference.md) is an OKF route only, not a relation and not revalidated here. Unknown: target/dependency resolution, macro expansion, binary/library build, command parsing/dispatch/execution, reverse consumers, config/filesystem state, PAT validity/authentication, output bytes, network and endpoint behavior, telemetry task/delivery/retention, package/release/signing, runtime, deployment, human ownership, and independent review.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Back to targets](#b01)
