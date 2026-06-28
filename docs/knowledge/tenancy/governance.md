---
type: "TenancyControl"
title: "Tenant governance: rate-limit, customer & user admin"
description: "The per-tenant request-rate bulkhead plus the self-serve customer and user-identity surfaces, all keyed fail-CLOSED on the edge-injected tenant id."
source_files:
  - "crates/corelink-container/src/routes/ratelimit_layer.rs"
  - "crates/corelink-container/src/routes/customer.rs"
  - "crates/corelink-container/src/routes/users.rs"
  - "crates/corelink-container/src/routes.rs"
checkpoint_sha: "57fd1bbeba017a3a9ac60d1a045728295fcf88d7"
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
  (`crates/corelink-container/src/routes.rs:837-840`), so it covers exactly the composed data-plane router
  and intentionally excludes `/_health` and `/_internal/*`
  (`crates/corelink-container/src/routes/ratelimit_layer.rs:15-22`).
- The bucket is keyed on the edge-injected `x-corelink-tenant-id`, the only trustworthy tenant source in
  the container (`crates/corelink-container/src/routes/ratelimit_layer.rs:24-35`).
- A stable 128-bit bucket key is derived from the raw tenant string via `tenant_key_uuid`, so non-UUID
  tenants still land in distinct buckets (`crates/corelink-container/src/routes/ratelimit_layer.rs:415`).
- The customer router exposes the self-serve surface — overview, usage, audit, billing, keys, team — under
  `/v1/customer/*` (`crates/corelink-container/src/routes/customer.rs:183-198`).
- Each customer handler resolves the tenant fail-CLOSED: a missing/empty/sentinel header is an `Err` the
  handler maps to `401` before any storage access
  (`crates/corelink-container/src/routes/customer.rs:210-220`).
- Revoking a credential is a destructive admin op gated on cache-write capability: a read-only principal
  is rejected `403` (`crates/corelink-container/src/routes/customer.rs:762-775`).
- `/v1/users/me` reflects the authenticated caller using the shared fail-CLOSED `AuthTenant` extractor and
  never echoes a sentinel or the raw PAT (`crates/corelink-container/src/routes/users.rs:106-110`).

# Invariants

- The data-plane rate limiter never covers the DO readiness probe or the internal shared-secret surfaces
  (`crates/corelink-container/src/routes/ratelimit_layer.rs:18-22`).
- Every customer surface rejects a missing/sentinel tenant with `401` before touching that tenant's
  billing/keys/team data (`crates/corelink-container/src/routes/customer.rs:203-220`).
- Credential revocation requires cache-write scope; a read-only token cannot revoke any credential in the
  tenant (intra-tenant lockout defence) (`crates/corelink-container/src/routes/customer.rs:768-775`).
- The user-identity surface never reflects the raw PAT and rejects sentinel tenants like every other v1
  surface (`crates/corelink-container/src/routes/users.rs:18-27`).

# Gotchas

- The limiter is wired with bounded NoOp sinks in production; the `InMemory*` capture sinks push every
  decision onto unbounded `Vec`/`HashMap`s and self-OOM the container if wired by mistake (the prior bug)
  (`crates/corelink-container/src/routes/ratelimit_layer.rs:38-49`).
- `principal()` (the token prefix for audit) fails CLOSED to the `_unknown` sentinel, but `tenant()` does
  NOT — the tenant must be a real, non-sentinel value or the request is `401`, so the audit prefix and the
  authorization identity have deliberately different fallbacks
  (`crates/corelink-container/src/routes/customer.rs:227-230`).

# Citations

1. `crates/corelink-container/src/routes/ratelimit_layer.rs:15-22` — the limiter is one layer over the data-plane router; excludes health/internal.
2. `crates/corelink-container/src/routes/ratelimit_layer.rs:18-22` — health probe + internal surfaces are outside the limiter.
3. `crates/corelink-container/src/routes/ratelimit_layer.rs:24-35` — keyed on the edge-injected tenant header.
4. `crates/corelink-container/src/routes/ratelimit_layer.rs:38-49` — bounded NoOp production sinks (the InMemory-OOM trap).
5. `crates/corelink-container/src/routes/ratelimit_layer.rs:415` — `tenant_key_uuid` stable 128-bit bucket key.
6. `crates/corelink-container/src/routes/customer.rs:168-183` — the `/v1/customer/*` router (overview/usage/audit/billing/keys/team).
7. `crates/corelink-container/src/routes/customer.rs:203-220` — fail-CLOSED tenant resolution (sentinel → `401`).
8. `crates/corelink-container/src/routes/customer.rs:210-220` — `tenant()` returns `Err` on missing/sentinel before storage access.
9. `crates/corelink-container/src/routes/customer.rs:227-230` — `principal()` falls back to `_unknown` (audit prefix only).
10. `crates/corelink-container/src/routes/customer.rs:762-775` — revoke requires cache-write scope; read-only → `403`.
11. `crates/corelink-container/src/routes/users.rs:18-27` — `/v1/users/me` security model (fail-CLOSED, never reflects the PAT).
12. `crates/corelink-container/src/routes/users.rs:106-110` — the `handle_me` handler signature using the `AuthTenant` extractor.
13. `crates/corelink-container/src/routes.rs:837-840` — the single `.layer(...)` wiring of the rate limiter in `build_with_factory`.
