# `corelink-server` current-main source readback — `96fd677` (2026-09-22)

Direct object inspection of `origin/main@96fd67793704c597c697d24b0baf96731da102ef`.
No checkout movement, Rust build, test, boot, D1/R2 call, deployment or runtime
operation was performed.

## Current drift from the historical pin

The server source remains 399 Rust files under `crates/corelink-container/src`
with 14 test Rust files counted separately. The current main tree adds DSR
classification for migration 0139/0140:

| Migration | Table/column | SQL scope | Current source disposition |
|---|---|---|---|
| 0139 `runner_entitlement_reconcile_fence` | `runner_entitlement_reconcile_fence` | `tenant_id` primary key | `ALL_TENANT_KEYED_TABLES` erase-set |
| 0140 `runner_entitlement_authority` | `authority_is_current` on 0139 | fence column | remains in the 0139 erase disposition |

The current `adapter_d1.rs` and `adapter_d1_registry.rs` readbacks explicitly
include `runner_entitlement_reconcile_fence`; the corresponding test checks its
tenant-erasure registration. This supersedes the historical 0133–0138-only
classification in `SERVER-CURRENT-MAIN-READBACK-20260922.md`.

## Boundary

The readback establishes source facts only. It does not prove Cargo resolution,
selected targets/features, route mounts, D1 execution, runtime reachability,
deployment or migration application state. The server artifacts remain draft
and require a fresh independent cold review after this reanchor.
