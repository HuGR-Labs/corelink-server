---
type: "CrateCluster"
title: "Operations crate cluster (GC, replication, ratelimit, SRE)"
description: "The background-plane workers — garbage collection with reachability + degrade-mode, soft-delete-first eviction, and the per-tenant token-bucket rate limiter."
source_files:
  - "crates/corelink-gc/src/lib.rs"
  - "crates/corelink-gc/src/run.rs"
  - "crates/corelink-gc/src/degrade.rs"
  - "crates/corelink-gc/src/worker.rs"
  - "crates/corelink-eviction/src/lib.rs"
  - "crates/corelink-eviction/src/reachable.rs"
  - "crates/corelink-eviction/src/blob_meta.rs"
  - "crates/corelink-ratelimit/src/lib.rs"
  - "crates/corelink-ratelimit/src/limiter.rs"
  - "crates/corelink-ratelimit/src/bucket.rs"
  - "crates/corelink-ratelimit/src/key.rs"
source_blobs:
  - "crates/corelink-gc/src/lib.rs@6851f4ddc0421cf900ac257fe95a92b2c04b382b"
  - "crates/corelink-gc/src/run.rs@24591a5c2b55beb45f35255ab97b7796ed3f912a"
  - "crates/corelink-gc/src/degrade.rs@cc8083ee0c3f24ffc66055996784bb54e98208ab"
  - "crates/corelink-gc/src/worker.rs@82048c93af987e9194d76123778e99c9a1d415a8"
  - "crates/corelink-eviction/src/lib.rs@47ee8e633f4ec00a9a5221390381b79d7e33e042"
  - "crates/corelink-eviction/src/reachable.rs@767578f6a50ef144ff7e4070d179d538344a7491"
  - "crates/corelink-eviction/src/blob_meta.rs@666009e8186b86382adaef9c7423a5a1c3316433"
  - "crates/corelink-ratelimit/src/lib.rs@f6d03b3f5d8eea26914501b0c8f8438a00f17a3f"
  - "crates/corelink-ratelimit/src/limiter.rs@24495e3d30240cedbe3a648768eb229d41fff62f"
  - "crates/corelink-ratelimit/src/bucket.rs@4ab6aa7c1baf67ad2824cc5a96b23ebb01f8da6b"
  - "crates/corelink-ratelimit/src/key.rs@1bb056c23a2b683edc8a9564529aa0bb1e9fcb1c"
checkpoint_sha: "fc7ec9bb9c5d8711cabc4b93c989062e71d2955f"
provenance: "AUTHORED"
tags: ["crates", "gc", "eviction", "ratelimit", "ops", "sre"]
timestamp: "2026-06-26T00:00:00Z"

---
# Operations crate cluster (GC, replication, ratelimit, SRE)

A content-addressed cache that never reclaims space goes bankrupt on storage COGS, and one that reclaims carelessly corrupts a tenant's data — so this cluster is the background plane that frees bytes safely and protects availability under load. It is grouped around two correctness spines: reclamation is reachability-gated and soft-delete-first (nothing is physically deleted while it could still be referenced, and the race-aware boundary uses a strict `created_at < evict_started_at_ms` so a concurrent write is protected), and rate limiting is per-tenant-isolated by a leftmost-tenant key so one tenant can never throttle another. `corelink-gc` is the GC worker + scheduler with an emergency degrade-mode stop; `corelink-eviction` is the LRU/TTL/quota-trigger evictor; `corelink-ratelimit` is the token-bucket engine.

# Role

The cluster backs the [GC / eviction operations](/ops/gc-eviction.md) runbook and the [tenant governance](/tenancy/governance.md) rate-limit control. These are scheduled/cron Durable-Object workers (GC, eviction) and a per-request DO singleton (rate limit) — the plane that keeps storage bounded and the data plane fair without ever touching the synchronous CAS hot path.

# How it works

- `corelink-gc` ships the GC run state machine — the executed `GcPhase::can_transition_to` enforces the monotone forward chain (Idle→Mark→Sweep→PhysicalDelete→Reconcile→Completed, any-pre-terminal→Failed, everything else rejected) (`crates/corelink-gc/src/run.rs:102-119`).
- GC carries a `gc-pause` emergency stop: the worker consults the `DegradeProbe` (contract: `crates/corelink-gc/src/degrade.rs:114-122`) at every phase transition — `transition_or_abort` probes BEFORE advancing the phase (`crates/corelink-gc/src/worker.rs:213`); only `GcPause` forces an abort at the next batch boundary (`requires_abort`, `crates/corelink-gc/src/degrade.rs:57-60`).
- `corelink-eviction` is soft-delete-first and reachability-gated: the executed reachable probe protects any blob a live AC entry references, using the strict `<` evict-arm / `>=` protect-arm boundary mirroring the GC TLA semantics (`crates/corelink-eviction/src/reachable.rs:177-200`); reclamation is the soft-delete `UPDATE … SET deleted_at` (NEVER a direct R2 DELETE), idempotent on `deleted_at IS NULL` (`crates/corelink-eviction/src/blob_meta.rs:336-353`).
- `corelink-ratelimit` is a lazy-refill token bucket keyed by a tenant-leftmost composite (`per_tenant` / `per_ip` / `per_tenant_per_endpoint`) per DO singleton, emitting RFC 6585 Retry-After seconds clamped to a floor/ceiling — the executed bucket math (`crates/corelink-ratelimit/src/bucket.rs:251-338`), the key dimensions (`crates/corelink-ratelimit/src/key.rs:30-49`). The live-bucket `HashMap` is **LRU-bounded** (`LIMITER_BUCKET_MAP_CAP`): a new key past the cap evicts the least-recently-accessed of a bounded sample (Redis-style approximate LRU, `O(1)` amortised) BEFORE materialising — closing the #534 OCI distinct-repo cross-tenant DoS where unbounded distinct keys grew the singleton's heap until the OOM-killer reaped it (`crates/corelink-ratelimit/src/limiter.rs:581` guard, eviction `evict_if_at_cap`).

# Invariants

- GC reclamation is pausable: the degrade probe is honored at every state transition, bounding blast radius of a bad sweep — consulted at the phase boundary in `transition_or_abort` (`crates/corelink-gc/src/worker.rs:213`).
- `INV-EVICT-SOFT-DELETE-FIRST`: eviction soft-deletes via `deleted_at`, never a direct R2 DELETE; physical cleanup is GC's job post-grace (`crates/corelink-eviction/src/blob_meta.rs:336-353`).
- The reachable-check is race-aware: `a.created_at < evict_started_at_ms` strict-evict with a `>=` protect mirror, so a blob written concurrently with the sweep is protected (`crates/corelink-eviction/src/reachable.rs:185-193`).
- `INV-AVAIL-ISOLATION` / `INV-TENANT-ISOLATION`: rate-limit buckets are per-tenant with a tenant-leftmost PK and an executed tenant-mismatch guard, so cross-tenant throttling is impossible by design (`crates/corelink-ratelimit/src/limiter.rs:581-590`).

# Gotchas

- Eviction is BLOB-only scope — chunks are owned by GC (S-06), not the evictor; mixing the two ownerships was an explicit Lote 10.7bis correction.
- TTLs are per-tier (free=7d … enterprise=365d default, admin-capped 730d) and the multipart reservation TTL is size-proportional (`max(60s, bytes/1MB/s × 2x), capped 7d`) so a 160 GiB upload no longer trips its own quota mid-stream.
- These crates ship pure-logic skeletons + in-memory fakes whose semantics mirror the embedded SQL migrations byte-for-byte; production binds them to real Cron Durable Objects.

# Citations

0a. `crates/corelink-gc/src/lib.rs:83-97` — the `corelink-gc` crate `pub mod` map (run/degrade/scheduler/sweep/**sweep_runner**/worker; `sweep_runner` is the new dry-run-first GC sweep entrypoint that closes the DD "GC has no entrypoint" gap — a daily cron + `gc_sweep` bin that deletes NOTHING unless `GC_LIVE_DELETE` is explicitly enabled).
0b. `crates/corelink-eviction/src/lib.rs:171-181` — the `corelink-eviction` crate `pub mod` map (reachable/blob_meta/trigger/storage_state).
0c. `crates/corelink-ratelimit/src/lib.rs:169-176` — the `corelink-ratelimit` crate `pub mod` map (bucket/limiter/key/config).
1. `crates/corelink-gc/src/run.rs:102-119` — `GcPhase::can_transition_to`: the executed monotone GC phase state machine.
2. `crates/corelink-gc/src/degrade.rs:114-122` — `DegradeProbe::probe`: the `gc-pause` degrade-mode emergency-stop CONTRACT; consulted at every phase transition by the worker (`crates/corelink-gc/src/worker.rs:213`, `transition_or_abort`).
2b. `crates/corelink-gc/src/degrade.rs:57-60` — `DegradeKind::requires_abort`: only `GcPause` aborts a Running worker at the next batch boundary.
3. `crates/corelink-eviction/src/reachable.rs:177-200` — the executed reachable check (strict `<` evict / `>=` protect; race-aware).
4. `crates/corelink-eviction/src/blob_meta.rs:336-353` — `INV-EVICT-SOFT-DELETE-FIRST`: the executed soft-delete `deleted_at` write (never a direct R2 DELETE; idempotent).
5. `crates/corelink-ratelimit/src/limiter.rs:581-590` — `INV-AVAIL-ISOLATION` / `INV-TENANT-ISOLATION`: the executed tenant-mismatch guard.
5b. `crates/corelink-ratelimit/src/limiter.rs:401` — `evict_if_at_cap`: the LRU-bounded live-bucket map (#534 OCI distinct-repo DoS fix; cap = `LIMITER_BUCKET_MAP_CAP`, approximate-LRU sample eviction).
6. `crates/corelink-ratelimit/src/bucket.rs:251-338` — `try_acquire`: lazy-refill token bucket + RFC 6585 Retry-After clamped to a per-config floor + hard ceiling.
6b. `crates/corelink-ratelimit/src/key.rs:30-49` — the tenant-leftmost bucket-key dimensions (`per_tenant` / `per_ip` / `per_tenant_per_endpoint`).
