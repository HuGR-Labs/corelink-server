# B-103/B-104/B-105/B-107/B-109/B-122/B-129 owner packet

Group status: **open** (production evidence pending).

This packet records the repository-owned attribution contract at the exact D03
base used for this change. It does not contain production timings and it does
not authorize a release, deploy, tail, or closure. Every backlog item in this
group remains **open** until the owner records a fresh observation against the
deployed commit.

## What is now attributable locally

- `ostore` is the CAS/R2 window. `oaccounting` is the bounded D1 byte-accounting
  and adapter URL-map window. They are disjoint phases; a broad outer storage
  scope is not used to hide accounting time.
- The sync `spawn_blocking` bridge carries the request ledger through a scoped
  thread-local. Nested R2 scopes coalesce by depth, while accounting scopes are
  opened only around their D1 calls.
- `qdo` and `qcontrol` remain diagnostic-only behind
  `SERVER_TIMING_WDB_DETAIL=on`. `ohop` and `ohandler` refuse malformed or
  over-counted splits rather than inventing an attribution.
- The container emits canonical `ohandler` plus one identical `oother;desc="legacy-alias"`
  during rollout. New Workers normalize/deduplicate it; old Workers consume the
  alias, so either deployment order is safe. Remove the alias only after old
  allowlists have drained.
- Authenticated 404 padding is intentionally not emitted as a public phase:
  publishing the pad would weaken the enumeration defence. The owner may
  derive a residual from the wire `total` and named phases, but must retain the
  raw response identity and treat the residual as a measurement, not a cause.

## Owner-only measurements

Run only after the production version is recorded and the test tenant is
explicitly approved. Retain operation IDs, response request IDs, CF-Ray colo,
HTTP status, raw `Server-Timing`, and content-addressed output digests; never
write a PAT or response body into the packet.

1. **B-103 — serialization/429:** issue bounded authenticated PUT bursts at
   concurrency 4, 16, and 64 (same 1 KiB body and tenant). Keep every status,
   including 429, and capture an unfiltered server tail for the burst. A green
   result requires zero failures at every level and scaling throughput; do not
   infer a cause from the status alone.
2. **B-104 — authenticated 404 residue:** issue at least ten authenticated GETs
   for distinct missing keys. Report median and p90 from elapsed wall time and
   the wire phases. Do not close on a better maximum, and do not expose or
   change the timing pad to make the residual look smaller.
3. **B-105 — cache comparison:** run the exact same build lane six times in
   alternating cache-off/cache-on order on one owner runner. Record the lane
   command, revision, runner, sccache hit/miss/error counters, and complete
   durations. A cache hit-rate claim is not a lane-speed claim.
4. **B-107 — R2 versus D1:** run three sequential authenticated 1 KiB PUTs
   within 60 seconds after the instrumented container is deployed. Require both
   `ostore` and `oaccounting` in every response. Report p50/p90/p99 for each
   phase and their sum. A legacy response with only aggregate `ostore` is
   **not** evidence of separation and leaves B-107 open.
5. **B-109 — named container work:** read `opat`, `oquota`, `ostore`,
   `oaccounting`, `ortier`, `oaudit`, and `ohandler` from the same warm PUT/GET
   population. The checked-in `ohandler` phase is explicit framework work; during
   rollout, the identical `oother` alias keeps old Workers from charging it to
   `ohop`. New Workers normalize the alias, and its production magnitude still
   requires a wire measurement.
6. **B-122 — boundary remeasurement:** after deployment, rerun the B-102 and
   B-107 warm baselines with the deployed commit and retain both the Worker
   serving SHA/version and container build SHA/application version beside the
   numbers. The production deploy lane writes
   `corelink-source-sha=<GITHUB_SHA>` into the Worker deployment annotation.
   The protected B-122 lane reads the active Worker and container application
   back before sampling: one Worker version must serve 100% of traffic; the
   container image tag must resolve to a full build SHA with a successful image
   build, the B-122 container change must be in that source, and GitHub compare
   must prove the build SHA is an ancestor of the Worker serving SHA. It repeats
   both provider identity reads after sampling; only the redacted binding plus
   timing receipt is retained. Confirm the blocking-task phase appears and the
   phase sum does not exceed the request clock. Local tests cannot close this
   production remeasurement requirement.
7. **B-129 — attribution contract and production residue budget (open):** the
   repository-side `qcontrol` split and fail-closed guard are ready, but do not
   close the item. On an approved diagnostic window only, arm
   `SERVER_TIMING_WDB_DETAIL=on`, record the deployed commit, region, timestamp,
   sample and flag, and collect authenticated cache reads. Reconcile
   `qtier+qdo+qbatch+qresid+qcontrol` to `wdb` and
   `ohop+container phases` to `origin`; reject malformed, conflicting-alias or
   over-counted rows. Close only with measured residual **<10% of total** across
   the agreed population. The flag or repository guard alone never closes B-129.

The repository verifier checks these instructions and the source-level
invariants only. It never contacts production and therefore reports the group
as structurally ready but operationally open:

```sh
python3 scripts/verify_b103_b129_attribution.py
```
