---
schema: corelink-ownership/1.1
document: reference
package: corelink-handler-cas
manifest: crates/corelink-handler-cas/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-handler-cas-structural-normalization-20260921
---

# corelink-handler-cas — reference

This reference records static source at `12ca4a8d1`. It is not evidence that
an HTTP handler is mounted, a concrete object store is reached, REAPI is
served, wasm runs, audit events persist, or SLI observations are delivered.

[Identity](#r01) · [Surface](#r02) · [Envelopes](#r03) · [Audit/SLI](#r04) ·
[Digest](#r05) · [Fake operations](#r06) · [Trait/fake gap](#r07) ·
[Limits](#r08).

<a id="r01"></a>
## R01 — Package identity and local territory

`crates/corelink-handler-cas/Cargo.toml` names this package
`corelink-handler-cas`. `src/lib.rs` exposes the local audit, digest algorithm,
error, handler, observer, and request modules, then re-exports their named
surface. The manifest declares `thiserror`, `corelink-slo`, `sha2`, and `hex`;
it has no storage, HTTP, REAPI, or wasm implementation dependency. Its
description calls it a handler skeleton and defers a real CF-Worker
implementation. This is package/source evidence, not proof of selected target
or runtime composition.

<a id="r02"></a>
## R02 — Public trait and error surface

The root exports `CasReadHandler`, `CasWriteHandler`, `CasDeleteHandler`, and
`CasListHandler`, plus `InMemoryCasHandler`. `CasReadHandler::exists` defaults
to calling `read`, mapping only `NotFound` to `Ok(false)`; it is correct by its
documented default but is not a cheap probe. `exists_batch` defaults to `None`.
The documented `Some(Ok(flags))` batch shape requires request-order flags with
equal cardinality, while `Some(Err(_))` represents the first request-order
error. `CasHandlerError` is `#[non_exhaustive]` and has `NotFound`,
`HashMismatch`, `CrossTenantDenied`, `AuditFailed`, `ObjectTooLarge`, and
`Internal`. Evidence: `src/lib.rs`, `src/handler.rs`, `src/error.rs`.

<a id="r03"></a>
## R03 — Request and response envelopes

Read requests carry path tenant/hash, principal, caller tenant, timestamp,
explicit `DigestAlgo`, and optional maximum bytes. `new` defaults to BLAKE3;
`with_algo` and `with_max_bytes` modify those fields. Writes separately carry
physical `tenant`, `caller_tenant`, and `accounting_tenant`; ordinary `new`
copies physical tenant into accounting tenant, while `for_public_namespace`
fixes physical tenant to `_public`. Authorization is exactly accounting tenant
equals caller tenant and physical tenant equals caller tenant or `_public`.

Delete requests/responses and list requests/responses are also local envelopes.
The delete response can report reclaimed bytes; the list fake's pagination
uses a hash cursor. The `1..=1000` list limit appears as route-facing request
documentation, but `InMemoryCasHandler::list` itself only applies `max(1)`.
Source: `src/request.rs`, `src/handler.rs`.

<a id="r04"></a>
## R04 — Local audit and SLI seams

`AuditSink::emit` returns `Result<(), String>` and `SliObserver::observe` is
infallible. `AuditEvent` includes kind, tenant, hash, principal, and timestamp;
the twelve enum variants map to canonical dotted slugs. `InMemoryAuditSink`
captures rows or can be configured to fail each emit. `InMemorySliObserver`
captures observations and offers snapshot/count methods. These are local seams
and capture fakes, not the `corelink-audit` envelope, durable sink, SLO
aggregator, Prometheus registry, or evidence of delivery. Source: `src/audit.rs`
and `src/observer.rs`.

<a id="r05"></a>
## R05 — Digest partition and helpers

`DigestAlgo` has only `Blake3` and `Sha256`; its default is `Blake3`. The in-memory write path dispatches through `expected_content_hash`: `Sha256` uses `sha256_hex`, which constructs `sha2::Sha256` and hex-encodes its final digest; `Blake3` uses `fake_hash`. `fake_hash` is a deterministic 64-character string formed from big-endian input length, first byte (or zero), then zero padding. It is not BLAKE3. Thus this crate statically proves real SHA-256 helper code and a synthetic BLAKE3-tagged fake path, not storage-level

digest verification, native BLAKE3, or Bazel REAPI interoperability. Source: `src/digest_algo.rs`, `src/handler.rs`, `Cargo.toml`.

<a id="r06"></a>
## R06 — In-memory fake operations

`InMemoryCasHandler` stores `Vec<u8>` in a mutex-protected map keyed by
`(tenant, hash)`, with test-only-looking public `seed` and mismatch-injection
handles. Read authorizes exact physical/caller tenant equality, records a
pre-lookup attempt row, optionally returns the injected mismatch, retrieves by
map key, applies `max_bytes` after obtaining the in-memory bytes, then records
served/correctness observations on its normal success path. It does not hash
stored bytes on ordinary reads.

Write uses the request authorization predicate, writes an attempt row, verifies
the claimed hash using the selected helper, inserts into the map, and then emits
the committed row. Delete is idempotent map removal and reports removed length;
list filters this tenant's map entries, sorts hash strings, and uses the fixed
epoch timestamp. These observations are precisely fake semantics, not object
storage semantics. Source: `src/handler.rs`.

<a id="r07"></a>
## R07 — Falsifiable contract observations

<a id="inv-001"></a>
**INV-001 — explicit namespace authorization.** In the fake, ordinary
read/delete/list access succeeds only when request tenant equals caller tenant;
write additionally permits only `_public` as a physical namespace different
from the caller, and only with matching accounting/caller tenant. A change to
`check_tenant` or `is_authorized_for_caller` falsifies this observation.

<a id="inv-002"></a>
**INV-002 — default probe is not storage HEAD.** A non-overriding
`CasReadHandler::exists` invokes `read`; its `exists_batch` returns `None`. An
override is required before a concrete storage-backed implementation can claim
metadata-only or batched behavior.

<a id="inv-003"></a>
**INV-003 — ordering is partial, not atomic delivery.** Fake write/delete
attempt rows precede map mutation; committed rows follow mutation. Consequently
a failed committed emit can return `AuditFailed` after mutation. Also, several
early `map_err(AuditFailed)` and lock-error paths return before their local SLI
closure runs. Trait prose requires observations on every return, but source
does not mechanically enforce it and the fake does not emit them on those
identified early-error paths. A control-flow change can falsify either fact.

<a id="inv-004"></a>
**INV-004 — fake read is not ordinary hash re-verification.** Other than the
explicit injection branch, fake read returns bytes found under the requested
map key without calling `expected_content_hash`, `fake_hash`, or `sha256_hex`.
Changing that flow can falsify this boundary.

<a id="r08"></a>
## R08 — Evidence limits and unknowns

Static source does not establish concrete trait implementors' behavior, full
reverse consumers, feature/target resolution, API compatibility, route verbs or
HTTP statuses, actual BLAKE3 verification, REAPI protocol behavior, object
storage/KV reachability, audit durability, metric aggregation, secrets,
traffic, deployment, or cold review. Declared test files indicate intended
coverage only; no test was executed for this reference.

[Ownership guide](../../../../.claude/skills/own-corelink-handler-cas/SKILL.md#s01) ·
[Blast radius](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01).
