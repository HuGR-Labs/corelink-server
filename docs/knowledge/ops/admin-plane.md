---
type: "Runbook"
title: "The admin plane: config-singleton CAS + dual-approval mutate + pilot lifecycle"
description: "How operators mutate runtime config with optimistic concurrency and dual approval, and how the container admin routes gate every privileged action."
source_files:
  - "crates/corelink-container/src/routes/admin.rs"
  - "crates/corelink-container/src/routes/admin_pilot.rs"
  - "docs/internal/admin-plane.md"
checkpoint_sha: "a7c16588ead34ed6095e0aa9db67ddf77ac96688"
provenance: "AUTHORED"
tags: ["ops", "admin", "config", "dual-approval", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# The admin plane: config-singleton CAS + dual-approval mutate + pilot lifecycle

The admin plane is the operator control surface. The **LIVE** container admin mutate route gates
privileged tenant-scoped actions — granting a tenant tier (`set_tenant_tier`) and rotating the admin
token (`rotate_admin_token`), the only two mutate ops it supports — each internal-auth-gated,
dual-approval-bounded, MFA-bounded, and audit-logged; a missing gate key fails closed rather than
running privileged logic (`crates/corelink-container/src/routes/admin.rs:573-586`).
The per-region Durable-Object config singleton (feature flags, rate-limit tunables, retention policies)
mutated with compare-and-swap versioning — plus Queue propagation, rollback, and the monthly drill — is a
**designed control** (implemented in `corelink-config-do`, consumed only by `corelink-ops`), **NOT yet
wired into the deployed container/worker**: neither crate depends on `corelink-config-do`, and there is
no `VersionConflict` / `expected_version` enforcer in the container or worker (design-plane only,
`docs/internal/admin-plane.md:23-53`). The container admin routes do **not** gate the config-singleton.
This concept ties the operational runbook (propagation timing, CAS retry, monthly rollback drill) to the
live route code that enforces it, and keeps the config-singleton design described as the unwired target.
Related: [the operations crate cluster](/crates/operations.md) and [the D1 CONFIG_DB](/storage/d1-config-db.md).

# Role

It is where humans (operators) change CoreLink's runtime behaviour safely: an optimistic-concurrency
config store with versioned history and rollback, plus the pilot-tenant provisioning routes, all
behind one fail-closed authorization gate.

# How it works

- Config mutation is a CAS PUT carrying `expected_version`; a version mismatch returns `409 VersionConflict`, per the architecture diagram `docs/internal/admin-plane.md:23-53`.
- Clients follow a bounded CAS retry loop (GET version → modify → PUT → backoff on 409) `docs/internal/admin-plane.md:77-90`.
- The container admin router exposes the read + mutate routes `crates/corelink-container/src/routes/admin.rs:464`.
- Every privileged call is internal-auth-checked; an unconfigured key is treated as absent and rejects `crates/corelink-container/src/routes/admin.rs:79`.
- The internal-auth key resolves via a specific-then-shared env lookup `crates/corelink-container/src/routes/admin.rs:401`.
- A mutate body is parsed into a request that carries the dual-approval `(approval_id, approver)` pair `crates/corelink-container/src/routes/admin.rs:588`.
- Pilot provisioning routes (create / grant-tier / checkin) are canonical path constants `crates/corelink-container/src/routes/admin_pilot.rs:110`.
- Pilot persistence is abstracted behind a `PilotStore` trait `crates/corelink-container/src/routes/admin_pilot.rs:390`.
- Config propagation is Queue-driven with a 60s safety-net poll, targeting ≤ 5s p99 `docs/internal/admin-plane.md:93-105`.

# Invariants

- A CAS write whose `expected_version` is stale must 409, never silently overwrite `docs/internal/admin-plane.md:80-86`.
- Rollback requires fresh MFA AND a dual approver (`X-Dual-Approver`) `docs/internal/admin-plane.md:120-123`.
- The admin plane never runs privileged logic without a configured gate key (unconfigured ⇒ reject) `crates/corelink-container/src/routes/admin.rs:79`.
- A dual-approval pair must be set together: an **incomplete** pair (one of `approval_id`/`approver` present, the other absent) is rejected at request construction (`crates/corelink-container/src/routes/admin.rs:591`); a **fully-absent** pair yields `None` (no approval requested) and proceeds — the per-operation approval *requirement* is enforced in the handler, not at construction.
- Pilot mutations emit a `corelink.admin.pilot_*` audit event on attempt and success `crates/corelink-container/src/routes/admin_pilot.rs:1141-1179`.

# Gotchas

- The pilot store baseline is `InMemoryPilotStore` (non-durable) — pilot ops do not survive a restart until a D1-durable store is wired; `build_handlers` returns `InMemoryPilotStore::new()` `crates/corelink-container/src/routes/admin_pilot.rs:1031-1032`.
- A rollback target older than 90 days returns `410 VersionExpired`; older versions live only in the R2 long-term archive `docs/internal/admin-plane.md:140-142`.

# Citations

1. `docs/internal/admin-plane.md:23-53` — the DO config-singleton CAS PUT architecture.
2. `docs/internal/admin-plane.md:77-90` — the client-side CAS retry pattern.
3. `docs/internal/admin-plane.md:80-86` — 409 VersionConflict on stale expected_version.
4. `docs/internal/admin-plane.md:93-105` — the ≤5s p99 propagation timing model.
5. `docs/internal/admin-plane.md:120-123` — rollback dual-approval + fresh-MFA requirement.
6. `docs/internal/admin-plane.md:140-142` — 410 VersionExpired for >90d targets.
7. `crates/corelink-container/src/routes/admin.rs:79` — internal-auth fail-closed gate.
8. `crates/corelink-container/src/routes/admin.rs:401` — specific-then-shared internal-auth key resolution.
9. `crates/corelink-container/src/routes/admin.rs:464` — the admin read+mutate router.
10. `crates/corelink-container/src/routes/admin.rs:591` — incomplete dual-approval pair rejected at request construction (the `_ => return Err(...)` arm).
10b. `crates/corelink-container/src/routes/admin.rs:573-586` — the only two LIVE mutate ops: `set_tenant_tier` + `rotate_admin_token` (the config-singleton CAS is design-plane, not wired here).
11. `crates/corelink-container/src/routes/admin_pilot.rs:110` — canonical grant-tier pilot route const.
12. `crates/corelink-container/src/routes/admin_pilot.rs:1141-1179` — pilot create handler emits the `corelink.admin.pilot_*` audit on attempt + success.
13. `crates/corelink-container/src/routes/admin_pilot.rs:1031-1032` — `build_handlers` returns the `InMemoryPilotStore::new()` baseline.
