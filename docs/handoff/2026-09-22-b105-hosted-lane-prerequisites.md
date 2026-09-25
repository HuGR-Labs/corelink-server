# B-105 hosted paired-lane prerequisites

Status: **blocked on owner inputs; B-105 remains open**.

This note records the result of the #1661 feasibility check against the
canonical B-105 contract in
[`evidence/owner-actions/B-105/cache-cost-contract.json`](../../evidence/owner-actions/B-105/cache-cost-contract.json).
It does not authorize a dispatch, a production write, or a deployment.

## Decision

A GitHub-hosted paired build can measure elapsed time and sccache hit/miss/error
counters after the owner supplies a production dogfood credential. The current
repository cannot produce the requested **truthful cost, transfer, and storage**
evidence from a credentialless lane:

| Required signal | Current state | Why this blocks a claim |
| --- | --- | --- |
| Same lane on a GitHub-hosted runner | `perf-production-evidence.yml` runs on `ubuntu-24.04`. | This repository prerequisite is satisfied; it does not prove a production measurement ran. |
| Cache-on arm | `scripts/collect_b105_same_lane.py` requires `CORELINK_PERF_BASE`, `CORELINK_PERF_TENANT`, and `CORELINK_PERF_PAT`. | No staging endpoint or credentialless WebDAV surface is present. A public production endpoint without an owner credential is not a measurement. |
| Isolated cache state | The collector's isolated mode uses a run-scoped namespace and exact-key cleanup; the production workflow must invoke that mode. | Until the protected workflow runs with owner-provided inputs, no isolated production receipt exists. |
| Cache transfer bytes | Isolated mode records successful GET payload bytes, PUT request bytes, retained indexed payload bytes, and exact-key cleanup. | These are client-boundary observations; a server/provider receipt is still needed to establish backend transfer, retention, and cleanup quantities. |
| Storage and compute cost | The repository has no provider billing or server-side byte receipt bound to this experiment. | Duration is not a currency cost, and client-side hit counts cannot establish R2/storage cost. |

The production workflow already has a protected manual dispatch and the hosted
isolated collector has credentialless behavior proof. Its B-105 call must use
isolated mode before any production run can produce an attributable receipt.
These repository changes do not satisfy the paired performance or provider-cost
gate; the contract remains `open_external_measurement_required`.

## Owner prerequisites

Before a dedicated hosted lane can be added or dispatched, the owner must
provide all of the following:

1. A dedicated disposable dogfood tenant (or a server-supported namespace
   prefix) and a low-privilege PAT with only the sccache read/write scope. The
   tenant or namespace must be exclusive to one run and must have a documented
   delete/GC operation after the receipt is captured.
2. The exact production origin and deployed revision serving `/cargo`. The
   origin must be supplied through a protected environment variable; it must
   never be hard-coded into a receipt alongside credentials.
3. A server or provider receipt that reports, for the isolated namespace,
   bytes read, bytes written, retained bytes, and cleanup result. If these
   values cannot be emitted by the production path, B-105 can report timing and
   counters only and cannot claim transfer/storage cost.
4. The billing basis for compute, egress, and storage, or an explicit decision
   to report physical quantities only. A GitHub Actions wall-clock duration
   alone does not prove a monetary cost.
5. Owner approval to run six or more pairs on `ubuntu-24.04`. The run must be
   manual, protected-main only, single-flight, and bounded by a job timeout.

## Required hosted-lane protocol

Once those prerequisites exist, the lane should be implemented as a separate
manual workflow rather than extending the broad production evidence job:

1. Checkout the exact dispatched commit and assert `git rev-parse HEAD` equals
   the recorded revision. Assert the workspace Rust toolchain, target triple,
   and the exact command `cargo test --package corelink-reapi --release
   --no-run`; set `CARGO_INCREMENTAL=0`.
2. Use `ubuntu-24.04`, a SHA-pinned sccache installation, `contents: read`, a
   protected `production` environment, and a single concurrency group. Keep
   every build directory under `RUNNER_TEMP`; do not use a shared Actions cache
   or a repository cache key.
3. Generate one run-scoped cache namespace from the run ID and revision. Seed
   that namespace once and record seed uploads separately. Every measured pair
   must use a fresh target directory and the same source, toolchain, target,
   command, and input set. Alternate control-first and treatment-first order
   across at least six pairs.
4. Run the control with the remote wrapper disabled and the treatment with the
   wrapper enabled. Record each arm's wall time, sccache hits, misses, read
   errors, write errors, revision, runner, toolchain, target, and namespace.
   Do not turn a failed cache request into a successful cache result.
5. Join the pair receipt with the server/provider byte receipt. Keep seed
   writes separate from measured treatment reads, and report cache misses and
   writes independently of the hit benefit. A zero-miss treatment has zero
   measured write-path contribution; no write timing may be used to explain it.
6. Emit only a redacted receipt. In an unconditional cleanup step, delete the
   run namespace and record deletion success or failure. Upload the receipt
   with bounded retention and leave B-105 open until the owner reviews the
   result against runner variance.

The existing credentialless contract gate remains the only repository-side
check. It may prove the path and arithmetic, but it cannot substitute for the
owner-supplied paired production receipt.
