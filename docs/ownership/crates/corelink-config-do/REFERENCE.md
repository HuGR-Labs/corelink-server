---
schema: corelink-ownership/1.1
document: reference
package: corelink-config-do
manifest: crates/corelink-config-do/Cargo.toml
source_commit: cca798ff5bc2df660ecf2570ed243eb9775ff3d0
profile: S
state: draft
evidence_set: corelink-config-do-structural-normalization-20260921
---

# corelink-config-do — ownership reference

Static-source reference for the package’s typed configuration and in-memory
models. It records declarations and source-visible control flow; it does not
prove Durable Object, D1, Prometheus, propagation, rollback, or deployed
runtime behavior.

[Identity](#r01) · [Boundary](#r02) · [Build declaration](#r03) · [Contracts](#r04) · [Hash](#r05) · [Store](#r06) · [Models](#r07) · [Evidence](#r08).

<a id="r01"></a>
## R01 — Identity

| Field | Static evidence |
|---|---|
| Package | `corelink-config-do`, `crates/corelink-config-do/Cargo.toml` |
| Source modules | `error`, `hash`, `metrics`, `propagation`, `store`, `types`, and `validation`, all declared in `src/lib.rs` |
| Test target | `prop_cas`, at `tests/prop_cas.rs` |
| Crate-root surface | `ConfigError`; metrics observer/fakes; propagation types; audit/store traits and fixtures; payload/domain types; `validate_payload`; `VERSION` |

<a id="r02"></a>
## R02 — Boundary

The checked implementation is `InMemoryConfigSingletonStore`, built around a
per-instance `Arc<Mutex<InMemoryState>>`; the snapshot and metrics facilities
are also in-memory/no-op interfaces. Comments describe possible production
systems, but this record makes no claim that a Durable Object, D1, Prometheus
exporter, queue/poll propagation mechanism, or rollback endpoint exists or is
wired.

<a id="r03"></a>
## R03 — Manifest and portability declaration

The manifest declares `thiserror`, `serde`, `serde_json`, `serde_jcs`, `sha2`,
`hex`, `tracing`, `async-trait`, and workspace `uuid` with `v7` and `serde`
features. Its `cfg(target_arch = "wasm32")` dependency section declares UUID
again with `v7`, `serde`, and `js`. This is a manifest declaration only; no
wasm build or runtime result is asserted. Development dependencies declare
`proptest` and `tokio-test`.

<a id="r04"></a>
## R04 — Types, validation, and errors

`ConfigPayload` contains a schema version plus `BTreeMap` collections for
feature flags, rate-limit tunables keyed by `RateLimitKey`, and retention
policies; it and inbound domain structs use `serde(deny_unknown_fields)` where
defined. `SUPPORTED_SCHEMA_VERSION` is `1` in the checked source. The crate
also exposes `ConfigVersionEntry`, `ChangeType`, and `AdminActor`.

`validate_payload` rejects a non-supported schema version, feature rollout
above 100, zero rate-limit refill, and retention TTL above 365 days. The
non-exhaustive `ConfigError` defines source-level categories for version
conflict, schema invalidity, expired/unknown rollback targets, propagation
timeout, backend, and serialization failure. These are interfaces and local
logic, not validation of external inputs at a live boundary.

<a id="r05"></a>
## R05 — Canonical payload hash

`compute_payload_hash` serializes `ConfigPayload` through `serde_jcs`, feeds
the resulting bytes to SHA-256, and returns `[u8; 32]`; `hash_to_hex` uses
`hex::encode`. The in-memory update and rollback paths place the computed hash
in a `ConfigVersionEntry`. Deterministic behavior is source intent based on
the selected types and code path; no stored-record, interoperation, or runtime
integrity result is established.

<a id="r06"></a>
## R06 — Store and audit contracts

`ConfigSingletonStore` declares async `update`, `current`, `rollback_to`, and `history`. In the selected in-memory implementation, update validates and hashes before locking; under the mutex it checks expected version, emits its `ConfigAuditSink` entry, then mutates payload/history/current state. The rollback path looks up its target, applies the source’s 90-day window, retrieves and re-validates its historical payload, hashes it, emits audit, and then mutates to a new version. Failed `emit` returns an error before the

later in-memory mutation statements in both paths. `history` returns retained entries newest first, and the update/rollback paths lazily prune expired non-current entries. This describes only the checked in-memory control flow.

<a id="r07"></a>
## R07 — Metrics and propagation models

`metrics.rs` defines five `corelink_admin_config_*` metric-name constants,
outcome enums, the `MetricsObserver` trait, and no-op/in-memory observers.
`propagation.rs` defines `ConfigChangeEvent` and `ConfigSnapshot`; the latter
advances only when the supplied version is strictly greater than its locked
local version and otherwise ignores the supplied payload. There is no source
execution evidence here for metric registration/export or event delivery,
polling, timing, or propagation across workers.

<a id="r08"></a>
## R08 — Evidence and unknowns

Evidence set: the manifest; all seven `src` modules; root workspace membership
and dependency declaration; static `corelink-ops` manifest/import/re-export
relations; and `tests/prop_cas.rs`. The property-test source names CAS,
schema-drift, rollback-target, and payload-validation assertions, plus a
fail-closed audit regression; it is not an execution result.

Unknown: Cargo resolution and feature selection, compilation/test execution,
reverse-graph completeness, all external consumers, stored-data compatibility,
Durable Object/D1 implementation or persistence, Prometheus wiring, actual
propagation and rollback behavior, telemetry delivery, deployment, and review
status.

[Impact map](BLAST_RADIUS.md#b01) · [Maintenance](MAINTENANCE.md#m01) · [Start](#r01)
