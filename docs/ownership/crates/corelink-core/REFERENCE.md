---
schema: corelink-ownership/1.1
document: reference
package: corelink-core
manifest: crates/corelink-core/Cargo.toml
source_commit: 5d617634662ee9475dc66cb83b5a57296eb75dc2
profile: S
state: author_validated
evidence_set: core-static-source-20260920
---

# corelink-core — ownership reference

Static source reference for the workspace apex types crate. It records contracts observable in the pinned files, not whether legacy types were migrated, a Cargo feature was selected, or any path reaches a deployment.

[Identity](#r01) · [Boundary](#r02) · [Map](#r03) · [Contracts](#r04) · [Invariants](#r05) · [Dependencies](#r06) · [Errors](#r07) · [Unknowns](#r08).

<a id="r01"></a>
## R01 — Identity and scope

| Field | Static evidence |
|---|---|
| Package / manifest | `corelink-core` / `crates/corelink-core/Cargo.toml` |
| Declared role | Apex cross-cutting type, error, and clock surface |
| Public surface covered here | `TenantId`, `Digest`, `Region`, `SecretWrap`, `CoreError`, `Clock` |
| Internal CoreLink dependencies | None declared in this manifest |
| Direct manifest consumers found | `corelink-ac`, `corelink-adapter-host`, `corelink-cas`, `corelink-server` (manifest `crates/corelink-container/Cargo.toml`) |

Source: `Cargo.toml`, `src/lib.rs`. Direct manifest presence is not proof that a target or feature selects the dependency.

<a id="r02"></a>
## R02 — Ownership boundary

This crate owns the source contracts of the six listed surfaces. `TenantId`, `Digest`, and `Region` are typed representations; `SecretWrap` owns an explicit redaction/exposure boundary; `CoreError` is a narrow cross-cutting error surface; `Clock` is an abstraction only.

It does not implement hashing, a concrete clock, storage, authorization, deployment, or a migration from scattered legacy types. Source comments name future or external locations, but this reference does not treat them as live wiring.

<a id="r03"></a>
## R03 — Source map

| Path | Observable responsibility |
|---|---|
| `src/lib.rs` | Declares modules and reexports the six covered surfaces |
| `src/types/tenant.rs` | UUID wrapper and canonical text rendering |
| `src/types/digest.rs` | Fixed-width digest construction, parsing, formatting, serde, and constant-time comparison |
| `src/types/region.rs` | Four region variants, strings, and serde form |
| `src/types/secret.rs` | Secret-string wrapper, explicit exposure, and redacted debug |
| `src/errors.rs`, `src/errors/digest_parse.rs` | Cross-cutting error variants and digest parsing conversion |
| `src/time.rs` | Clock trait and default millisecond conversion |

All listed paths were read from the pinned tree. No source-only observation proves a consumer has adopted these types.

<a id="r04"></a>
## R04 — Public contracts

[API-001](#api-001) · [API-002](#api-002) · [API-003](#api-003) · [API-004](#api-004) · [API-005](#api-005) · [API-006](#api-006).

<a id="api-001"></a>
### API-001 — Tenant identity text

`TenantId` wraps a `Uuid`; construction accepts a UUID and display plus canonical text render that UUID's standard textual form. The source describes canonical UUIDv7 lowercase hyphenated text, while the constructor itself wraps any provided `Uuid`; therefore version validation is not established here. Changing formatting risks callers that persist or compare text. Source: `types/tenant.rs`.
[Contract index](#r04)

<a id="api-002"></a>
[↩](#r01)
### API-002 — Digest representation and formatting

`Digest` holds exactly 32 bytes. It accepts a 64-byte-character hexadecimal input with either case, renders 64 lowercase hexadecimal characters, serializes through that text form, and exposes raw bytes. Parsing rejects another length or a non-hex byte with its position. Parsing does not compute content integrity. Source: `types/digest.rs`.
[Contract index](#r04)

<a id="api-003"></a>
[↩](#r01)
### API-003 — Digest comparison boundary

`Digest` provides a comparison using `subtle::ConstantTimeEq`; derived equality remains available for non-adversarial use. The observable contract is a boolean comparison of two fixed-width digest values, not a timing property for an entire request or parser. Do not replace the explicit method where latency observation matters without a separate security decision. Source: `types/digest.rs`.
[Contract index](#r04)

<a id="api-004"></a>
[↩](#r01)
### API-004 — Region vocabulary

`Region` has four variants: Wnam, Enam, Weur, and Sam. `ALL` contains those four, `as_str` returns `wnam`, `enam`, `weur`, or `sam`, and serde uses lowercase names. Adding, removing, renaming, or recasing a value is a representation compatibility change. Source: `types/region.rs`.
[Contract index](#r04)

<a id="api-005"></a>
[↩](#r01)
### API-005 — Secret exposure and formatting

`SecretWrap` accepts a string or secret string, exposes plaintext only through an explicit method, and formats debug output as a redacted marker. It has no display implementation in the inspected source. The zeroization behavior belongs to the wrapped dependency; this document records no secret value. Source: `types/secret.rs`, `Cargo.toml`.
[Contract index](#r04)

<a id="api-006"></a>
[↩](#r01)
### API-006 — Error and clock semantics

`CoreError` carries storage, mutex-poison, invalid-configuration, and digest-parse conditions and is non-exhaustive. `Clock: fmt::Debug + Send + Sync` requires `now() -> SystemTime`; its default milliseconds conversion uses the wall-clock duration, returns zero before the epoch, and saturates overflow at `u64::MAX`. It does not supply a concrete implementation or monotonic method. Source: `errors.rs`, `errors/digest_parse.rs`, `time.rs`.
[Contract index](#r04)
[↩](#r01)

<a id="r05"></a>
## R05 — Falsifiable invariants

[INV-001](#inv-001) · [INV-002](#inv-002) · [INV-003](#inv-003) · [INV-004](#inv-004).

<a id="inv-001"></a>
### INV-001 — Digest text round trip

For any 32-byte value constructed by the public byte constructor, lowercase rendering is 64 characters and parsing that rendering returns the same value. This is falsified by a width, alphabet, case, or parser mismatch. Source: `types/digest.rs` and its unit tests.
[Invariant index](#r05)

<a id="inv-002"></a>
[↩](#r01)
### INV-002 — Region set and spelling

The exported region set has four enumerated values, and each `as_str` result is its lowercase token. This is falsified by a missing, additional, or differently spelled value. Source: `types/region.rs` and its unit tests.
[Invariant index](#r05)

<a id="inv-003"></a>
[↩](#r01)
### INV-003 — Secret debug redaction

Formatting a `SecretWrap` with debug emits the fixed redacted marker rather than the wrapped content. This is falsified by a debug result containing supplied sentinel text. No real secret is needed to test it. Source: `types/secret.rs` and its unit tests.
[Invariant index](#r05)

<a id="inv-004"></a>
[↩](#r01)
### INV-004 — Clock boundary conversion

The default millisecond method derives from `now`, maps a pre-epoch result to zero, and caps an unrepresentable millisecond count at `u64::MAX`. This is falsified by a clock fixture returning a contrary result. Source: `time.rs`.
[Invariant index](#r05)
[↩](#r01)

<a id="r06"></a>
## R06 — Dependency and consumer evidence

`Cargo.toml` declares serde, thiserror, uuid, subtle, and secrecy, but no dependency whose package name starts `corelink-`. A static search of workspace manifests finds direct entries in `corelink-ac`, `corelink-adapter-host`, `corelink-cas`, and `corelink-server` at `crates/corelink-container/Cargo.toml`.

This is a direct-manifest inventory only. It does not prove package selection, enabled features, source imports, reverse transitive consumers, legacy replacement, or runtime reachability.

<a id="r07"></a>
## R07 — Failure boundaries

| Surface | Observable failure or limit | Boundary |
|---|---|---|
| Digest | Wrong text length or non-hex byte is rejected | No content hashing is performed by parsing |
| CoreError | Narrow listed variants, non-exhaustive enum | Not a catch-all for context-owned errors |
| SecretWrap | Exposure requires explicit method | No credential operation or logging authority |
| Clock | Pre-epoch/overflow conversion saturates | No concrete clock or monotonic guarantee |

Source: `types/digest.rs`, `errors.rs`, `types/secret.rs`, `time.rs`.

<a id="r08"></a>
## R08 — Explicit unknowns

The source does not establish whether scattered legacy types have migrated to this crate; whether any direct manifest consumer selects it for a target or feature; which external effects use its formatted identity or digest values; whether concrete clocks preserve intended monotonic behavior; or whether a deployment reaches this crate.

No secret values were inspected or recorded. Static documentation validation is not semantic approval or cold review.

[Relations](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
