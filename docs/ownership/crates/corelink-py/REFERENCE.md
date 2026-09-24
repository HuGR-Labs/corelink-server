---
schema: corelink-ownership/1.1
document: reference
package: corelink-py
manifest: tools/sdks/python/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: candidate
evidence_set: corelink-py-source-static-20260920
---

# corelink-py — ownership reference

S-profile SOURCE reference. “Invariant” means a source-falsifiable predicate at the pinned commit; it is not an executed Python API, client, build, release, or runtime result.

[Identity](#r01) · [Feature](#r02) · [Bridge](#r03) · [Verify](#r04) · [Local operations](#r05) · [Module](#r06) · [Safety](#r07) · [Limits](#r08)

<a id="r01"></a>
## R01 — Package identity and Python packaging declarations

**Predicate:** `Cargo.toml` names `corelink-py`, sets `publish = false`, declares a `cdylib` named `corelink_py`, and lists `pyo3`, `corelink-client-verify`, and `tracing` as dependencies. `pyproject.toml` declares the `maturin>=1.7,<2` build requirement/backend and project metadata `corelink-py` version `0.1.0`, Python `>=3.10`, README, license text, and classifiers. **Falsifier:** alter/remove a named declaration. **SOURCE:** `tools/sdks/python/Cargo.toml:1-15,42-45`; `tools/sdks/python/pyproject.toml:1-16`. **Unknown:** backend resolution, selected Cargo resolution, emitted library/wheel, Python distribution contents, installation, or publication; metadata is a declaration, not proof of an uploaded package.

<a id="r02"></a>
## R02 — Extension feature invariant

**Predicate:** the crate owns `default = []` and `extension-module = ["pyo3/extension-module"]`; `[tool.maturin]` selects `features = ["extension-module"]`, `module-name = "corelink_py"`, and `strip = true`, and does not set `python-source`. **Falsifier:** alter these feature/backend declarations or add/remove `python-source`. **SOURCE:** `tools/sdks/python/Cargo.toml:16-19`; `tools/sdks/python/pyproject.toml:18-26`. **Unknown:** selected feature, backend interpretation, wheel contents, stripping outcome, or PyO3-generated extension behavior.

<a id="r03"></a>
## R03 — Annotated bridge invariant

**Predicate:** `PyCorelinkClient` has the exact `#[pyclass(name = "CoreLinkClient")]` annotation; its impl is `#[pymethods]`; `StatResult` is separately annotated `#[pyclass]`. **Falsifier:** remove/alter an annotation or named type. **SOURCE:** `src/lib.rs:32,47-52,169-183`. **Unknown:** generated names, conversion semantics, import, or call from Python.

<a id="r04"></a>
## R04 — Constructor and verify axiom

**Predicate:** `new` declares `client_verify=true`; true selects `VerifyConfig::new()`, while false emits the `COR_CAS_VERIFY_DISABLED` warning and selects `VerifyConfig::disabled()`. **Falsifier:** change the default, branch calls, warning code, or constructor result. **SOURCE:** `src/lib.rs:52-84`. **Unknown:** log delivery, counter observation, or a Python caller's configuration.

<a id="r05"></a>
## R05 — Local-operation axioms

**Get predicate:** `get` parses its supplied digest, fixes `body: &[u8] = b""`, and calls `verifier.verify` only when `client_verify_enabled` is true. **Falsifier:** replace the parse, body literal, guard, or verification call. **SOURCE:** `src/lib.rs:109-138`. **Unknown:** server request, download, or returned Python bytes.

**Put predicate:** `put` calls `Digest::compute(data)` and returns that digest's `to_hex()` text. **Falsifier:** replace either named call. **SOURCE:** `src/lib.rs:146-149`. **Unknown:** upload, persistence, or a client round trip.

**Stat predicate:** `stat` parses its supplied digest and constructs `StatResult { size_bytes: 0, exists: false }`. **Falsifier:** remove the parse or alter either fixed field. **SOURCE:** `src/lib.rs:155-165`. **Unknown:** metadata lookup, persistence, or a client round trip.

<a id="r06"></a>
## R06 — Module registration invariant

**Predicate:** `#[pymodule] fn corelink_py` adds `PyCorelinkClient` and `StatResult`. **Falsifier:** remove the annotation, function, or either `add_class` call. **SOURCE:** `src/lib.rs:193-201`. **Unknown:** module artifact, loader behavior, or Python-visible API.

<a id="r07"></a>
## R07 — Local unsafe prohibition invariant

**Predicate:** crate root declares `#![deny(unsafe_code)]`; static inspection of `src/lib.rs` finds that declaration but no `unsafe` operation. **Falsifier:** remove the deny attribute or add an unsafe operation. **SOURCE:** `src/lib.rs:18` and static text inspection. **Unknown:** macro expansion, dependencies, generated code, memory safety, or FFI safety.

<a id="r08"></a>
## R08 — Build-script linker gate and evidence boundary

**Predicate:** `build.rs` returns unless `CARGO_FEATURE_EXTENSION_MODULE` is set and `CARGO_CFG_TARGET_VENDOR` equals `apple`; only then it emits `-undefined` and `dynamic_lookup` through `cargo:rustc-link-arg-cdylib`, with rerun declarations for the script and both environment variables. This is a source-declared Apple-target cdylib linker workaround gated by the extension feature. **Falsifier:** alter either gate, emitted cdylib arguments, or rerun declarations. **SOURCE:** `tools/sdks/python/build.rs:35-63`.

The pack records static manifest, Python packaging metadata, build-script, and Rust binding text only. The verified canonical OKF route is [SDK reference](../../../knowledge/ops/sdk-reference.md); it is routed only, not copied or revalidated. **Unknown:** whether the script is selected/executed, target environment values, linker acceptance, backend/feature resolution, compilation, linking, wheel or extension artifact, Python execution/import, release, runtime, and review. No build success or runtime behavior is established.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Back to identity](#r01)
