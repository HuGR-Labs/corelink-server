---
type: "Runbook"
title: "The admin plane: config-singleton CAS + dual-approval mutate + pilot lifecycle"
description: "How operators mutate runtime config with optimistic concurrency and dual approval, and how the container admin routes gate every privileged action."
checkpoint_sha: "648ecdccd229bdb5154b86843053c28b9cce9d36"
source_files:
  - "crates/corelink-container/src/routes/admin/part-01.rs"
  - "crates/corelink-container/src/routes/admin/part-00-00.rs"
  - "crates/corelink-container/src/routes/admin/part-00-02.rs"
  - "crates/corelink-container/src/routes/admin_pilot/part-00-00.rs"
  - "crates/corelink-container/src/routes/admin_pilot/part-01-01.rs"
  - "crates/corelink-container/src/routes/admin_tenant_detail.rs"
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/routes/build.rs"
  - "docs/internal/admin-plane.md"
source_blobs:
  - "crates/corelink-container/src/routes/admin/part-00-00.rs@d194ecab8cee6ec8a4ad93324306448336e394a2"
  - "crates/corelink-container/src/routes/admin/part-00-02.rs@3a73973052bf7781e71375e27f0895d67b5e7f20"
  - "crates/corelink-container/src/routes/admin/part-01.rs@c59a9716d2c92b25bce4a4e19d3cebf72b58f653"
  - "crates/corelink-container/src/routes/admin_pilot/part-00-00.rs@9b22b2a3cf27b48a0b515204624a72fabe98284d"
  - "crates/corelink-container/src/routes/admin_pilot/part-01-01.rs@6195ffddd9b75cb6fd88c0a5dfba60cb01622e4e"
  - "crates/corelink-container/src/routes/build.rs@43036d91a76bf2e14d6d49c2bca705037fc02081"
---
# The admin plane: config-singleton CAS + dual-approval mutate + pilot lifecycle

The admin plane is the operator control surface. The **LIVE** container admin mutate route gates
privileged tenant-scoped actions — granting a tenant tier (`set_tenant_tier`) and rotating the admin
token (`rotate_admin_token`), the only two mutate ops it supports — each internal-auth-gated,
dual-approval-bounded, MFA-bounded, and audit-logged; a missing gate key fails closed rather than
running privileged logic (`crates/corelink-container/src/routes/admin/part-01.rs:50-65`).
Alongside the admin router sits the operator per-tenant deep-dive READ surface `admin_tenant_detail`
(usage / billing / consents / dsr / pats) — an operator-scoped read enrichment for the admin console that
reuses the SAME constant-time `internal_auth_ok` gate as `admin.rs` (now shared as a `pub(crate)` fn),
so both surfaces share one operator-auth boundary; the Worker strips internal-auth on `/v1/*`, keeping the
whole family operator-only (`crates/corelink-container/src/routes/admin_tenant_detail.rs:156`).
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
- The container admin router exposes the read + mutate routes `crates/corelink-container/src/routes/admin/part-00-02.rs:252-252`.
- Every privileged call is internal-auth-checked; an unconfigured key is treated as absent and rejects `crates/corelink-container/src/routes/admin/part-00-00.rs:87-87`.
- The internal-auth key resolves via a specific-then-shared env lookup `crates/corelink-container/src/routes/admin/part-00-02.rs:114-139`.
- A mutate body is parsed into a request that carries the dual-approval `(approval_id, approver)` pair `crates/corelink-container/src/routes/admin/part-01.rs:67`.
- The recording half of two-person control — `POST /v1/admin/approve` — is gated by a SEPARATE, **dedicated-only** approver key: `approver_auth_key_from_env` resolves `CORELINK_ADMIN_APPROVER_AUTH_KEY` ONLY, with NO fallback to the shared `CORELINK_INTERNAL_AUTH_KEY` (same no-fallback treatment as the erase authority, finding H4), so an unset/blank/too-short value fails the approve gate CLOSED (`403`) rather than silently borrowing the mutate key `crates/corelink-container/src/routes/admin/part-00-02.rs:201-201`. The gate itself checks that key before the body is parsed and `403`s on a miss `crates/corelink-container/src/routes/admin/part-01.rs:339-339`.
- Credential separation is enforced in code at boot, not left to operator discipline: `approver_key_distinct_or_none` constant-time-compares the resolved approver key against the internal/mutate key and, on a byte-equal match, collapses it to `None` (logs + fails approve CLOSED) so a mis-provisioned key equal to `CORELINK_INTERNAL_AUTH_KEY` cannot silently re-collapse approve+mutate onto one shared secret `crates/corelink-container/src/routes.rs:490`; it is wired into the boot path in `build_with_factory` `crates/corelink-container/src/routes/build.rs:315-315`.
- The operator per-tenant read surface reuses that SAME `internal_auth_ok` gate and returns `403` fail-CLOSED before any storage read `crates/corelink-container/src/routes/admin_tenant_detail.rs:162`, then binds the target `tenant_id` into every tenant-scoped `WHERE tenant_id = ?1` query (usage/billing/consents/dsr/pats) `crates/corelink-container/src/routes/admin_tenant_detail.rs:207`.
- Pilot provisioning routes (create / grant-tier / checkin) are canonical path constants using axum-0.8 `{tenant_id}` capture syntax `crates/corelink-container/src/routes/admin_pilot/part-00-00.rs:109-109`.
- Pilot persistence is abstracted behind a `PilotStore` trait `crates/corelink-container/src/routes/admin_pilot/part-00-00.rs:415-415`.
- Config propagation is Queue-driven with a 60s safety-net poll, targeting ≤ 5s p99 `docs/internal/admin-plane.md:93-105`.

# Invariants

- A CAS write whose `expected_version` is stale must 409, never silently overwrite `docs/internal/admin-plane.md:80-86`.
- Rollback requires fresh MFA AND a dual approver (`X-Dual-Approver`) `docs/internal/admin-plane.md:120-123`.
- The admin plane never runs privileged logic without a configured gate key (unconfigured ⇒ reject) `crates/corelink-container/src/routes/admin/part-00-00.rs:87-87`.
- A dual-approval pair must be set together: an **incomplete** pair (one of `approval_id`/`approver` present, the other absent) is rejected at request construction (`crates/corelink-container/src/routes/admin/part-01.rs:70`); a **fully-absent** pair yields `None` (no approval requested) and proceeds — the per-operation approval *requirement* is enforced in the handler, not at construction.
- Two-person control is only real when the approve gate and the mutate gate hold DIFFERENT secrets: the approver key is dedicated-only (no shared-key fallback) `crates/corelink-container/src/routes/admin/part-00-02.rs:201-201` AND is forced distinct from the internal/mutate key at boot — a byte-equal approver key collapses to `None` so `POST /v1/admin/approve` fails CLOSED `crates/corelink-container/src/routes.rs:490`.
- Pilot mutations emit a `corelink.admin.pilot_*` audit event on attempt and success `crates/corelink-container/src/routes/admin_pilot/part-01-01.rs:231-269`.

# Gotchas

- The pilot store baseline is `InMemoryPilotStore` (non-durable) — pilot ops do not survive a restart until a D1-durable store is wired; `build_handlers` returns `InMemoryPilotStore::new()` `crates/corelink-container/src/routes/admin_pilot/part-01-01.rs:121-122`.
- A rollback target older than 90 days returns `410 VersionExpired`; older versions live only in the R2 long-term archive `docs/internal/admin-plane.md:140-142`.

# Citations

1. `docs/internal/admin-plane.md:23-53` — the DO config-singleton CAS PUT architecture.
2. `docs/internal/admin-plane.md:77-90` — the client-side CAS retry pattern.
3. `docs/internal/admin-plane.md:80-86` — 409 VersionConflict on stale expected_version.
4. `docs/internal/admin-plane.md:93-105` — the ≤5s p99 propagation timing model.
5. `docs/internal/admin-plane.md:120-123` — rollback dual-approval + fresh-MFA requirement.
6. `docs/internal/admin-plane.md:140-142` — 410 VersionExpired for >90d targets.
7. `crates/corelink-container/src/routes/admin/part-00-00.rs:87-87` — internal-auth fail-closed gate (absent/unconfigured key ⇒ `return false`).
8. `crates/corelink-container/src/routes/admin/part-00-02.rs:114-139` — specific-then-shared internal-auth key resolution.
9. `crates/corelink-container/src/routes/admin/part-00-02.rs:252-252` — the admin read+mutate router.
10. `crates/corelink-container/src/routes/admin/part-01.rs:70` — incomplete dual-approval pair rejected at request construction (the `_ => return Err(...)` arm).
10b. `crates/corelink-container/src/routes/admin/part-01.rs:50-65` — the only two LIVE mutate ops: `set_tenant_tier` + `rotate_admin_token` (`_ => return Err("unknown op_kind")`; the config-singleton CAS is design-plane, not wired here).
10e. `crates/corelink-container/src/routes/admin/part-00-02.rs:201-201` — `approver_auth_key_from_env` resolves `CORELINK_ADMIN_APPROVER_AUTH_KEY` via `resolve_dedicated_auth_key` — dedicated-only, NO shared `CORELINK_INTERNAL_AUTH_KEY` fallback (approve fails CLOSED when unset).
10f. `crates/corelink-container/src/routes/admin/part-01.rs:339-339` — the `handle_approve` gate checks `approver_auth_key` before body parse and `403`s fail-CLOSED on a miss.
10g. `crates/corelink-container/src/routes.rs:490` — `approver_key_distinct_or_none`: constant-time byte-equality vs the internal/mutate key; a byte-equal approver key collapses to `None` (boot-enforced credential separation).
10h. `crates/corelink-container/src/routes/build.rs:315-315` — `build_with_factory` wires the approver key through `approver_key_distinct_or_none` against the internal auth key at boot.
10c. `crates/corelink-container/src/routes/admin_tenant_detail.rs:162` — the shared-`internal_auth_ok` operator gate returns `403` fail-CLOSED before any storage access (bad/absent internal-auth).
10d. `crates/corelink-container/src/routes/admin_tenant_detail.rs:207` — binds the target `tenant_id` into a tenant-scoped `WHERE tenant_id = ?1` operator read (usage/`tenant_storage_state`).
11. `crates/corelink-container/src/routes/admin_pilot/part-00-00.rs:109-109` — canonical grant-tier pilot route const.
12. `crates/corelink-container/src/routes/admin_pilot/part-01-01.rs:231-269` — pilot create handler emits the `corelink.admin.pilot_*` audit on attempt + success.
13. `crates/corelink-container/src/routes/admin_pilot/part-01-01.rs:121-122` — `build_handlers` returns the `InMemoryPilotStore::new()` baseline.


# Revalidation

This concept was revalidated against the cumulative implementation tree; its existing source citations remain the controlling evidence for the behavior described above.
