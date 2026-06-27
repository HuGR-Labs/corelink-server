---
type: "TenancyControl"
title: "Tenant governance: rate-limit, customer & user admin"
description: "The per-tenant request-rate bulkhead plus the self-serve customer and user-identity surfaces, all keyed fail-CLOSED on the edge-injected tenant id."
source_files:
  - "crates/corelink-container/src/routes/ratelimit_layer.rs"
  - "crates/corelink-container/src/routes/customer.rs"
  - "crates/corelink-container/src/routes/users.rs"
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/main.rs"
checkpoint_sha: "bdf83b18abf324a57d93b0bc8db8366ddea6e182"
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

- The rate limiter is wired as ONE final `router.layer(axum::middleware::from_fn_with_state(rate_limit_state, rate_limit_layer))`
  call at the end of `build_with_factory` (`crates/corelink-container/src/routes.rs:825-828`), so it covers exactly the
  composed data-plane router; `/_health` and `/_internal/*` are excluded BY ROUTE-MERGE ORDER — `main.rs`
  appends `.route("/_health", …)` to the already-rate-limited router returned by `build_with_factory`
  (`crates/corelink-container/src/main.rs:460-461`) and merges every `/_internal/*` surface AFTER it
  (`crates/corelink-container/src/main.rs:471`), so they sit OUTSIDE the layer by construction. When the D1 tier-resolver is unavailable
  the layer falls back to the team-default `RateLimitLayerState` for all tenants rather than failing the router (F-017).
- The per-tenant bucket is keyed on the edge-injected `x-corelink-tenant-id`, the trustworthy tenant
  source for the native surfaces (`crates/corelink-container/src/routes/ratelimit_layer.rs:24-35`). It is
  NOT uniformly tenant-keyed across the WHOLE data plane: the OCI surface reaches this layer with
  `x-corelink-tenant-id` deleted by the Worker (the tenant is resolved from the OCI bearer AFTER the
  limiter runs), so the per-tenant gate fail-OPENS for every OCI request. A SEPARATE per-repo limiter
  (`oci_limiter`, its own tighter config) keyed on the OCI repository name parsed from the request path
  guards the shared, unauthenticated `_oci` pool instead — `OCI_REPO_REQ_PER_SEC = 50` / `OCI_REPO_BURST
  = 200` (F-016) (`crates/corelink-container/src/routes/ratelimit_layer.rs:133-161`,
  `:204-208`). So the data-plane velocity bulkhead is tenant-keyed on the native surfaces but
  repo/realm-keyed on OCI.
- A stable 128-bit bucket key is derived from the raw tenant string via `tenant_key_uuid`, so non-UUID
  tenants still land in distinct buckets (`crates/corelink-container/src/routes/ratelimit_layer.rs:415`).
- The customer router exposes the self-serve surface — overview, usage, audit, billing, keys, team, account-delete — under
  `/v1/customer/*` (`crates/corelink-container/src/routes/customer.rs:185-198`).
- Each customer handler resolves the tenant fail-CLOSED via the `tenant()` helper: a missing/empty/sentinel
  `x-corelink-tenant-id` returns `Err(())` (the `TENANT_SENTINELS` guard) which the handler maps to `401`
  before any storage access (`crates/corelink-container/src/routes/customer.rs:225-235`).
- Revoking a credential is a destructive admin op gated on cache-write capability: `requires_cache_write`
  rejects a read-only principal with `403 FORBIDDEN` (`crates/corelink-container/src/routes/customer.rs:783-790`).
- `/v1/users/me` reflects the authenticated caller using the shared fail-CLOSED `AuthTenant` extractor and
  never echoes a sentinel or the raw PAT (`crates/corelink-container/src/routes/users.rs:106-110`).

# Invariants

- The data-plane rate limiter never covers the DO readiness probe or the internal shared-secret surfaces,
  enforced by route-merge order: `/_health` + every `/_internal/*` are appended to the rate-limited router
  AFTER `build_with_factory` returns (`crates/corelink-container/src/main.rs:460-461`,
  `crates/corelink-container/src/main.rs:471`).
- Every customer surface rejects a missing/sentinel tenant with `401` before touching that tenant's
  billing/keys/team data (`crates/corelink-container/src/routes/customer.rs:225-235`).
- Credential revocation requires cache-write scope; a read-only token cannot revoke any credential in the
  tenant (intra-tenant lockout defence) (`crates/corelink-container/src/routes/customer.rs:783-790`).
- The user-identity surface never reflects the raw PAT and rejects sentinel tenants like every other v1
  surface (`crates/corelink-container/src/routes/users.rs:18-27`).

# Gotchas

- The limiter is wired with bounded NoOp sinks in production; the `InMemory*` capture sinks push every
  decision onto unbounded `Vec`/`HashMap`s and self-OOM the container if wired by mistake (the prior bug)
  (`crates/corelink-container/src/routes/ratelimit_layer.rs:38-49`).
- `principal()` (the token prefix for audit) fails CLOSED to the `_unknown` sentinel
  (`crates/corelink-container/src/routes/customer.rs:242-245`), but `tenant()` does NOT — the tenant must be
  a real, non-sentinel value or the request is `401` (`crates/corelink-container/src/routes/customer.rs:231-234`),
  so the audit prefix and the authorization identity have deliberately different fallbacks.

# Citations

1. `crates/corelink-container/src/main.rs:460-461` — the route-merge order enforcer: `/_health` is appended to the already-rate-limited router returned by `build_with_factory`, so it sits OUTSIDE the limiter by construction.
2. `crates/corelink-container/src/main.rs:471` — `/_internal/*` surfaces (pat/mint, introspect, billing-ingest, dsr, cas-erase, tier-select, webhook) are merged AFTER `build_with_factory`, also outside the limiter (the `//!` narration is `crates/corelink-container/src/routes/ratelimit_layer.rs:15-22`).
3. `crates/corelink-container/src/routes/ratelimit_layer.rs:24-35` — the per-tenant bucket keyed on the edge-injected tenant header.
3b. `crates/corelink-container/src/routes/ratelimit_layer.rs:133-161` — the OCI per-repo velocity gate (`OCI_NS`/`OCI_REPO_REQ_PER_SEC`/`OCI_REPO_BURST`): the per-tenant gate fail-OPENS on OCI (header deleted), so a separate repo-keyed limiter guards the shared `_oci` pool.
3c. `crates/corelink-container/src/routes/ratelimit_layer.rs:204-208` — the `oci_limiter` field: a SEPARATE limiter (not tenant-keyed) for the OCI plane.
4. `crates/corelink-container/src/routes/ratelimit_layer.rs:38-49` — bounded NoOp production sinks (the InMemory-OOM trap).
5. `crates/corelink-container/src/routes/ratelimit_layer.rs:415` — `tenant_key_uuid` stable 128-bit bucket key.
6. `crates/corelink-container/src/routes/customer.rs:185-198` — the `/v1/customer/*` routes (overview/usage/audit/billing/keys/team/account-delete).
7. `crates/corelink-container/src/routes/customer.rs:225-235` — `tenant()` fail-CLOSED resolution: missing/empty/sentinel → `Err(())` → `401` before storage access.
8. `crates/corelink-container/src/routes/customer.rs:231-234` — the `TENANT_SENTINELS`/empty `Err` guard inside `tenant()`.
9. `crates/corelink-container/src/routes/customer.rs:242-245` — `principal()` falls back to `_unknown` (audit prefix only).
10. `crates/corelink-container/src/routes/customer.rs:783-790` — revoke requires cache-write scope (`requires_cache_write`); read-only → `403 FORBIDDEN`.
11. `crates/corelink-container/src/routes/users.rs:18-27` — `/v1/users/me` security model (fail-CLOSED, never reflects the PAT).
12. `crates/corelink-container/src/routes/users.rs:106-110` — the `handle_me` handler signature using the `AuthTenant` extractor.
13. `crates/corelink-container/src/routes.rs:825-828` — the single `router.layer(from_fn_with_state(...))` wiring of the rate limiter in `build_with_factory`.
