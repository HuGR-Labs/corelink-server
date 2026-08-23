---
type: "StorageComponent"
title: "D1 CONFIG_DB"
description: "The tenant control-plane store: the native container reaches Cloudflare D1 (CONFIG_DB) over the REST HTTP API for tenant/billing/PAT rows — this half is live. The per-region 'DO config singleton' (`corelink-config-do`) is DESIGNED-NOT-WIRED: only `InMemoryConfigSingletonStore` exists, there is no Durable Object binding in `worker/` or `wrangler.toml`, and its only consumer is `corelink-ops` tooling — treat any DO/`storage.transaction()` claim below as the intended production target, not current behaviour."
source_files:
  - "crates/corelink-container/src/customer_d1.rs"
  - "crates/corelink-config-do/src/lib.rs"
  - "crates/corelink-config-do/src/types.rs"
  - "crates/corelink-config-do/src/store.rs"
  - "crates/corelink-container/src/storage/d1_http.rs"
checkpoint_sha: "8af9ed65caf286d3f800e91d3f823face3aefd31"
provenance: "AUTHORED"
tags: ["storage", "d1", "config-db", "control-plane", "tenant-isolation"]
timestamp: "2026-06-26T00:00:00Z"
---

# D1 CONFIG_DB

CONFIG_DB is CoreLink's control-plane database: tenant rows, storage-state byte accounting, billing,
tier selections, and PAT rows. The native container cannot use the Worker's fast D1 binding, so it
reaches CONFIG_DB over the Cloudflare D1 REST API (`api.cloudflare.com/.../d1/database/{db}/query`) via
an async `reqwest` client, while the sync handler traits (shared with the wasm Worker) bridge to it with
`block_in_place`. Every read is parameterised and tenant-scoped, and every write fails CLOSED rather than
fabricate data. Distinct from this REST-reached database, `corelink-config-do` DESIGNS a per-region
**config singleton** — a schema-versioned payload (feature flags + rate-limit tunables + retention
policies) meant to live in a Durable Object with CAS-atomic updates via `storage.transaction()`.
**That DO does not exist today**: the crate ships only `InMemoryConfigSingletonStore`
(`crates/corelink-config-do/src/store.rs`), there is no config-DO binding anywhere in `worker/` or
`wrangler.toml`, and its only consumer is `corelink-ops` tooling (`crates/corelink-ops/Cargo.toml`) —
NOT the deployed Worker or the native container. The two D1-over-HTTP control-plane round-trips this
store costs on the CAS read path are the subject of the
[CAS hot-path latency](/storage/cas-hot-path-latency.md) concept, and its quota rows back
the [billing quota check](/flows/billing-quota-check.md).

# Role
- The live D1-backed customer/control-plane handler serving real dashboard data from deployed tables
  (`crates/corelink-container/src/customer_d1.rs:1-25`).
- The async D1 HTTP client that targets the CF D1 REST query endpoint from the native container
  (`crates/corelink-container/src/storage/d1_http.rs:1-23`).

# How it works
1. D1 is reachable outside a CF Worker via the REST API; the native container posts parameterised SQL to
   `api.cloudflare.com/.../d1/database/{db}/query` (`crates/corelink-container/src/storage/d1_http.rs:1-23`).
2. The client builds its query URL from the account id + database id and carries the CF API token as a
   bearer (`crates/corelink-container/src/storage/d1_http.rs:90-118`).
3. `D1CustomerHandler` serves dashboard data from deployed tables (`tenant`, `tenant_storage_state`,
   `tenant_billing`, `tier_selections`, `pat`, `customer_audit_events` — migration 0077 — which now
   backs BOTH the audit log AND the overview's `recent_activity` feed since BE-3, and
   `monthly_request_counts` — migration 0071 — which the usage endpoint reads for its real
   `request_count` since BE-1a) — real where a table
   exists, honest empty/501 where it does not
   (`crates/corelink-container/src/customer_d1.rs:1-25`). The `customer_audit_events` rows are written
   UNSKIPPABLE / fail-CLOSED (INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER), not best-effort: a failed insert now
   surfaces as `AuditFailed` → 503 and the control-plane mutation never commits
   (`crates/corelink-container/src/customer_d1.rs:1-25`).
4. Each sync handler call bridges to the async D1 client through `block_in_place` +
   `Handle::current().block_on`, valid because the native server is multi-thread `#[tokio::main]`
   (`crates/corelink-container/src/customer_d1.rs:26-39`).
5. **DESIGNED, not wired.** The per-region config singleton is *intended* to be a DO holding a
   schema-versioned `ConfigPayload` (feature flags, rate-limit tunables, retention), updated via CAS and
   logged to a D1 `config_change_log` with 90d retention (`crates/corelink-config-do/src/lib.rs:1-26`).
   Today the only implementation is `InMemoryConfigSingletonStore`
   (`crates/corelink-config-do/src/store.rs:1-20`) — a per-instance `Arc<Mutex<>>`, not a DO.
6. A config update takes an `expected_version` and returns `VersionConflict` on mismatch. The crate's own
   doc comment says the check runs inside a DO `storage.transaction()` **"in the Cloudflare DO production
   implementation"** — but no such DO binding exists in `worker/` or `wrangler.toml`; in the actual
   `InMemoryConfigSingletonStore` the mutex is what gives sequential-access atomicity, not a DO transaction
   (`crates/corelink-config-do/src/lib.rs:42-49`).

# Invariants
- Every D1 statement is parameterised (positional binds) and tenant-scoped via `WHERE tenant_id = ?` —
  `INV-TENANT-ISOLATION` (`crates/corelink-container/src/customer_d1.rs:48-51`).
- A D1 transport / non-2xx / decode error fails CLOSED to a 500, never to fabricated empty data
  (`crates/corelink-container/src/customer_d1.rs:46-48`).
- The CF API bearer token is redacted in the client's manual `Debug` impl — a `{:?}` can never print it
  (`crates/corelink-container/src/storage/d1_http.rs:44-51`).
- The config schema uses `#[serde(deny_unknown_fields)]`, so schema drift is a hard error, never a
  silent default — the attribute sits on the `ConfigPayload` struct at
  `crates/corelink-config-do/src/types.rs:34-35`.
- A config CAS update with a stale `expected_version` is rejected with `VersionConflict` — no last-writer
  -wins clobber (`crates/corelink-config-do/src/lib.rs:42-49`).

# Gotchas
- CONFIG_DB is reached over the slow REST control-plane HTTP API from the container, NOT the Worker's
  fast binding — two of these round-trips in series on the CAS hot path are the measured ~1.5s warm cost.
- **⚠️ Designed-not-wired: the `corelink-config-do` "DO config singleton" is gated-inert.** There is no
  config Durable Object binding in `worker/` or `wrangler.toml`, and `grep -rn "impl ConfigSingletonStore"`
  finds only `InMemoryConfigSingletonStore` (`crates/corelink-config-do/src/store.rs`). Its only consumer
  is `corelink-ops` tooling (`crates/corelink-ops/Cargo.toml`) — not the deployed Worker, not the native
  container. Any doc comment in the crate that says "in the Cloudflare DO production implementation" is
  describing the intended target, not current behaviour; do not cite it as evidence a config DO is live.
  This is a separate D1 surface from the REST-reached CONFIG_DB tables described above, which ARE live.

# Citations
1. `crates/corelink-container/src/customer_d1.rs:1-25` — D1-backed customer handler + deployed-table source-of-truth matrix (incl. the BE-3 `recent_activity` = `customer_audit_events` overview row and the BE-1a usage `request_count` = `monthly_request_counts` row).
2. `crates/corelink-container/src/customer_d1.rs:26-39` — the sync↔async `block_in_place` D1 bridge.
3. `crates/corelink-container/src/customer_d1.rs:46-48` — fail-CLOSED on D1 transport error (→ 500, never fabricated data).
4. `crates/corelink-container/src/customer_d1.rs:48-51` — parameterised, tenant-scoped SQL (`WHERE tenant_id = ?`).
5. `crates/corelink-config-do/src/lib.rs:1-26` — per-region DO config singleton (schema-versioned payload, CAS update, 90d change log).
6. `crates/corelink-config-do/src/types.rs:34-35` — `#[serde(deny_unknown_fields)]` on the `ConfigPayload` struct (schema-drift hard error).
7. `crates/corelink-config-do/src/lib.rs:42-49` — CAS `expected_version` → `VersionConflict`, DO-transactional.
8. `crates/corelink-container/src/storage/d1_http.rs:1-23` — D1 reached via the CF REST query endpoint from the native container.
9. `crates/corelink-container/src/storage/d1_http.rs:44-51` — CF API token redacted in the manual `Debug` impl.
10. `crates/corelink-container/src/storage/d1_http.rs:90-118` — query-URL construction from account + database id + bearer token.
11. `crates/corelink-config-do/src/store.rs:1-20` — `InMemoryConfigSingletonStore` is the ONLY implementation of `ConfigSingletonStore`; no DO exists.
12. `crates/corelink-ops/Cargo.toml` — the only in-tree crate that depends on `corelink-config-do`, confirming it is ops tooling, not the deployed Worker/container.
