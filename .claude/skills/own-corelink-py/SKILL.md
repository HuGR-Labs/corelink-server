---
name: own-corelink-py
description: Maintain the static Rust/PyO3 binding ownership record for corelink-py; do not infer a Python client, artifact, release, or runtime.
metadata:
  schema: "corelink-ownership/1.1"
  package: "corelink-py"
  manifest: "tools/sdks/python/Cargo.toml"
  source-commit: "3feae2baed63061354533ffdfe2d94acfd24aa9a"
  profile: "S"
  evidence-set: "corelink-py-source-static-20260920"
---

# Ownership — corelink-py

[Scope](#s01) · [Evidence](#s02) · [Surface](#s03) · [Axioms](#s04) · [Relations](#s05) · [Quality](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Scope and boundary

Own only this guide and `docs/ownership/crates/corelink-py/{REFERENCE,BLAST_RADIUS,MAINTENANCE}.md`. Review `tools/sdks/python/Cargo.toml` and static Rust binding source needed to anchor those records. Do not expand to a Python client, examples, documentation, package index, build artifact, release, or runtime.

<a id="s02"></a>
## S02 — Evidence discipline

Class claims as SOURCE, DOCUMENTARY, or UNKNOWN. SOURCE is the pinned manifest and checked-in Rust text; DOCUMENTARY is the supplied S-profile checker and whitespace diff. PyO3 expansion, Python import/call behavior, build/link, tests, logging delivery, network, client release, package publication, runtime, and deployment are UNKNOWN.

<a id="s03"></a>
## S03 — Source contracts

Record the `cdylib` declaration, `extension-module` feature, PyO3 annotations, module registration, and local `get`/`put`/`stat` branches separately. A `#[pymethods]` or `#[pymodule]` annotation is source evidence, not proof of a generated/importable Python API. A comment or README is not evidence of an HTTP client.

<a id="s04"></a>
## S04 — Axioms

Preserve falsifiable predicates: local source denies unsafe code; constructor default is `client_verify=true`; disabled construction selects `VerifyConfig::disabled()` and the named warning; `get` uses empty local bytes; `put` computes a digest locally; `stat` returns placeholder metadata. Every predicate needs a named source location and a concrete falsifying edit.

<a id="s05"></a>
## S05 — Relation method

Describe one directed source arrow per relation: manifest feature to PyO3, binding source to `corelink-client-verify`, PyO3 annotations to registered classes, and caller input to local branches. Do not infer feature resolution, generated bindings, reverse consumers, ABI compatibility, Python execution, or downstream invocation.

<a id="s06"></a>
## S06 — Quality and completion criteria

Success: four artifacts identify package, manifest, pinned commit, S profile, static boundary, unknowns, and the canonical OKF route. Completeness: S01–S07, R01–R08, B01–B06, M01–M06; R predicates and B arrows are atomic/falsifiable; M procedures name mode, prerequisite, predicate, evidence, stop, and recovery. Route to OKF without copying or revalidating it.

| Condition | Action | Evidence | Stop when |
|---|---|---|---|
| Manifest, feature, or annotation changes | Re-record its declaration/gate and direct arrow. | `Cargo.toml`, `src/lib.rs`. | Build, generated binding, or import result is needed. |
| Constructor, digest, stub, or error changes | Re-record one branch/call/result predicate per changed seam. | Named local source text. | Client/network/runtime behavior is needed. |
| Validation requested | Run only the S checker per artifact and whitespace check. | Checker output and diff status. | Cargo/test/network/deploy/runtime proof is requested. |

<a id="s07"></a>
## S07 — Definition of done and handoff

DoD: change only this guide and the three package records; run the S-profile checker once per artifact and `git diff --check` against `3feae2baed63061354533ffdfe2d94acfd24aa9a`; make the scoped commit. Handoff states paths, direct arrows, documentary checks, canonical OKF route, and unknowns. The route is context only, never imported proof.
