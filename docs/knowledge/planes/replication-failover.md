---
type: "CrateCluster"
title: "Multi-region replication + failover plane"
description: "STATUS: designed-not-wired pure-logic skeleton. The coordinator (region promotion / split-brain reject / 24h failback), replica-worker (hot-blob R2 fan-out + hash verify + residency), and failover-router (multi-signal read re-route + write block) all ship in-memory orchestrators with full invariant tests — but NO live request path (reapi / adapter-host / corelink-worker) calls them; consumers only import the Region/TenantCtx type-alias façade."
source_files:
  - "crates/corelink-replication/src/lib.rs"
  - "crates/corelink-replication/src/coordinator.rs"
  - "crates/corelink-replication-coordinator/src/lib.rs"
  - "crates/corelink-replication-coordinator/src/coordinator.rs"
  - "crates/corelink-replication-coordinator/src/state.rs"
  - "crates/corelink-replication-coordinator/src/lag.rs"
  - "crates/corelink-replication-coordinator/src/heartbeat.rs"
  - "crates/corelink-replica-worker/src/lib.rs"
  - "crates/corelink-replica-worker/src/replication.rs"
  - "crates/corelink-replica-worker/src/region.rs"
  - "crates/corelink-failover-router/src/lib.rs"
  - "crates/corelink-failover-router/src/router.rs"
  - "crates/corelink-failover-router/src/health.rs"
  - "crates/corelink-failover-router/src/failback.rs"
  - "crates/corelink-reapi/src/read.rs"
  - "tests/e2e-replication-failover/Cargo.toml"
checkpoint_sha: "304547f6b48b067aa668068036f1ec2d7cf7e6b2"
provenance: "AUTHORED"
tags: ["replication", "failover", "multi-region", "availability"]
timestamp: "2026-06-28T00:00:00Z"
---

# Multi-region replication + failover plane

CoreLink's multi-region story is carried by four crates: `corelink-replication-coordinator` (region
role state machine — which region is `Primary`, promotion on outage, split-brain rejection, 24h
failback cool-down), `corelink-replica-worker` (the hot-blob fan-out lane — copy a blob from its
primary region's R2 to its allowed sibling region, hash-verify post-copy, retry, residency-gate),
`corelink-failover-router` (read-side middleware — detect a degraded region from a multi-signal probe
and re-route reads to the sibling while blocking writes), and `corelink-replication` (a thin Wave-33
aggregator façade that re-exports all of the above plus the staged-rollout controller at canonical
submodule paths). Together they encode the resilience design: at most one `Primary` per scope, no
cross-jurisdiction replication (WNAM↔ENAM, WEUR↔SAM only), audit-emit-before-mutation everywhere.

STATUS — designed-not-wired. **Every piece here is the pure-logic skeleton the production Cloudflare
wiring "will satisfy", not a live plane.** Each crate's own module doc says so verbatim ("ships the
**pure-logic skeleton** … the production CF Workers cron-trigger / Tower layer / DO singleton-lock
will satisfy"). All the orchestrators are `InMemory*` (per-instance `Arc<Mutex<>>`, simulated R2
`HashMap`), the "singleton DO promotion lock" is *modelled* by a `std::sync::Mutex`, and no live
request path consumes any of the promote / route_read / replicate_batch logic. The only thing the live
request path (`corelink-reapi`, `corelink-adapter-host`) imports from this cluster is the
`region_resolver` type-alias façade — `Region` + `TenantCtx`, which are themselves re-exports of
`corelink_worker` types — NOT the replication/failover behaviour. `corelink-worker` (the deployed CF
Worker) does not depend on any of the four crates at all. Treat this plane as a tested-but-dormant
design artifact: the invariants are proven against in-memory fixtures + a deterministic e2e harness,
but a region outage in production does not currently flow through this code.

# Role
- **Coordinator** — owns per-region `RegionRole` (`Primary` / `HotStandby` / `Replica`) and the
  failover decision tree: `evaluate` (keep / promote / no-eligible-replica), `promote` (demote primary
  → hot-standby, promote replica, under a singleton lock with split-brain reject), `failback` (re-promote
  a hot-standby only after the 24h cool-down), `replication_status` (the `/health`-shaped snapshot)
  (`crates/corelink-replication-coordinator/src/coordinator.rs:103-163`).
- **Replica-worker** — the per-blob apply lane: residency-gate, copy primary→sibling R2, hash-verify,
  retry up to 5, emit audits, mark `Replicated`
  (`crates/corelink-replica-worker/src/replication.rs:153-287`).
- **Failover-router** — read-side routing decision: healthy → read primary; degraded → read the sibling
  + block writes (`crates/corelink-failover-router/src/router.rs:158-216`).
- **Aggregator façade** — `corelink-replication` re-exports all four (plus rollout-controller) at
  canonical `coordinator` / `replica` / `failover` / `region` submodule paths; the live request path
  only touches its `region_resolver` type-alias module
  (`crates/corelink-replication/src/lib.rs:86-99`).

# How it works
1. **Write fan-out (designed).** A blob deemed "hot" by the offline aggregator becomes a batch the
   replica-worker processes. Per blob it FIRST residency-checks (`replica_region` must be the static
   acyclic sibling of `primary_region`), then emits a `replication.started` audit BEFORE any copy
   (fail-CLOSED), simulates an R2 GET(primary)→PUT(sibling) — keying the simulated store by
   `(tenant_id, region, blob_hash)` so two tenants' identical blobs never collide — and verifies
   `actual_hash == expected_hash`
   (`crates/corelink-replica-worker/src/replication.rs:153-191`).
2. **Hash-verify + retry.** A mismatch retries up to `MAX_REPLICATION_RETRIES` (5); persistent failure
   emits `replication.failed` and returns `RetryExhausted`
   (`crates/corelink-replica-worker/src/replication.rs:193-286`).
3. **Coordinator role decision.** `evaluate(primary, now)` returns `KeepPrimary` when the primary's
   heartbeat is fresh AND lag is within SLO; otherwise it scans replicas in deterministic `Region::ALL`
   order for one that is fresh+within-SLO (`PromoteReplica`), else `NoEligibleReplica` (escalate to
   runbook — never auto-promote into a partition)
   (`crates/corelink-replication-coordinator/src/coordinator.rs:314-355`).
4. **Promotion under the singleton lock.** `promote` takes the state `Mutex` for the whole sequence
   (the modelled singleton lock), rejects with `SplitBrainRejected` if any OTHER region already holds
   `Primary`, rejects with `PrimaryStillEligible` if the old primary is actually healthy (anti-flap),
   emits the demoted+promoted audits BEFORE flipping roles, then sets old→`HotStandby` (cool-down clock
   started) and new→`Primary` (`crates/corelink-replication-coordinator/src/coordinator.rs:357-466`).
5. **Failback after 24h.** A `HotStandby` region may only return to `Primary` once
   `now − cooldown_started ≥ HOT_STANDBY_COOLDOWN_SECONDS` (86 400). Premature attempts emit
   `failback_blocked` and return `CooldownNotElapsed`
   (`crates/corelink-replication-coordinator/src/coordinator.rs:468-519`).
6. **Read failover routing.** `route_read` probes the primary's health; `Healthy` → read primary, writes
   allowed; degraded → look up the sibling, emit `failover.detected` BEFORE routing, return
   `read_region = sibling`, `read_mode = Replica`, `write_mode = Blocked`
   (`crates/corelink-failover-router/src/router.rs:158-216`).
7. **Multi-signal degradation.** A region is flagged `Degraded` only when ALL THREE signals fire (5xx
   rate > 1%, p99 > 300ms, ≥3 consecutive failures) — the AND of three triggers, to suppress transient
   false positives (`crates/corelink-failover-router/src/health.rs:222-256`).
8. **Failback audit-outbox gate.** Re-engaging writes on a recovered old-primary is additionally gated by
   `assert_outbox_drained_or_block`: it refuses (and emits `corelink_failback_blocked_total`) if the old
   primary's `audit_outbox` still has un-emitted rows, fail-CLOSED on a query error
   (`crates/corelink-failover-router/src/failback.rs:255-285`).
9. **What's actually wired.** The deterministic e2e harness drives the `InMemory*` orchestrators under a
   logical clock — it is a skeleton-validation suite, not a live-path test
   (`tests/e2e-replication-failover/Cargo.toml:2`). In `corelink-reapi`, the only import from this
   cluster is the `Region` type-alias via the façade, never a coordinator/router call
   (`crates/corelink-reapi/src/read.rs:76`).

# Invariants
- **INV-FAILOVER-NO-SPLIT-BRAIN** — at most one `Primary` at any instant: `promote` rejects when another
  region already holds `Primary`, and `register` enforces the same at registration time
  (`crates/corelink-replication-coordinator/src/coordinator.rs:386-403`).
- **Audit-emit-BEFORE-mutation, fail-CLOSED** — role flips, replications, and route decisions all emit
  their audit record before mutating state; an audit-sink error aborts with `Audit(..)` and leaves state
  untouched (`crates/corelink-replication-coordinator/src/coordinator.rs:417-465`;
  `crates/corelink-replica-worker/src/replication.rs:163-174`).
- **INV-REGION-NO-CROSS-LEAK (residency)** — replication is allowed ONLY to the static acyclic sibling
  (WNAM↔ENAM, WEUR↔SAM); any other target is a `ResidencyViolation`
  (`crates/corelink-replica-worker/src/region.rs:254-265`).
- **INV-CAS-INTEGRITY (hash verify post-copy)** — a replicated blob is only marked done when the copied
  bytes re-hash to the expected value; else retry, then `RetryExhausted`
  (`crates/corelink-replica-worker/src/replication.rs:210-286`).
- **24h hot-standby cool-down** — a demoted primary cannot fail back for `HOT_STANDBY_COOLDOWN_SECONDS`
  = 86 400 (`crates/corelink-replication-coordinator/src/state.rs:27`).
- **Anti-flap** — `promote` refuses if the old primary is still fresh+within-SLO
  (`crates/corelink-replication-coordinator/src/coordinator.rs:405-409`).

# Gotchas
- **Designed, not wired — the whole plane.** Nothing in the live request path
  (`corelink-worker`/`corelink-reapi`/`corelink-adapter-host`) calls `promote`, `route_read`, or
  `replicate_batch`. A production region outage does NOT flow through this code today; the `InMemory*`
  orchestrators + the e2e harness are the only callers. Do not claim multi-region failover is "live."
- **The "singleton DO lock" is a `std::sync::Mutex`.** Split-brain safety is proven only for concurrent
  callers against ONE in-process coordinator instance; the real cross-isolate Durable Object singleton
  is deferred (`crates/corelink-replication-coordinator/src/coordinator.rs:170-173`).
- **R2 GET/PUT is a `HashMap` keyed by `(tenant_id, region, blob_hash)`, hashes are FNV not SHA/ETag.**
  The replica-worker simulates the object store as an `Arc<Mutex<HashMap<(String,String,String),Vec<u8>>>>`
  — the leading `tenant_id` component is load-bearing even in simulation, so two tenants storing the same
  `blob_hash` in the same region do NOT collide (in production the leading key segment is the derived
  `tenant_prefix`, from that same `tenant_id`); it uses FNV-1a for both the blob hash and the
  `tenant_id_hash`, and production swaps in real R2 bindings + the R2 ETag
  (`crates/corelink-replica-worker/src/replication.rs:65-74`,
  `crates/corelink-replica-worker/src/replication.rs:393-407`).
- **`corelink-replication` is just a re-export façade.** Importing it does not pull in behaviour you can
  call on the live path — its `region_resolver` submodule is a type alias over `corelink_worker`, which
  is the only part anything live actually uses (`crates/corelink-replication/src/lib.rs:92-99`).
- **`route_read`'s probe-failure path fails CLOSED to degraded** (treats a probe error as an outage),
  which is conservative but means a flaky probe would route reads to the sibling if this were ever wired
  (`crates/corelink-failover-router/src/router.rs:150-155`).

# Citations
1. `crates/corelink-replication/src/lib.rs:86-99` — the aggregator re-exports (`coordinator`/`failover`/
   `region`/`replica`) + the `region_resolver` type-alias façade over `corelink_worker::{Region, TenantCtx}`.
2. `crates/corelink-replication/src/coordinator.rs:9` — `pub use corelink_replication_coordinator::*;`
   (the façade is a pure re-export, no logic of its own).
3. `crates/corelink-replication-coordinator/src/lib.rs:7-11` — module doc states this crate ships the
   "**pure-logic skeleton** … the production Cloudflare DO singleton-lock will satisfy."
4. `crates/corelink-replication-coordinator/src/coordinator.rs:103-163` — the `ReplicationCoordinator`
   trait surface (route_write / evaluate / promote / failback / replication_status).
5. `crates/corelink-replication-coordinator/src/coordinator.rs:170-173` — the per-instance `Mutex` IS
   the modelled singleton promotion lock ("production = DO singleton ID").
6. `crates/corelink-replication-coordinator/src/coordinator.rs:314-355` — `evaluate` decision tree
   (KeepPrimary / scan replicas for PromoteReplica / NoEligibleReplica).
7. `crates/corelink-replication-coordinator/src/coordinator.rs:357-409` — `promote` takes the singleton
   lock, rejects `SplitBrainRejected` / `PrimaryStillEligible` (anti-flap).
8. `crates/corelink-replication-coordinator/src/coordinator.rs:417-465` — audit-emit-BEFORE-mutation
   (demoted + promoted records emitted, then `guard.insert` flips roles).
9. `crates/corelink-replication-coordinator/src/coordinator.rs:468-519` — `failback` 24h cool-down gate
   (`failback_blocked` audit + `CooldownNotElapsed` when elapsed < required).
10. `crates/corelink-replication-coordinator/src/state.rs:27` — `HOT_STANDBY_COOLDOWN_SECONDS = 24*60*60`.
11. `crates/corelink-replication-coordinator/src/lag.rs:81` — `LagBundle::within_slo()` (all-domain SLO check).
12. `crates/corelink-replication-coordinator/src/heartbeat.rs:57` — `Heartbeat::is_fresh(now_ms)`.
13. `crates/corelink-replica-worker/src/lib.rs:5-11` — module doc: "**pure-logic skeleton** … the
    production CF Workers cron-trigger will satisfy".
14. `crates/corelink-replica-worker/src/replication.rs:153-174` — `replicate_one`: residency check
    FIRST, then `replication.started` audit BEFORE the copy.
15. `crates/corelink-replica-worker/src/replication.rs:193-286` — R2 PUT + post-copy hash verify, retry
    to `MAX_REPLICATION_RETRIES` (5), then `replication.failed` + `RetryExhausted`.
16. `crates/corelink-replica-worker/src/replication.rs:65-115` — the tenant-keyed simulated R2 store
    (`HashMap<(tenant_id, region, blob_hash), bytes>`) and the in-memory worker construction
    ("production = real R2 bindings, deferred").
17. `crates/corelink-replica-worker/src/region.rs:254-265` — `ResidencyGraph::is_allowed` (sibling-only;
    else `ResidencyViolationInfo`).
18. `crates/corelink-replica-worker/src/region.rs:229-237` — `ResidencyGraph::sibling` (WNAM↔ENAM,
    WEUR↔SAM).
19. `crates/corelink-failover-router/src/lib.rs:6-10` — module doc: "**pure-logic skeleton** … the
    production Tower layer will satisfy".
20. `crates/corelink-failover-router/src/router.rs:158-216` — `route_read`: healthy→primary;
    degraded→sibling read + `failover.detected` audit + `WriteMode::Blocked`.
21. `crates/corelink-failover-router/src/router.rs:150-155` — `get_health` fails CLOSED to degraded on a
    probe error.
22. `crates/corelink-failover-router/src/health.rs:222-245` — `evaluate`: `Degraded` only when all three
    triggers (5xx>1%, p99>300ms, ≥3 consecutive failures) fire.
23. `crates/corelink-failover-router/src/failback.rs:255-285` — `assert_outbox_drained_or_block`:
    refuses re-engagement (+`corelink_failback_blocked_total`) when the old-primary outbox is dirty,
    fail-CLOSED on a query error.
24. `crates/corelink-reapi/src/read.rs:76` — the live reapi read path imports ONLY
    `corelink_replication::region_resolver::TenantCtx` (the type-alias façade), never a
    coordinator/router call — evidence the plane is unconsumed.
25. `tests/e2e-replication-failover/Cargo.toml:2` — the e2e harness drives the `InMemory*` coordinator/
    heartbeat/audit orchestrators (skeleton validation), not a live request path.
