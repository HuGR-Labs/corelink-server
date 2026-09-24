---
schema: corelink-ownership/1.1
document: reference
package: corelink-byok-fuzz
manifest: crates/corelink-byok/fuzz/Cargo.toml
source_commit: 1177dad2ca2a9f21c29b5a118aa7944b77147798
profile: S
state: draft
evidence_set: w016-byok-fuzz-source-1177dad2
---

# corelink-byok-fuzz — ownership reference

Static source map for a separate Cargo fuzz package with two declared envelope
targets. It records target intent and assertions only; no target, provider,
runtime, storage, or external operation was observed.

[Identity](#r01) · [Boundary](#r02) · [Source map](#r03) · [Contracts](#r04) ·
[State and invariants](#r05) · [Targets](#r06) · [Failures](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity and function

| Field | Verified source fact |
|---|---|
| Package / manifest | `corelink-byok-fuzz` / `crates/corelink-byok/fuzz/Cargo.toml` |
| Source pin | `1177dad2ca2a9f21c29b5a118aa7944b77147798` |
| Shape | Own `[workspace]`; `cargo-fuzz = true`; two explicit binary targets |
| Edition / version / publication | 2021 / `0.0.0` / `publish = false`; license `UNLICENSED` |
| Target execution | UNKNOWN; declaration and source are not run evidence |

<a id="r02"></a>
## R02 — Ownership boundary

| Role | Owner evidenced by source | Limit |
|---|---|---|
| Harness implementation | This package: `fuzz/Cargo.toml` and both `fuzz_targets/*.rs` files | Does not own parent cryptography or service composition |
| Called API and implementation | `corelink-byok`; see [parent reference](../corelink-byok/REFERENCE.md#r02) | This fuzz source does not approve parent API or persisted-data compatibility |
| Local test double | `StubKms` in `envelope_roundtrip.rs` | In-process harness code; no external provider call is shown |
| Composition root / runtime operator / review authority | UNKNOWN | No verified owner or route in this source set |

The targets do not establish customer input, credentials, provider use, KMS
activity, D1/R2 access, or deployment. The target comments mention storage
systems; this package source set does not verify those claims.

**Canonical concept:** [BYOK envelope encryption at rest](../../../knowledge/storage/byok-envelope-encryption.md).
That route preserves the cross-cutting policy; this reference does not restate it.

<a id="r03"></a>
## R03 — Implementation map

| Source | Source-visible responsibility | Nature |
|---|---|---|
| `fuzz/Cargo.toml` | Separate workspace, lints, dependency keys, and two `[[bin]]` declarations | Owned manifest |
| `fuzz_targets/wrapped_dek_parse.rs` | Feeds bytes to `WrappedDek` and `EncryptedBlob` JSON parsing; on successful parse, serializes and reparses | Owned target |
| `fuzz_targets/envelope_roundtrip.rs` | Parses a byte layout, creates a local `StubKms`, and calls parent `EnvelopeEncryptor` methods | Owned target calling parent API |
| `split_input`, `key_id`, `StubKms` | Private input splitting, fixed synthetic key identifier, and in-process trait stub | Target-local helpers |

The manifest description says the parse target includes envelope decrypt;
its inspected source calls serde parsing only. Encrypt/decrypt calls appear in
`envelope_roundtrip`, using `StubKms`. Record this source/description mismatch
without inferring test coverage.

<a id="r04"></a>
## R04 — Target contracts

These are binary-target contracts, not public Rust library exports.

**Index:** [API-001](#api-001) · [API-002](#api-002).

<a id="api-001"></a>
### API-001 — `wrapped_dek_parse`

**Entry:** `fuzz_target!(|data: &[u8]| ...)` in `wrapped_dek_parse.rs`.
**Input / precondition:** arbitrary bytes; JSON parse success is conditional.
**Assertions:** successful `WrappedDek` parse must serialize and reparse, keep
`encryption_context` Some/None polarity, provider, region, and key identifier;
successful `EncryptedBlob` parse must serialize and reparse. Parse errors are
ignored. **Effects / compatibility:** local assertions only; not a backward
compatibility test over stored envelopes. **Proof:** pinned target source.

[Target index](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — `envelope_roundtrip`

**Entry:** `fuzz_target!(|data: &[u8]| ...)` in `envelope_roundtrip.rs`.
**Input:** two length bytes capped at 64, UTF-8 tenant and blob strings, then
plaintext bytes; malformed/short inputs return early. **Path:** a current-thread
Tokio runtime, local cache, `EnvelopeEncryptor<StubKms>`, matching-context
encrypt/decrypt, then a changed-tenant decrypt. **Assertions:** recovered
plaintext equals input and changed-tenant decrypt is `Err`. Runtime/cache/encrypt
construction errors return early. The stub stores DEK bytes directly and has no
transport. **Proof:** pinned target source; no execution observed.

[Target index](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — State and falsifiable source invariants

State visible here is per-target-call memory: input slices, parsed values,
temporary serialized bytes, a target-local cache, and the in-process stub. No
package-owned persistent state or external I/O is present in the enumerated
target source; this says nothing about the parent crate's consumers.

**Index:** [INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003).

<a id="inv-001"></a>
### INV-001 — Successful parsed values roundtrip through serde

For successful parses, the target serializes and reparses `WrappedDek` and
`EncryptedBlob`; `WrappedDek` additionally checks the listed fields in API-001.
Removing those calls/assertions falsifies this source predicate. It is not proof
that every input parses or that old persisted data remains compatible.

[Invariant index](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Matching-context target path preserves supplied plaintext

When input splitting, runtime/cache creation, and stub-backed encryption succeed,
`envelope_roundtrip` asserts recovered bytes equal its plaintext slice. The target
uses a fresh encryptor for the changed-tenant case and asserts that result is an
error. These are target assertions, not a claim of cryptographic certification.

[Invariant index](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — `StubKms` methods remain in-process

The target-local `wrap_dek`, `unwrap_dek`, and `check_access` bodies return local
values and contain no transport call in the pinned source. Adding external I/O
would falsify this source predicate and require a new scope review.

[Invariant index](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Dependency and target declarations

| Declaration | Manifest fact | Selection / execution limit |
|---|---|---|
| Targets | `wrapped_dek_parse` and `envelope_roundtrip`; each has `test = false`, `doc = false`, `bench = false` | No target selection or execution observed |
| Local dependency | `corelink-byok = { path = ".." }` | Declared edge only; see REL-001 |
| Other direct keys | `libfuzzer-sys = 0.4`, `serde_json = 1`, `arbitrary = 1` with `derive`, `tokio = 1` with `macros, rt`, `async-trait = 0.1` | Resolved versions/features UNKNOWN |
| Harness lint declarations | Rust `unsafe_code = forbid`; Clippy `unwrap_used`, `panic`, `indexing_slicing`, `todo`, `unimplemented` denied | Declaration only; no lint/build result |

The target source uses `libfuzzer_sys`, `serde_json`, `tokio`, and `async_trait`.
No `arbitrary` use appears in the two enumerated target files; generated or
resolved behavior was not checked. No package lockfile, resolved graph, selected
feature set, target platform, corpus, or build result was established.

<a id="r07"></a>
## R07 — Failures and observability

| Source condition | Target result visible in source | Limit |
|---|---|---|
| JSON parse returns `Err` | That parse branch is skipped | No parser result was observed at runtime |
| Input split, runtime, cache, or encrypt setup returns no value/error | Target returns early | Coverage of remaining assertions is unknown |
| A stated roundtrip or mismatch assertion fails | Panic in the target source | No crash artifact or fuzz result observed |
| Stub `unwrap_dek` receives non-32-byte ciphertext | Returns `DekLengthInvalid` | Local stub branch only; no actual KMS behavior |

No metrics, logs, persisted state, or live error signal is owned or observed here.

<a id="r08"></a>
## R08 — Evidence and unknowns

Evidence: pinned `fuzz/Cargo.toml` and the two target sources at
`1177dad2ca2a9f21c29b5a118aa7944b77147798`; the scoped comparison against
integration baseline `d80f245e0be3c61c250249de8292e42a6cd8ef5d` found no diff in
root `Cargo.toml` or `crates/corelink-byok/`. No Rust or fuzz command was run.

Unknown: resolved graph, exact selected target/features, target behavior under
execution, corpus quality, resource limits, minimization/reproduction results,
parent wire compatibility with stored values, inverse graph completeness,
workflow ownership, runtime, deployment, and independent review.

[Impact](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Back to identity](#r01)
