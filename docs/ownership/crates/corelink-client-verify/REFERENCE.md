---
schema: corelink-ownership/1.1
document: reference
package: corelink-client-verify
manifest: crates/corelink-client-verify/Cargo.toml
source_commit: 16d9f0303d849a1ab3df14688bd2c7cdbfee8140
profile: S
state: candidate
evidence_set: client-verify-source-static-16d9f0303
---

# corelink-client-verify — ownership reference

S-profile SOURCE reference. “Invariant” below means a source-falsifiable
predicate at the pinned commit, not an executed verification, an SDK result, an
FFI/ABI result, telemetry delivery, or runtime guarantee.

[Identity](#r01) · [Root](#r02) · [Config](#r03) · [Verify](#r04) · [Errors](#r05) · [Stream](#r06) · [FFI](#r07) · [Limits](#r08)

<a id="r01"></a>
## R01 — Manifest identity invariant

**Predicate:** the manifest names `corelink-client-verify`, declares `rlib`,
`cdylib`, and `staticlib`, and defines empty-default `stream` and `ffi` feature
names. **Falsifier:** rename/remove a listed crate type, feature, or default
entry. **SOURCE:** `Cargo.toml:1-28`. **Unknown:** selected features, resolved
dependencies, or emitted artifacts.

<a id="r02"></a>
## R02 — Root export invariant

**Predicate:** `lib.rs` unconditionally exposes `config`, `digest`, `error`,
and `verifier`, and re-exports `VerifyConfig`, digest items, `VerifyError`,
`opt_out_total`, and `ClientVerifier`. **Falsifier:** remove an unconditional
module declaration or named `pub use`. **SOURCE:** `src/lib.rs:54-77`.
**Unknown:** downstream import, compile, or use.

<a id="r03"></a>
## R03 — Default configuration axiom

**Predicate:** `VerifyConfig::new` constructs `enabled: true` and
`warn_on_optout: true`; `Default` delegates to `new`; `disabled` constructs
`enabled: false` and `warn_on_optout: true`; both fields are `pub(crate)`.
**Falsifier:** alter a literal, field visibility, or `Default` delegation.
**SOURCE:** `src/config.rs:22-57, 89-108`. **Unknown:** a caller's chosen
configuration or warning observation.

<a id="r04"></a>
## R04 — Synchronous branch axiom

**Predicate:** `ClientVerifier::verify` returns `VerifyDisabled` before hash
calculation when disabled; otherwise it computes `Digest::compute(body)` and
uses `verify_constant_time(expected)` to select success or `DigestMismatch`.
**Falsifier:** remove/reorder the disabled branch, replace either named call, or
change its result arm. **SOURCE:** `src/verifier.rs:92-119`. **Unknown:** hash
implementation behavior, caller handling, or any downloaded body.

<a id="r05"></a>
## R05 — Error taxonomy axiom

**Predicate:** `VerifyError` has `DigestMismatch { expected, computed }` and
`VerifyDisabled`, and `VerifyError::code` maps them to their respective
constants. **Falsifier:** alter a variant, fields, match arm, or constant
mapping. **SOURCE:** `src/error.rs:22-68`; `src/digest.rs:11-27`.
**Unknown:** rendered error delivery or consumer exception mapping.

<a id="r06"></a>
## R06 — Stream feature-mode invariant

**Predicate:** `stream` gates the root `stream` module, `StreamVerifyError`,
and `STREAM_CHUNK_BYTES`; `verify_stream` passes the config enabled flag to the
state implementation, whose disabled branch emits `VerifyDisabled` and whose
EOF branch compares the final digest with `ConstantTimeEq`.
**Falsifier:** remove a gate, alter the flag passage/disabled branch, or replace
the EOF comparison. **SOURCE:** `Cargo.toml:20-28`; `src/lib.rs:63-65,79-83`;
`src/stream.rs:43-101,130-188`. **Unknown:** feature selection, reader I/O,
chunk delivery, or stream consumption.

<a id="r07"></a>
## R07 — FFI feature-mode invariant

**Predicate:** `ffi` gates `ffi`; its verify entry rejects oversized body
length, null handle, inconsistent body pointer, malformed digest length, and
null digest pointer before creating input slices, then maps the three verifier
results to the declared tags. **Falsifier:** remove the root gate, a named
validation return, or a final result match arm. **SOURCE:** `Cargo.toml:20-28`;
`src/lib.rs:67-72`; `src/ffi.rs:75-95,225-318`. **Unknown:** C ABI loading,
pointer validity supplied by callers, or foreign-language behavior.

<a id="r08"></a>
## R08 — Evidence boundary invariant

**Predicate:** this pack is limited to manifest and local source text at the
pinned commit. **Falsifier:** represent source inspection as Cargo/test,
SDK/client, ABI, network, runtime, deployment, or independent-review evidence.
The verified canonical OKF route is [SDK reference](../../../knowledge/ops/sdk-reference.md);
it is routed only, not copied or revalidated. **Unknown:** reverse dependency
graph, feature resolution, compilation, tests, SDK integration, FFI ABI use,
logging/metric observation, runtime, deployment, and review.

[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Back to identity](#r01)
