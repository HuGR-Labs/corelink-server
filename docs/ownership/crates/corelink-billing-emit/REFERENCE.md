---
schema: corelink-ownership/1.1
document: reference
package: corelink-billing-emit
manifest: crates/corelink-billing-emit/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-billing-emit-structural-normalization-20260921
---

# `corelink-billing-emit` reference

[R01](#r01) · [R02](#r02) · [R03](#r03) · [R04](#r04) · [R05](#r05) · [R06](#r06) · [R07](#r07) · [R08](#r08)

Static source reference for package `corelink-billing-emit`, baseline
`9f372cc1f`. Evidence class is **SOURCE** unless stated otherwise.

<a id="r01"></a>
## R01 — Identity and boundary

The manifest names this workspace package `corelink-billing-emit`. `lib.rs`
exports event, idempotency, sink, emitter, audit, and error surfaces plus
`billing_emit_schema_version() -> 1`. It directly depends on
`corelink-analytics`, used by the event `Region` field. This is not evidence
that the package exports analytics or that an analytics pipeline runs.

Evidence: `crates/corelink-billing-emit/Cargo.toml`; `src/lib.rs`; `src/event.rs`.

<a id="r02"></a>
## R02 — Owned modules and provider boundary

`event`, `idempotency`, `sink`, `emitter`, `audit`, and `error` are this
package's source territory. `R2UsageSink`, `BillingAuditSink`,
`IdempotencyTracker`, and `UsageEventEmitter` are ports. The checked concrete
implementations are in-memory (and failing test fixtures); the emitter is
parameterized over audit/idempotency but directly holds `InMemoryR2UsageSink`.
Thus the source owns port contracts and fake behavior, not an R2, audit-outbox,
or D1 provider implementation.

Evidence: `src/{sink,audit,idempotency,emitter}.rs`.

<a id="r03"></a>
## R03 — Public event contract

`UsageEvent::new` sets `specversion` to `"1.0"`, event type to
`"corelink.billing.usage.recorded"`, content type to `"application/json"`,
subject to `tenant:<uuid>`, derives a unit from the selected kind, initializes
the idempotency slot to zero, and rejects malformed `YYYY-MM` periods.
`UsageEvent` remains publicly deserializable, so that constructor validation
does not by itself prove validation of every deserialization path.

Evidence: `src/event.rs` (`UsageEvent`, `UsageEvent::new`,
`validate_billing_period`).

<a id="r04"></a>
## R04 — Enumerations and error surface

The source maps eight event kinds to strings and exposes two units (`bytes`,
`op_count`); both enums are `#[non_exhaustive]`. Audit has four canonical type
strings: usage emitted, duplicate rejected, sink failure, and idempotency
collision. `BillingEmitError`, `R2UsageSinkError`, and audit-sink error expose
canonicalization, audit, sink, duplicate/collision, and internal failure
surfaces. Additive enum compatibility is reserved; consumer compatibility is
not established here.

Evidence: `src/{event,audit,error}.rs`; `tests/prop_billing_emit.rs`.

<a id="r05"></a>
## R05 — Canonical key and tracker invariant

`compute_canonical_bytes_for_idem` clones an event, zeros its `idem_key`, and
JCS-serializes it. `derive_idem_key_from_canonical` BLAKE3-hashes those bytes
into `IdemKey`. The in-memory tracker keys accepted values by tenant and key;
same bytes return `DuplicateRejected`, divergent bytes return
`IdempotencyCollision`, and a first sight records both key and bytes. This is
an in-memory, mutex-protected invariant—not durable uniqueness, collision-rate,
or cross-process evidence.

Evidence: `src/idempotency.rs`.

<a id="r06"></a>
## R06 — Exact emitter, audit, and append behavior

In `InMemoryUsageEventEmitter::emit`, canonicalization/key derivation precede
`IdempotencyTracker::insert`. On `Accepted`, the source emits `UsageEmitted`
before `InMemoryR2UsageSink::append`; on sink error it tries `SinkFailure`
audit before returning the sink error. On duplicate/collision, the source emits
the respective audit record after the tracker produced that result. Therefore
the source does **not** enforce audit-before-idempotency mutation or rollback
of an accepted tracker entry if later audit/sink work fails.

The in-memory sink serializes a line, uses
`usage/{tenant}/{YYYY-MM}/{seq:08}.usage.ndjson`, rejects an already-recorded
key, then pushes the buffer/key and advances that pair's sequence. This proves
the fake's append-only guard only.

Evidence: `src/emitter.rs`; `src/sink.rs`.

<a id="r07"></a>
## R07 — Test target and static checks

The manifest names `tests/prop_billing_emit.rs` as `prop_billing_emit`.
Its source names checks for key determinism, duplicate handling, append-only
rejection, tenant isolation, audit decision arms, JCS stability, period shape,
and taxonomy pinning. This reference records the target and its assertions; it
does not report a test execution.

Evidence: `Cargo.toml`; `tests/prop_billing_emit.rs`.

<a id="r08"></a>
## R08 — Unknowns and non-claims

No SOURCE evidence in this record establishes a real R2 object, Queue, Cron,
D1 table, audit sink, handler mount, credentials, network call, retention,
delivery, retry, or runtime reachability. Nor does it establish a complete
consumer graph, deployed wire compatibility, or provider transactionality.
Obtain resolved/deployment/runtime evidence from the relevant provider or
composition owner before making those claims.

Continue with [blast radius](BLAST_RADIUS.md#b01) and [maintenance](MAINTENANCE.md#m01).
