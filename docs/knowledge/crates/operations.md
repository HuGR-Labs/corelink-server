---
type: "CrateCluster"
title: "Operations crate cluster (GC, replication, ratelimit, SRE)"
description: "The background-plane workers — garbage collection with reachability + degrade-mode, soft-delete-first eviction, and the per-tenant token-bucket rate limiter."
source_files:
  - "crates/corelink-gc/src/lib.rs"
  - "crates/corelink-gc/src/worker.rs"
  - "crates/corelink-eviction/src/lib.rs"
  - "crates/corelink-eviction/src/reachable.rs"
  - "crates/corelink-eviction/src/blob_meta.rs"
  - "crates/corelink-ratelimit/src/lib.rs"
  - "crates/corelink-ratelimit/src/key.rs"
checkpoint_sha: "5571b910292cbe3d53cbf46d7e0f120dbef877e2"
provenance: "AUTHORED"
tags: ["crates", "gc", "eviction", "ratelimit", "ops", "sre"]
timestamp: "2026-06-26T00:00:00Z"
---

# Operations crate cluster (GC, replication, ratelimit, SRE)

A content-addressed cache that never reclaims space goes bankrupt on storage COGS, and one that reclaims carelessly corrupts a tenant's data — so this cluster is the background plane that frees bytes safely and protects availability under load. It is grouped around two correctness spines: reclamation is reachability-gated and soft-delete-first (nothing is physically deleted while it could still be referenced, and the race-aware boundary uses a strict `created_at < evict_started_at_ms` so a concurrent write is protected), and rate limiting is per-tenant-isolated by a leftmost-tenant key so one tenant can never throttle another. `corelink-gc` is the GC worker + scheduler with an emergency degrade-mode stop; `corelink-eviction` is the LRU/TTL/quota-trigger evictor; `corelink-ratelimit` is the token-bucket engine.

# Role

The cluster backs the [GC / eviction operations](/ops/gc-eviction.md) runbook and the [tenant governance](/tenancy/governance.md) rate-limit control. These are scheduled/cron Durable-Object workers (GC, eviction) and a per-request DO singleton (rate limit) — the plane that keeps storage bounded and the data plane fair without ever touching the synchronous CAS hot path.

# How it works

- `corelink-gc` ships the GC run state machine (`GcPhase`/`GcStatus`, partial-UNIQUE on `WHERE status='running'`, monotone phase transitions, idempotent resume) plus the scheduler driving `cron tick → list candidate tenants → spawn worker per tenant` (`crates/corelink-gc/src/lib.rs:16-39`).
- GC carries a `gc-pause` emergency stop: a `DegradeProbe` consulted at every phase transition with a ≤100ms next-batch propagation gate, so an operator can halt reclamation fleet-wide (`crates/corelink-gc/src/lib.rs:40-56`).
- `corelink-eviction` is soft-delete-first and reachability-gated: it `UPDATE … SET deleted_at` (NEVER a direct R2 DELETE) only for blobs no live AC entry references, using the strict `<` evict-arm / `>=` protect-arm boundary mirroring the GC TLA semantics (`crates/corelink-eviction/src/lib.rs:42-57`).
- `corelink-ratelimit` is a lazy-refill token bucket keyed by a tenant-leftmost composite (`per_tenant` / `per_ip` / `per_tenant_per_endpoint`) per DO singleton, emitting RFC 6585 Retry-After seconds clamped to a floor/ceiling (`crates/corelink-ratelimit/src/lib.rs:11-18`; `crates/corelink-ratelimit/src/lib.rs:28-43`).

# Invariants

- GC reclamation is pausable: `transition_or_abort` probes degrade-mode at every phase boundary and routes a `gc-pause` to `finalize_aborted`, bounding blast radius of a bad sweep (`crates/corelink-gc/src/worker.rs:204-219`).
- `INV-EVICT-SOFT-DELETE-FIRST`: `soft_delete_for_eviction` sets `deleted_at_ms = Some(now_ms)` and returns `SoftDeleteOutcome::Deleted`, never a direct R2 DELETE; physical cleanup is GC's job post-grace (`crates/corelink-eviction/src/blob_meta.rs:336-354`).
- The reachable-check is race-aware: `find_active_reference` uses `row.created_at_ms < evict_started_at_ms` strict-evict, so a row with `created_at_ms >= evict_started_at_ms` (a blob written concurrently with the sweep) is protected (`crates/corelink-eviction/src/reachable.rs:177-199`).
- `INV-AVAIL-ISOLATION` / `INV-TENANT-ISOLATION`: the tenant-leftmost `BucketKey` (the `tenant_id` `Uuid` is the first field, a dedicated column the scope_key cannot bypass) makes rate-limit buckets per-tenant, so cross-tenant throttling is impossible by design (`crates/corelink-ratelimit/src/key.rs:77-87`).

# Gotchas

- Eviction is BLOB-only scope — chunks are owned by GC (S-06), not the evictor; mixing the two ownerships was an explicit Lote 10.7bis correction.
- TTLs are per-tier (free=7d … enterprise=365d default, admin-capped 730d) and the multipart reservation TTL is size-proportional (`max(60s, bytes/1MB/s × 2x), capped 7d`) so a 160 GiB upload no longer trips its own quota mid-stream.
- These crates ship pure-logic skeletons + in-memory fakes whose semantics mirror the embedded SQL migrations byte-for-byte; production binds them to real Cron Durable Objects.

# Citations

1. `crates/corelink-gc/src/lib.rs:16-39` — the GC run state machine + scheduler (cron → list → spawn-per-tenant).
2. `crates/corelink-gc/src/worker.rs:204-219` — `transition_or_abort`: degrade-probe at every phase boundary → `finalize_aborted` (`gc-pause`).
3. `crates/corelink-eviction/src/reachable.rs:177-199` — `find_active_reference` impl: the race-aware reachable check (`created_at_ms < evict_started_at_ms` strict; `>=` protected).
4. `crates/corelink-eviction/src/blob_meta.rs:336-354` — `soft_delete_for_eviction` impl: `INV-EVICT-SOFT-DELETE-FIRST` (sets `deleted_at_ms`, never a direct R2 DELETE).
5. `crates/corelink-ratelimit/src/key.rs:77-87` — tenant-leftmost `BucketKey` struct (`tenant_id` first field): `INV-AVAIL-ISOLATION` / `INV-TENANT-ISOLATION`.
6. `crates/corelink-ratelimit/src/lib.rs:11-18` — RFC 6585 Retry-After seconds clamped to a per-config floor + hard ceiling.
6b. `crates/corelink-ratelimit/src/lib.rs:28-43` — the tenant-leftmost bucket key + lazy-refill + RFC 6585 Retry-After.
