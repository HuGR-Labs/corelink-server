---
type: "CrateCluster"
title: "Multi-region replication + failover plane"
description: "STATUS: SPLIT. The read-side failover-router IS live — `corelink-container` layers `failover::failover_guard` as a Tower middleware over the data plane, driving `corelink-failover-router`'s decision core with a REAL rolling-metrics health probe over the container's own traffic; on a detected region degradation it fail-CLOSED blocks writes with a 503 `failover_readonly` and stamps a sibling-read hint. The coordinator (region promotion / split-brain reject / 24h failback) and replica-worker (hot-blob R2 fan-out + hash verify + residency) remain designed-not-wired pure-logic skeletons: `corelink-container` has no dependency on either crate, so `promote` and `replicate_batch` never run against production traffic."
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
  - "crates/corelink-container/src/routes/build.rs"
  - "crates/corelink-container/src/routes/failover.rs"
  - "crates/corelink-container/Cargo.toml"
  - "tests/e2e-replication-failover/Cargo.toml"
source_blobs:
  - "crates/corelink-replication/src/lib.rs@6e1f3491cf6d52fc6b6d51324ce5c4c45601cc4d"
  - "crates/corelink-replication/src/coordinator.rs@d594c794c0aea88b0437cb64447ed29eeff8d27e"
  - "crates/corelink-replication-coordinator/src/lib.rs@912d83ab0826045aa07ceacc37ba1ebfe5d4d27a"
  - "crates/corelink-replication-coordinator/src/coordinator.rs@3968034d21fba878f8c006ff9c2145a9b709eef1"
  - "crates/corelink-replication-coordinator/src/state.rs@fbe52da081778756f4af0423204d2d005c193b96"
  - "crates/corelink-replication-coordinator/src/lag.rs@229d66666517683bb08398c418eba23857a10eeb"
  - "crates/corelink-replication-coordinator/src/heartbeat.rs@5d46ae71b328cf07e12fa06fbfe1565ea87edf5a"
  - "crates/corelink-replica-worker/src/lib.rs@d56ee1095eb03417fe635b69cf5555c0d63b6a7f"
  - "crates/corelink-replica-worker/src/replication.rs@cfd57d6323bd8b6bb7f096bf2d1d10c79946d65b"
  - "crates/corelink-replica-worker/src/region.rs@990a56680933c3d4cd48252730ef56547cc833ff"
  - "crates/corelink-failover-router/src/lib.rs@c3e6a6dda601d18d33b18ad6e1fd38beeb46747a"
  - "crates/corelink-failover-router/src/router.rs@bead0cac486f4feb8aacfbb96e8a6ea71778729e"
  - "crates/corelink-failover-router/src/health.rs@a6243b0447b896130f571115454706c413e22fa5"
  - "crates/corelink-failover-router/src/failback.rs@c7bef30880b77d7e9723cea72ec5dd93cdaac14b"
  - "crates/corelink-reapi/src/read.rs@7cb16637db6d2f1d3667fc45fe12fcd5bd943630"
  - "crates/corelink-container/Cargo.toml@e9feace6365776c6a4141ff59efa70fe1b36e158"
  - "tests/e2e-replication-failover/Cargo.toml@13ed9f963c625e3f06885e1aad711547d4ecee67"

  - "crates/corelink-container/src/routes/build.rs@43036d91a76bf2e14d6d49c2bca705037fc02081"
  - "crates/corelink-container/src/routes/failover.rs@1657370527936e90f2f7d51f3b5400cbd30055fe"
checkpoint_sha: "91630baebe3ae7abe686cd4e06a5621ecdc4ab73"
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

STATUS — SPLIT, and this is load-bearing for anyone diagnosing a production 503. The **read-side
failover-router is wired and live**: `crates/corelink-container/src/routes/build.rs:641-644` layers
`failover::failover_guard` (`crates/corelink-container/src/routes/failover.rs:635`) as a Tower
middleware over the data-plane router, alongside `residency_guard`. That middleware drives
`corelink-failover-router`'s decision core with a REAL `RollingMetricsHealthProbe` built from THIS
container's own live traffic (5xx rate, p99 latency, consecutive failures) — not the `InMemory*` test
fixture — gated by TWO anti-false-positive layers before a write can be blocked: (1) a statistical
 sample floor (`FAILOVER_MIN_SAMPLES`, default 50 — below it a clean/empty window is healthy, but
 a newly observed 5xx event makes health indeterminate once; a following clean event cannot
 re-count that same event) and (2) trip/recover hysteresis (`HysteresisGate`: 3
consecutive DEGRADED probe observations to latch, 5 consecutive HEALTHY to release). Only then does
it call `state.router.route_read(...)`
(`crates/corelink-container/src/routes/failover.rs:670`). **If the region is degraded, writes
(`POST`/`PUT`/`PATCH`/`DELETE`) are fail-CLOSED rejected with a 503 body `failover_readonly`**, and
reads are passed through with a `x-corelink-failover-read-region` / `x-corelink-failover-active: 1`
hint for the edge Worker to re-route. **A 503 with that body in production means this live code path
tripped — it is not a hypothetical failure mode, and it is not explained by the "designed, not wired"
disclaimer below.**

An authenticated internal `POST /_internal/failover/heartbeat` endpoint refreshes an external
heartbeat wired into `FailoverLayerState`; the public `/_health` readiness endpoint cannot refresh
it. If that heartbeat is stale, the same hysteresis gate fails closed even when no data-plane
traffic has arrived. Data-plane responses and anonymous requests never refresh that signal.

The **coordinator** (`corelink-replication-coordinator`, `promote`/`failback`) and the
**replica-worker** (`corelink-replica-worker`, `replicate_batch`) remain the pure-logic skeletons
their own module docs describe ("ships the **pure-logic skeleton** … the production CF Workers
cron-trigger / DO singleton-lock will satisfy"). `crates/corelink-container/Cargo.toml` has no
dependency on `corelink-replication-coordinator` or `corelink-replica-worker` (only on
`corelink-failover-router`, verified against `crates/corelink-container/Cargo.toml:312`) — so no live request path ever calls
`promote` or `replicate_batch`. Region promotion, split-brain rejection, and hot-blob R2 fan-out are
still `InMemory*` orchestrators exercised only by the deterministic e2e harness; a coordinator-level
region promotion in production does NOT flow through this code today. Do not conflate the two halves:
the router's read-side decision + write-block is live production behaviour; promotion/replication is
not.

# Role
- **Coordinator** — owns per-region `RegionRole` (`Primary` / `HotStandby` / `Replica`) and the
  failover decision tree: `evaluate` (keep / promote / no-eligible-replica), `promote` (demote primary
  → hot-standby, promote replica, under a singleton lock with split-brain reject), `failback` (re-promote
  a hot-standby only after the 24h cool-down), `replication_status` (the `/health`-shaped snapshot)
  (`crates/corelink-replication-coordinator/src/coordinator.rs:103-163`).
- **Replica-worker** — the per-blob apply lane: residency-gate, copy primary→sibling R2, hash-verify,
  retry up to 5, emit audits, mark `Replicated`
  (`crates/corelink-replica-worker/src/replication.rs:158-291`).
- **Failover-router** — read-side routing decision: healthy → read primary; degraded → read the sibling
  + block writes (`crates/corelink-failover-router/src/router.rs:166-238`).
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
   (`crates/corelink-replica-worker/src/replication.rs:158-195`).
2. **Hash-verify + retry.** A mismatch retries up to `MAX_REPLICATION_RETRIES` (5); persistent failure
   emits `replication.failed` and returns `RetryExhausted`
   (`crates/corelink-replica-worker/src/replication.rs:197-290`).
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
   (`crates/corelink-failover-router/src/router.rs:166-238`).
7. **Multi-signal degradation.** A region is flagged `Degraded` only when ALL THREE signals fire (5xx
   rate > 1%, p99 > 300ms, ≥3 consecutive failures) — the AND of three triggers, to suppress transient
   false positives (`crates/corelink-failover-router/src/health.rs:222-256`).
8. **Failback audit-outbox gate.** Re-engaging writes on a recovered old-primary is additionally gated by
   `assert_outbox_drained_or_block`: it refuses (and emits `corelink_failback_blocked_total`) if the old
   primary's `audit_outbox` still has un-emitted rows, fail-CLOSED on a query error
   (`crates/corelink-failover-router/src/failback.rs:255-285`).
9. **What's actually wired.** `corelink-container`'s `failover_guard` Tower middleware
   (`crates/corelink-container/src/routes/build.rs:641-644`) drives `corelink-failover-router`'s
   `route_read` against a REAL health probe on every request through the data-plane router — this
   part IS live production code. The coordinator's `promote`/`failback` and the replica-worker's
   `replicate_batch` have no such caller: the deterministic e2e harness drives those `InMemory*`
   orchestrators under a logical clock, which is a skeleton-validation suite, not a live-path test
   (`tests/e2e-replication-failover/Cargo.toml:2`). In `corelink-reapi`, the only import from this
   cluster is the `Region` type-alias via the façade, never a coordinator/router call
   (`crates/corelink-reapi/src/read.rs:76`) — `corelink-reapi` is a separate consumer from
   `corelink-container`, and it is `corelink-container` that wires the router.

# Invariants
- **INV-FAILOVER-NO-SPLIT-BRAIN** — at most one `Primary` at any instant: `promote` rejects when another
  region already holds `Primary`, and `register` enforces the same at registration time
  (`crates/corelink-replication-coordinator/src/coordinator.rs:386-403`).
- **Audit-emit-BEFORE-mutation, fail-CLOSED** — role flips, replications, and route decisions all emit
  their audit record before mutating state; an audit-sink error aborts with `Audit(..)` and leaves state
  untouched (`crates/corelink-replication-coordinator/src/coordinator.rs:417-465`;
  `crates/corelink-replica-worker/src/replication.rs:168-178`).
- **INV-REGION-NO-CROSS-LEAK (residency)** — replication is allowed ONLY to the static acyclic sibling
  (WNAM↔ENAM, WEUR↔SAM); any other target is a `ResidencyViolation`
  (`crates/corelink-replica-worker/src/region.rs:254-265`).
- **INV-CAS-INTEGRITY (hash verify post-copy)** — a replicated blob is only marked done when the copied
  bytes re-hash to the expected value; else retry, then `RetryExhausted`
  (`crates/corelink-replica-worker/src/replication.rs:215-290`).
- **24h hot-standby cool-down** — a demoted primary cannot fail back for `HOT_STANDBY_COOLDOWN_SECONDS`
  = 86 400 (`crates/corelink-replication-coordinator/src/state.rs:27`).
- **Anti-flap** — `promote` refuses if the old primary is still fresh+within-SLO
  (`crates/corelink-replication-coordinator/src/coordinator.rs:405-409`).

# Gotchas
- **⚠️ The read-side router IS live — a 503 `failover_readonly` in production is real.**
  `corelink-container` layers `failover::failover_guard` as Tower middleware
  (`crates/corelink-container/src/routes/build.rs:641-644`), and that middleware calls
  `state.router.route_read(...)` (`crates/corelink-container/src/routes/failover.rs:670`) against a
  REAL rolling-metrics health probe on every request. If you are diagnosing a production 503 with body
  `failover_readonly`, this is the code that produced it — treat it as a genuine region-degradation
  signal, not a test artifact, and check `RollingMetricsHealthProbe`'s inputs (5xx rate / p99 / consecutive
  failures) for the primary region.
- **Designed, not wired — only `promote` and `replicate_batch`.** `crates/corelink-container/Cargo.toml`
  depends on `corelink-failover-router` but NOT on `corelink-replication-coordinator` or
  `corelink-replica-worker` (verified: `grep -n "corelink-replication\|corelink-replica-worker" crates/corelink-container/Cargo.toml`
  matches only `corelink-failover-router`). Nothing in the live request path calls `promote` (region
  promotion) or `replicate_batch` (hot-blob R2 fan-out). A production region promotion or replication
  event does NOT flow through this code today; the `InMemory*` orchestrators + the e2e harness are the
  only callers. Do not claim automatic promotion or cross-region replication is "live" — only the
  read-reroute-hint + write-block decision is.
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
  `crates/corelink-replica-worker/src/replication.rs:397-417`).
- **`corelink-replication` is just a re-export façade.** Importing it does not pull in behaviour you can
  call on the live path — its `region_resolver` submodule is a type alias over `corelink_worker`, which
  is the only part anything live actually uses (`crates/corelink-replication/src/lib.rs:92-99`).
- **`route_read`'s probe-failure path fails CLOSED to degraded** (treats a probe error as an outage),
  which is conservative but means a flaky probe would route reads to the sibling if this were ever wired
  (`crates/corelink-failover-router/src/router.rs:166-172`).

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
14. `crates/corelink-replica-worker/src/replication.rs:158-178` — `replicate_one`: residency check
    FIRST, then `replication.started` audit BEFORE the copy.
15. `crates/corelink-replica-worker/src/replication.rs:197-290` — R2 PUT + post-copy hash verify, retry
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
20. `crates/corelink-failover-router/src/router.rs:166-238` — `route_read`: healthy→primary;
    degraded→sibling read + `failover.detected` audit + `WriteMode::Blocked`.
21. `crates/corelink-failover-router/src/router.rs:166-172` — `get_health` fails CLOSED to degraded on a
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
26. `crates/corelink-container/src/routes/build.rs:641-644` — `failover::FailoverLayerState::from_env()` +
    `router.layer(axum::middleware::from_fn_with_state(failover_state, failover::failover_guard))`,
    layered alongside `residency_guard` — the live Tower-middleware wiring.
27. `crates/corelink-container/src/routes/failover.rs:454-505` — `RollingMetricsHealthProbe::region_health`: a too-small sample window is healthy when clean, but one newly observed 5xx makes health indeterminate once; the probe then computes the 5xx rate, consecutive failures, and p99 for a full window.
28. `crates/corelink-container/Cargo.toml:312` — the crate depends on `corelink-failover-router` only;
    no dependency on `corelink-replication-coordinator` or `corelink-replica-worker`, confirming
    `promote`/`replicate_batch` are still unreachable from the live request path.
29. `crates/corelink-container/src/routes/failover.rs:635-695` — `failover_guard`: applies heartbeat, health, and hysteresis signals; blocks writes with 503 `failover_readonly` and stamps the sibling read-region hint for reads during degradation.


# Revalidation

This concept was revalidated against the cumulative implementation tree; its existing source citations remain the controlling evidence for the behavior described above.
28. `crates/corelink-container/src/routes/build.rs:1` — declared source anchor.
