---
type: "TenancyControl"
title: "Tenant governance: rate-limit, customer & user admin"
description: "The per-tenant request-rate bulkhead plus the self-serve customer and user-identity surfaces, all keyed fail-CLOSED on the edge-injected tenant id."
source_files:
  - "crates/corelink-container/src/oci_suspend.rs"
  - "crates/corelink-container/src/routes/ratelimit_layer.rs"
  - "crates/corelink-container/src/routes/customer.rs"
  - "crates/corelink-container/src/routes/customer_export.rs"
  - "crates/corelink-container/src/routes/customer_runners.rs"
  - "crates/corelink-container/src/routes/workspaces.rs"
  - "crates/corelink-container/src/routes/users.rs"
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/usage_meter.rs"
  - "crates/corelink-ratelimit/src/audit.rs"
  - "crates/corelink-ratelimit/src/metrics.rs"
checkpoint_sha: "294982663d1b05462077c616ca00886be16159e7"
provenance: "AUTHORED"
tags: ["tenancy", "governance", "rate-limit", "customer", "users", "fail-closed"]
timestamp: "2026-06-26T00:00:00Z"
---

# Tenant governance: rate-limit, customer & user admin

Beyond the spend, count, and byte ceilings, a tenant has a *velocity* bulkhead and a set of self-serve
admin surfaces — and all three must read the same trusted tenant identity and none other. The
in-app rate limiter bounds request velocity per tenant (a single PAT could otherwise hammer the shared
container as long as it stayed under its byte quota); the `/v1/customer/*` surface exposes billing, keys,
and team management; and `/v1/users/me` is the canonical "who am I" reflection. Every one of these keys
on the DO-injected `x-corelink-tenant-id` and fails CLOSED on a missing or sentinel value, so governance
actions can never act on the wrong tenant.

# Role

This control is the operational/administrative face of tenancy — it complements the metering triad (the
[$-ceiling](/tenancy/dollar-ceiling.md), [request-quota](/tenancy/request-quota.md), and
[storage-quota header](/tenancy/storage-quota-header.md)) with the velocity bulkhead and the customer
self-service plane. It rests on the same trusted tenant id established by
[tenant isolation](/tenancy/isolation.md), and its key-management surface writes through the
[D1 PAT store](/auth/d1-pat-store.md).

# How it works

- **Container-side tenant-suspend gate on the OCI registry plane (go-live G4b).** The worker-side suspend
  gate denies a suspended/erased tenant at the edge, but the OCI plane is reached by a bearer minted from a
  short-lived token exchange, so a suspend could leak for the token's lifetime. `oci_suspend.rs` re-checks
  `tenant_offboarding_state.state ∈ {suspended, erased}` on every OCI op and 403s a suspended tenant before
  the request runs (`crates/corelink-container/src/oci_suspend.rs:85-86` — the `state_denies` predicate:
  only the terminal `suspended`/`erased` states deny; the earlier grace/export windows keep access by
  design). Unlike the fail-OPEN request-count cap, the suspend verdict is fail-CLOSED **sticky**: a tenant
  already known suspended stays denied through a transient D1 read fault (the error is never cached, so a
  later successful read can flip it back), and only an UNKNOWN tenant fails OPEN
  (`crates/corelink-container/src/oci_suspend.rs:250-283` — `CachedSuspendResolver::suspended_state`,
  single-flight + TTL cache with the sticky read-error fold).
- The rate limiter is wired as ONE `.layer(...)` line at the end of `build_with_factory`
  (`crates/corelink-container/src/routes.rs:1007-1010`), so it covers exactly the composed data-plane router;
  `/_health` and `/_internal/*` are merged in `main.rs` AFTER `build_with_factory` returns and are therefore
  outside this layer by construction (the executed enforcer is `rate_limit_layer`,
  `crates/corelink-container/src/routes/ratelimit_layer.rs:436-462`).
- The bucket is keyed on the edge-injected `x-corelink-tenant-id`, the only trustworthy tenant source in
  the container — read in `rate_limit_layer` via the `TENANT_HEADER` lookup
  (`crates/corelink-container/src/routes/ratelimit_layer.rs:441-446`, `crates/corelink-container/src/routes/ratelimit_layer.rs:462`).
- A stable 128-bit bucket key is derived from the raw tenant string via `tenant_key_uuid`, so non-UUID
  tenants still land in distinct buckets (`crates/corelink-container/src/routes/ratelimit_layer.rs:414`).
- The customer router exposes the self-serve surface — overview, usage, audit, billing, keys, team — under
  `/v1/customer/*` (`crates/corelink-container/src/routes/customer.rs:206`).
- Each customer handler resolves the tenant fail-CLOSED: a missing/empty/sentinel header is an `Err` the
  handler maps to `401` before any storage access
  (`crates/corelink-container/src/routes/customer.rs:251-261`).
- Revoking a credential is a destructive admin op gated on cache-write capability: a read-only principal
  is rejected `403` (`crates/corelink-container/src/routes/customer.rs:794-822`).
- `/v1/users/me` reflects the authenticated caller using the shared fail-CLOSED `AuthTenant` extractor and
  never echoes a sentinel or the raw PAT (`crates/corelink-container/src/routes/users.rs:106-110`).
- The `/v1/customer/usage` ROI surface is fed by the in-process DISPLAY aggregator
  `crate::usage_meter::UsageMeter`: a hot-path-safe `record(tenant, ReadHit|ReadMiss|Write)` that only bumps
  in-memory counters under a lock (no await/IO on the request path)
  (`crates/corelink-container/src/usage_meter.rs:167-179`), plus a background flusher that additively UPSERTs
  the accumulated deltas into the per-tenant `usage_daily` D1 rollup (migration 0089)
  (`crates/corelink-container/src/usage_meter.rs:256-276`). It is per-tenant DISPLAY telemetry ONLY — never a
  rate-limit, quota, or billing input, so a lost flush degrades a chart, never a gate.
- The `/v1/customer/export` tenant self-service data-export (portability) surface is single-sourced in
  `customer_export`: the customer router's `export` collaborator is built by `from_handlers_and_env` and the
  streamed bundle is assembled by `build_export_stream` (throttled by a dedicated per-tenant export rate
  limiter, `build_export_rate_limiter`) so a tenant can pull its own cached-state + metadata without a
  parallel gather path (`crates/corelink-container/src/routes/customer_export.rs:216-240`,
  `crates/corelink-container/src/routes/customer_export.rs:522`).
- Two more `/v1/customer/*` tenant-scoped surfaces ride the same trusted-tenant discipline. The Runners
  read surface (`customer_runners`) resolves the tenant fail-CLOSED — a missing/empty/sentinel
  `x-corelink-tenant-id` is an `Err(())` mapped to 401 before any storage access
  (`crates/corelink-container/src/routes/customer_runners.rs:139`) — and serves entitlement / allowlist /
  runs strictly `WHERE tenant_id = ?1` (`crates/corelink-container/src/routes/customer_runners.rs:244`);
  these are READs and carry no extra write-scope gate.
- The Workspaces surface (`workspaces`) resolves the tenant the same fail-CLOSED way
  (`crates/corelink-container/src/routes/workspaces.rs:155`) and exposes list / create / delete / pin, every
  statement bound `WHERE tenant_id = ?1` (`crates/corelink-container/src/routes/workspaces.rs:273`). Its
  MUTATIONS (create/delete/pin) additionally require the Worker-trusted cache-write capability — a
  read-only (`cas:r`) token is rejected 403 by `write_scope_gate_reject`
  (`crates/corelink-container/src/routes/workspaces.rs:195`), invoked at the top of each mutating handler
  (`crates/corelink-container/src/routes/workspaces.rs:294`); an unwired `db` fails writes CLOSED (503)
  rather than pretending to persist.

# Invariants

- The data-plane rate limiter never covers the DO readiness probe or the internal shared-secret surfaces:
  the layer wraps only the router returned by `build_with_factory`, and `/_health` + `/_internal/*` are
  merged AFTER it in `main.rs` — so exclusion is structural, not a path check inside `rate_limit_layer`
  (`crates/corelink-container/src/routes.rs:1007-1010`; the executed enforcer is
  `crates/corelink-container/src/routes/ratelimit_layer.rs:436-462`).
- Every customer surface rejects a missing/sentinel tenant with `401` before touching that tenant's
  billing/keys/team data (`crates/corelink-container/src/routes/customer.rs:251-261`).
- Credential revocation requires cache-write scope; a read-only token cannot revoke any credential in the
  tenant (intra-tenant lockout defence) (`crates/corelink-container/src/routes/customer.rs:811-817`).
- The user-identity surface never reflects the raw PAT and rejects sentinel tenants like every other v1
  surface (`crates/corelink-container/src/routes/users.rs:18-27`).

# Gotchas

- The limiter is wired with bounded NoOp sinks in production — `RateLimitLayerState::new` /
  `with_tier_resolver` construct them (`crates/corelink-container/src/routes.rs:970-988`); the crate's
  `InMemoryRateLimitAuditSink` / `InMemoryRateLimitMetrics` capture sinks
  (`crates/corelink-ratelimit/src/audit.rs:147`, `crates/corelink-ratelimit/src/metrics.rs:177`) push every
  decision onto unbounded `Vec`/`HashMap`s and would self-OOM the container if wired here by mistake (the
  prior bug).
- `principal()` (the token prefix for audit) fails CLOSED to the `_unknown` sentinel
  (`crates/corelink-container/src/routes/customer.rs:269-271`), but `tenant()` does NOT — the tenant must be
  a real, non-sentinel value or the request is `401` (`crates/corelink-container/src/routes/customer.rs:251-261`),
  so the audit prefix and the authorization identity have deliberately different fallbacks.

# Citations

1. `crates/corelink-container/src/routes/ratelimit_layer.rs:436-462` — the EXECUTED `rate_limit_layer` enforcer (the `:15-49` ranges are the module `//!` doc-comments describing it).
2. `crates/corelink-container/src/routes.rs:1007-1010` — the single `.layer(...)` wiring; `/_health` + `/_internal/*` are merged AFTER it in `main.rs`, so exclusion is structural (not a path check inside the layer).
3. `crates/corelink-container/src/routes/ratelimit_layer.rs:441-446` — the executed keying: read of the edge-injected `TENANT_HEADER` (`x-corelink-tenant-id`).
4. `crates/corelink-ratelimit/src/audit.rs:147`, `crates/corelink-ratelimit/src/metrics.rs:177` — the `InMemoryRateLimit*` capture sinks (unbounded `Vec`/`HashMap`); the prod NoOp sinks are constructed at `crates/corelink-container/src/routes.rs:970-988`.
5. `crates/corelink-container/src/routes/ratelimit_layer.rs:414` — `tenant_key_uuid` stable 128-bit bucket key.
6. `crates/corelink-container/src/routes/customer.rs:206` — the `/v1/customer/*` router (overview/usage/audit/billing/keys/team).
7. `crates/corelink-container/src/routes/customer.rs:251-261` — `tenant()`: fail-CLOSED resolution returning `Err` on missing/sentinel before storage access (handler maps to `401`).
9. `crates/corelink-container/src/routes/customer.rs:269-271` — `principal()` falls back to `_unknown` (audit prefix only).
10. `crates/corelink-container/src/routes/customer.rs:794-822` — `handle_keys_revoke`: revoke requires cache-write scope; read-only → `403`.
11. `crates/corelink-container/src/routes/users.rs:18-27` — `/v1/users/me` security model (fail-CLOSED, never reflects the PAT).
12. `crates/corelink-container/src/routes/users.rs:106-110` — the `handle_me` handler signature using the `AuthTenant` extractor.
13. `crates/corelink-container/src/routes.rs:1007-1010` — the single `.layer(...)` wiring of the rate limiter in `build_with_factory`.
14. `crates/corelink-container/src/routes/customer_runners.rs:139` — `tenant()`: fail-CLOSED resolution (missing/empty/sentinel `x-corelink-tenant-id` ⇒ `Err(())` → 401) before any storage access.
15. `crates/corelink-container/src/routes/customer_runners.rs:244` — a tenant-scoped `WHERE tenant_id = ?1` read (runners entitlement).
16. `crates/corelink-container/src/routes/workspaces.rs:155` — `tenant()`: same fail-CLOSED missing/sentinel ⇒ 401 resolution.
17. `crates/corelink-container/src/routes/workspaces.rs:195` — `write_scope_gate_reject`: mutations require the Worker-trusted cache-write capability; a read-only token → 403.
18. `crates/corelink-container/src/routes/workspaces.rs:273` — a tenant-scoped `WHERE tenant_id = ?1` list read (every workspaces statement is tenant-bound).
19. `crates/corelink-container/src/usage_meter.rs:167-179` — `UsageMeter::record`: the hot-path-safe in-memory counter increment behind a lock (DISPLAY telemetry, never a gate).
20. `crates/corelink-container/src/usage_meter.rs:256-276` — the `usage_daily` D1 sink: the background flusher's additive `INSERT … ON CONFLICT DO UPDATE` UPSERT of accumulated deltas (migration 0089).
21. `crates/corelink-container/src/oci_suspend.rs:85-86` — `state_denies`: only the terminal `suspended`/`erased` offboarding states deny on the OCI plane (grace/export windows keep access).
22. `crates/corelink-container/src/oci_suspend.rs:250-283` — `CachedSuspendResolver::suspended_state`: single-flight + TTL cache with the fail-CLOSED sticky read-error fold (known-suspended stays denied through a D1 fault; unknown fails OPEN).
23. `crates/corelink-container/src/routes/customer_export.rs:216-240` — `build_export_stream`: assembles the `/v1/customer/export` tenant self-service data-export (portability) bundle; `from_handlers_and_env` (`crates/corelink-container/src/routes/customer_export.rs:522`) builds the router's `export` collaborator from env.
