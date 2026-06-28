---
type: "TenancyControl"
title: "Tenant governance: rate-limit, customer & user admin"
description: "The per-tenant request-rate bulkhead plus the self-serve customer and user-identity surfaces, all keyed fail-CLOSED on the edge-injected tenant id."
source_files:
  - "crates/corelink-container/src/routes/ratelimit_layer.rs"
  - "crates/corelink-container/src/routes/customer.rs"
  - "crates/corelink-container/src/routes/users.rs"
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-ratelimit/src/audit.rs"
  - "crates/corelink-ratelimit/src/metrics.rs"
checkpoint_sha: "01274fea4ad4228fbf2e0633443f759fb40582b0"
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

- The rate limiter is wired as ONE `.layer(...)` line at the end of `build_with_factory`
  (`crates/corelink-container/src/routes.rs:849-852`), so it covers exactly the composed data-plane router;
  `/_health` and `/_internal/*` are merged in `main.rs` AFTER `build_with_factory` returns and are therefore
  outside this layer by construction (the executed enforcer is `rate_limit_layer`,
  `crates/corelink-container/src/routes/ratelimit_layer.rs:437-463`).
- The bucket is keyed on the edge-injected `x-corelink-tenant-id`, the only trustworthy tenant source in
  the container — read in `rate_limit_layer` via the `TENANT_HEADER` lookup
  (`crates/corelink-container/src/routes/ratelimit_layer.rs:442-447`, `crates/corelink-container/src/routes/ratelimit_layer.rs:463`).
- A stable 128-bit bucket key is derived from the raw tenant string via `tenant_key_uuid`, so non-UUID
  tenants still land in distinct buckets (`crates/corelink-container/src/routes/ratelimit_layer.rs:415`).
- The customer router exposes the self-serve surface — overview, usage, audit, billing, keys, team — under
  `/v1/customer/*` (`crates/corelink-container/src/routes/customer.rs:184`).
- Each customer handler resolves the tenant fail-CLOSED: a missing/empty/sentinel header is an `Err` the
  handler maps to `401` before any storage access
  (`crates/corelink-container/src/routes/customer.rs:225-235`).
- Revoking a credential is a destructive admin op gated on cache-write capability: a read-only principal
  is rejected `403` (`crates/corelink-container/src/routes/customer.rs:763-787`).
- `/v1/users/me` reflects the authenticated caller using the shared fail-CLOSED `AuthTenant` extractor and
  never echoes a sentinel or the raw PAT (`crates/corelink-container/src/routes/users.rs:106-110`).

# Invariants

- The data-plane rate limiter never covers the DO readiness probe or the internal shared-secret surfaces:
  the layer wraps only the router returned by `build_with_factory`, and `/_health` + `/_internal/*` are
  merged AFTER it in `main.rs` — so exclusion is structural, not a path check inside `rate_limit_layer`
  (`crates/corelink-container/src/routes.rs:849-852`; the executed enforcer is
  `crates/corelink-container/src/routes/ratelimit_layer.rs:437-463`).
- Every customer surface rejects a missing/sentinel tenant with `401` before touching that tenant's
  billing/keys/team data (`crates/corelink-container/src/routes/customer.rs:225-235`).
- Credential revocation requires cache-write scope; a read-only token cannot revoke any credential in the
  tenant (intra-tenant lockout defence) (`crates/corelink-container/src/routes/customer.rs:782-787`).
- The user-identity surface never reflects the raw PAT and rejects sentinel tenants like every other v1
  surface (`crates/corelink-container/src/routes/users.rs:18-27`).

# Gotchas

- The limiter is wired with bounded NoOp sinks in production — `RateLimitLayerState::new` /
  `with_tier_resolver` construct them (`crates/corelink-container/src/routes.rs:830-848`); the crate's
  `InMemoryRateLimitAuditSink` / `InMemoryRateLimitMetrics` capture sinks
  (`crates/corelink-ratelimit/src/audit.rs:147`, `crates/corelink-ratelimit/src/metrics.rs:177`) push every
  decision onto unbounded `Vec`/`HashMap`s and would self-OOM the container if wired here by mistake (the
  prior bug).
- `principal()` (the token prefix for audit) fails CLOSED to the `_unknown` sentinel
  (`crates/corelink-container/src/routes/customer.rs:243-245`), but `tenant()` does NOT — the tenant must be
  a real, non-sentinel value or the request is `401` (`crates/corelink-container/src/routes/customer.rs:225-235`),
  so the audit prefix and the authorization identity have deliberately different fallbacks.

# Citations

1. `crates/corelink-container/src/routes/ratelimit_layer.rs:437-463` — the EXECUTED `rate_limit_layer` enforcer (the `:15-49` ranges are the module `//!` doc-comments describing it).
2. `crates/corelink-container/src/routes.rs:849-852` — the single `.layer(...)` wiring; `/_health` + `/_internal/*` are merged AFTER it in `main.rs`, so exclusion is structural (not a path check inside the layer).
3. `crates/corelink-container/src/routes/ratelimit_layer.rs:442-447` — the executed keying: read of the edge-injected `TENANT_HEADER` (`x-corelink-tenant-id`).
4. `crates/corelink-ratelimit/src/audit.rs:147`, `crates/corelink-ratelimit/src/metrics.rs:177` — the `InMemoryRateLimit*` capture sinks (unbounded `Vec`/`HashMap`); the prod NoOp sinks are constructed at `crates/corelink-container/src/routes.rs:830-848`.
5. `crates/corelink-container/src/routes/ratelimit_layer.rs:415` — `tenant_key_uuid` stable 128-bit bucket key.
6. `crates/corelink-container/src/routes/customer.rs:184` — the `/v1/customer/*` router (overview/usage/audit/billing/keys/team).
7. `crates/corelink-container/src/routes/customer.rs:225-235` — `tenant()`: fail-CLOSED resolution returning `Err` on missing/sentinel before storage access (handler maps to `401`).
9. `crates/corelink-container/src/routes/customer.rs:243-245` — `principal()` falls back to `_unknown` (audit prefix only).
10. `crates/corelink-container/src/routes/customer.rs:763-787` — `handle_keys_revoke`: revoke requires cache-write scope; read-only → `403`.
11. `crates/corelink-container/src/routes/users.rs:18-27` — `/v1/users/me` security model (fail-CLOSED, never reflects the PAT).
12. `crates/corelink-container/src/routes/users.rs:106-110` — the `handle_me` handler signature using the `AuthTenant` extractor.
13. `crates/corelink-container/src/routes.rs:849-852` — the single `.layer(...)` wiring of the rate limiter in `build_with_factory`.
