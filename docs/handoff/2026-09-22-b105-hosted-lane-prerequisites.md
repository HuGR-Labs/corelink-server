# B-105 hosted paired-lane protocol

Status: **implemented for one protected-main dispatch; B-105 remains open until
the receipt is reviewed.**

This protocol belongs to issue #1661 and the canonical contract in
[`evidence/owner-actions/B-105/cache-cost-contract.json`](../../evidence/owner-actions/B-105/cache-cost-contract.json).
The source workflow is
[`issue-1661-b105-hosted.yml`](../../.github/workflows/issue-1661-b105-hosted.yml).

## Inputs and scope

- The existing `CORELINK_SCCACHE_TOKEN` is documented as `cas:rw` on the
  dogfood tenant `ee30f7ba-fc25-4d71-939e-ebe130b4c6a3`.
- `CORELINK_PERF_BASE` is stored as a non-secret variable on the protected
  `production` environment and points to `https://corelink-api.humangr.com`.
- The job runs only for a manual dispatch on the protected `main` branch of the
  canonical repository. A confirmation input, single-flight concurrency group,
  240-minute timeout, and six-pair bound limit each run.
- The collector forwards WebDAV calls through a local meter. It prefixes every
  key with a fresh run ID plus random nonce and records only the exact keys it
  touched. It reports successful GET response payload bytes, successful PUT
  request payload bytes, and the sum of payload sizes successfully written for
  unique indexed keys. These are application-layer quantities, not R2 physical
  retention, TLS wire bytes, or a provider invoice.
- Collection-root `PROPFIND` and `MKCOL` calls receive a synthetic local
  response; the shared tenant root is never forwarded or enumerated.
- The run seeds the unique namespace before timing. It removes the Cargo target
  directory before every arm and alternates order across six cache-off/cache-on
  pairs. Both arms use the same pinned Rust toolchain, target, command, runner,
  and source revision; cache I/O errors fail the measurement.
- Results are `faster`, `slower`, or `indeterminate`. The classification uses
  the six paired deltas and their two-sided 95% Student-t interval; a faster
  result requires the entire interval below zero.
- A finally path deletes every exact key recorded for the run and verifies each
  key returns 404. A workflow fallback repeats that idempotent cleanup after an
  interrupted collector. The artifact contains no bearer credential or cache
  object body and is retained for seven days.

## Cost reporting decision

This campaign reports application payload and indexed logical bytes only.
It does not claim a dollar amount: the measurement has no billing rate for the
GitHub runner, network transfer, or R2 retention. Seed writes, measured pair
traffic, retained indexed payload bytes, and cleanup are separate receipt fields. A
zero-miss treatment therefore has zero measured artifact writes, regardless of
seed traffic.

## Frozen success and completion gate

- **Success criteria:** exactly six alternating pairs on one `ubuntu-24.04`
  runner and exact main SHA; every arm completes the same Cargo command; cache
  I/O errors are zero; each treatment has at least one remote hit; receipt
  reports paired durations, hits/misses, application payload bytes, indexed
  payload bytes, and cleanup; a performance win is accepted only when the paired 95%
  interval is fully faster.
- **Definition of done:** the focused PR passes required hosted checks and DCO;
  the exact-main production dispatch completes; its redacted artifact is linked
  from #1661; `BACKLOG.md` and issue state reflect the measured result; all run
  keys are verified absent.
- **Completeness:** the receipt binds run ID, full source SHA, runner, toolchain,
  target, fixed command, treatment order, all six pairs, byte counters, indexed
  payload bytes, and cleanup outcome. Any failed arm or incomplete cleanup is
  not a valid measurement.
- **Invariants:** no local builds/tests/lint; no shared Cargo or Actions build
  cache; no fail-open cache I/O; no other tenant or namespace; no credentials,
  raw cache values, or raw Cargo logs in receipts; no claim that payload bytes
  equal wire or billed bytes; slower and inconclusive outcomes remain visible.
- **Quality standard:** exact-main guard, minimal `contents: read`, pinned
  Actions and sccache version, bounded run time and retention, fail-closed input
  validation, redacted output, and exact-key cleanup with verification.

The existing credentialless contract check remains separate. It verifies the
repository rules but cannot substitute for the main-only live artifact.
