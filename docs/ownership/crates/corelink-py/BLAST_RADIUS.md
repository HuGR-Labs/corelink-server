---
schema: corelink-ownership/1.1
document: blast_radius
package: corelink-py
manifest: tools/sdks/python/Cargo.toml
source_commit: 3feae2baed63061354533ffdfe2d94acfd24aa9a
profile: S
state: candidate
evidence_set: corelink-py-source-static-20260920
---

# corelink-py — blast radius

Each relation is one bounded SOURCE arrow at the pinned commit. It is not proof of resolution, compilation, PyO3 generation, Python use, client I/O, release, runtime, or deployment.

[Manifest](#b01) · [Verifier](#b02) · [Annotations](#b03) · [Local paths](#b04) · [Textual references](#b05) · [Closure](#b06)

<a id="b01"></a>
## B01 — Packaging, feature, and linker declarations

`pyproject.toml` build-system → `maturin` backend (`maturin>=1.7,<2`); `[tool.maturin]` → `extension-module`, `corelink_py`, and `strip=true`; `Cargo.toml` feature → `pyo3/extension-module`; `build.rs` environment gates → cdylib-only `-undefined dynamic_lookup` arguments when extension-module is active for Apple targets. These are four direct declared seams; changing the named declarations changes the local packaging/build contract. **Evidence:** `pyproject.toml:1-3,18-26`; `Cargo.toml:16-19,42-45`; `build.rs:35-63`. **Failure boundary:** no backend/feature resolution, build-script execution, target selection, linker acceptance, extension/wheel artifact, or Python import is observed.

<a id="b02"></a>
## B02 — Binding source → client-verifier relation

`src/lib.rs` → `corelink-client-verify`: the binding imports `ClientVerifier`, `Digest`, `VerifyConfig`, and `VerifyError`, then uses them in construction, `get`, and `put`. Changing a named import/call can alter this local source seam. **Evidence:** `Cargo.toml:42-45`; `src/lib.rs:24,65-78,115-147`. **Failure boundary:** no digest implementation behavior, verifier execution, body provenance, or client outcome is observed.

<a id="b03"></a>
## B03 — PyO3 annotations → module registration relation

The exact `#[pyclass(name = "CoreLinkClient")]` at `src/lib.rs:32`, its `#[pymethods]` impl, and the `StatResult` annotation → `#[pymodule] corelink_py`: the root declares two annotated classes and registers both in the annotated module function. Changing an annotation, type, or `add_class` call can alter the declared bridge seam. **Evidence:** `src/lib.rs:32,47-52,169-201`. **Failure boundary:** no macro expansion, generated API, module load, or Python compatibility is observed.

<a id="b04"></a>
## B04 — Caller data → local stub branches relation

Constructor and method inputs → local `VerifyConfig`, digest, and placeholder branches: `client_verify` selects a config; `get` uses empty bytes; `put` returns locally computed digest text; `stat` produces fixed metadata. Changing an input, branch, or result can alter static behavior described by R04/R05. **Evidence:** `src/lib.rs:65-165`. **Failure boundary:** no caller, PAT handling outcome, HTTP/gRPC transport, CAS persistence, or runtime observation is established.

<a id="b05"></a>
## B05 — Consumer, installation, and release relation unknown

No bounded SOURCE arrow from this binding to a consumer, installed distribution, package index, or release is asserted. `pyproject.toml` supplies declared project metadata only; it does not identify a published or installed distribution. Those relationships are **UNKNOWN** because the assigned evidence is local static source/configuration. **Falsifier:** add a bounded, named source declaration that directly identifies such a relation. **Failure boundary:** README/examples, declared project metadata, package-index text, or a repository-wide search do not establish a built, installed, or released package.

<a id="b06"></a>
## B06 — Relation closure and unknowns

Coverage ends at direct local configuration/source arrows B01–B05, including the declared maturin and build-script linker gates in B01. The [SDK reference](../../../knowledge/ops/sdk-reference.md) is the verified canonical OKF route only—not a relation and not revalidated here. Unknown: reverse graph, resolved features/dependencies/backend, build-script execution, macro expansion, build/link outcome, generated extension/wheel/stub, installation, Python import/execution, consumers, package release, network/CAS, telemetry, runtime, deployment, and independent review.

[Reference](REFERENCE.md#r01) · [Maintenance](MAINTENANCE.md#m01) · [Back to manifest](#b01)
