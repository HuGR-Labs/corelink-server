---
name: own-corelink-client-verify
description: Maintain the source-scoped ownership record for corelink-client-verify without treating static package evidence as SDK, FFI consumer, execution, or runtime evidence.
metadata:
  evidence-set: client-verify-source-static-16d9f0303
  source-commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
  manifest: crates/corelink-client-verify/Cargo.toml
  package: corelink-client-verify
  profile: S
  evidence: source-static
---

# Own corelink-client-verify

Use this S-profile skill only for the static ownership boundary of
`corelink-client-verify`. It records manifest declarations, local Rust source,
exports, and feature gates; it does not certify a consumer, ABI use,
compilation, test, network, runtime, or deployment outcome.

[Scope](#s01) · [Evidence](#s02) · [Surface](#s03) · [Axioms](#s04) · [Relations](#s05) · [Quality](#s06) · [Handoff](#s07)

<a id="s01"></a>
## S01 — Scope and boundary

Own only this skill and `docs/ownership/crates/corelink-client-verify/{REFERENCE,BLAST_RADIUS,MAINTENANCE}.md`. Source review may read the package manifest and local `src/` files to anchor these records. Do not expand to SDK wrappers, consumer packages, generated headers, examples, tests, or runtime systems.

<a id="s02"></a>
## S02 — Evidence discipline

Class every claim as SOURCE, DOCUMENTARY, or UNKNOWN. SOURCE means the pinned manifest and local Rust text; DOCUMENTARY means the supplied profile-S checker and whitespace result. Compilation, ABI consumption, SDK behavior, test execution, logging delivery, metrics, network, and runtime are UNKNOWN unless independently evidenced.

<a id="s03"></a>
## S03 — Source contracts

Record the unconditional Rust exports separately from `stream` and `ffi` gates. Record `VerifyConfig` construction, `ClientVerifier::verify`, error taxonomy, and any FFI input/tag mapping as independent static contracts. A declared crate type or feature is not proof that an artifact was built or called.

<a id="s04"></a>
## S04 — Axioms

Preserve falsifiable source predicates: default configuration enables verification; the fields are crate-visible; disabled verification returns its explicit error; enabled verification uses `Digest::compute` and `verify_constant_time`; stream and FFI modules are separately gated. Every predicate needs one named source location and a concrete edit that falsifies it.

<a id="s05"></a>
## S05 — Relation method

Describe one directed source arrow per relation: manifest feature to gated root module, digest re-export to `corelink-hash`, verifier to config/error, stream to verifier/error, and FFI to verifier/error. Do not infer reverse dependencies, feature resolution, ABI compatibility, or any downstream invocation.

<a id="s06"></a>
## S06 — Quality and completion criteria

Success: all four artifacts identify package, manifest, pinned commit, S profile,
static boundary, unknowns, and the canonical OKF route. Completeness: S01–S07,
R01–R08, B01–B06, and M01–M06 exist; R predicates and B arrows are atomic and
falsifiable; M procedures name mode, prerequisite, predicate, evidence, stop,
and recovery. Do not copy or revalidate OKF policy.

| Condition | Action | Evidence | Stop when |
|---|---|---|---|
| Root, export, or feature changes | Re-record one gate/export predicate and one direct arrow per changed seam. | `Cargo.toml` and `src/lib.rs`. | A build, loaded symbol, or consumer result is needed. |
| Config, verifier, or error changes | Re-record constructor, branch, and error/tag predicates separately. | Named local source range. | A caller's use or log/metric delivery is needed. |
| Stream or FFI changes | Keep the feature mode and raw interface as distinct relations. | Manifest plus `stream.rs` or `ffi.rs`. | ABI safety, stream consumption, or runtime I/O is needed. |
| Validation requested | Run only the supplied S checker per artifact and whitespace check. | Checker output and diff status. | Semantic approval, Cargo/test, network, deploy, or runtime proof is requested. |

<a id="s07"></a>
## S07 — Definition of done and handoff

DoD: change only this skill and the three package records; run the supplied
profile-S checker once per artifact; run `git diff --check` against the pinned
baseline; then make the scoped commit. Handoff states paths, source relations,
documentary checks, the canonical OKF route, and unknowns. The route is context
only, never proof imported into this record.
