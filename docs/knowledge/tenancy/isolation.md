---
type: "TenancyControl"
title: "Tenant isolation via idFromName(tenant_id)"
description: "How CoreLink keeps one tenant's traffic, state, and storage keys structurally separate from every other tenant's across the Worker, the Durable Object, and the container."
source_files:
  - "worker/src/index.ts"
  - "crates/corelink-worker/src/tenant.rs"
  - "crates/corelink-container/src/auth_tenant.rs"
  - "crates/tenant-path/src/lib.rs"
  - "crates/tenant-path/src/prefix.rs"
checkpoint_sha: "e5c838d365a175d617f8368b3749a463a49bb750"
provenance: "AUTHORED"
tags: ["tenancy", "isolation", "durable-object", "multi-tenant", "security"]
timestamp: "2026-06-26T00:00:00Z"
---

# Tenant isolation via idFromName(tenant_id)

CoreLink is a multi-tenant cache: every blob, every counter, every credential belongs to exactly one
tenant, and a cross-tenant leak is the single worst failure the platform can have. Isolation is enforced
at three layers that reinforce each other — the Worker routes each request to a *per-tenant* Durable
Object named by the tenant id, the container refuses to act without a non-sentinel authenticated tenant,
and every R2/KV/D1 key is namespaced under an HMAC-derived prefix that *cannot* be constructed from a
mismatched `(tenant_id, prefix)` pair. The trust root is "the tenant id the Worker resolved from the
PAT"; everything downstream is keyed off that one value and nothing else.

# Role

This control sits at the spine of the plane topology: it is what makes "multi-tenant" safe rather than
just "shared." It is the boundary the [PAT moat](/auth/pat-moat.md) and the
[D1 PAT store](/auth/d1-pat-store.md) feed into — auth resolves *which* tenant, and isolation guarantees
that resolved tenant is the *only* one whose data the rest of the request can touch. Every other tenancy
control in this directory (the $-ceiling, the request quota, the storage-quota header, governance) is
keyed on the same trusted tenant id this control establishes.

# How it works

- The Worker maps each tenant to its own Durable Object instance via `idFromName(resolvedTenantId)` —
  derived from the Worker-resolved tenant id, never from client input — so a single DO is the sole
  serialization point for that tenant's state; the local (this-region) DO id is always keyed on
  `resolvedTenantId`, never the shared `_pending_auth` — `worker/src/index.ts:3013`.
- Non-tenant system traffic uses reserved sentinel DO names (e.g. `_system`, `_oci`) that are deliberately
  distinct from any real tenant id — `worker/src/index.ts:645`.
- Inside the container the ONLY trustworthy tenant source is the DO-injected `x-corelink-tenant-id` header;
  the `AuthTenant` extractor reads it and trims it — `crates/corelink-container/src/auth_tenant.rs:59-65`.
- The reserved-sentinel rejection is now a SHARED source-of-truth `pub fn is_reserved_sentinel`, reused
  verbatim by every cache surface (the `AuthTenant` extractor here AND the Bazel REAPI
  `caller_tenant`/`BazelPutGuard`) so the reserved set (`_oci`, `_public`,
  `_anonymous`/`_unknown`/`_system`/`_pending`/`""`) stays in lock-step instead of drifting per-surface —
  `crates/corelink-container/src/auth_tenant.rs:53-55`.
- Storage keys are namespaced by a per-tenant prefix derived via HMAC-SHA256 over the tenant UUID,
  base64url-encoded and truncated to `TENANT_PREFIX_LEN = 16` ASCII chars by `derive_prefix`
  (`crates/tenant-path/src/prefix.rs:148-166`; `crates/tenant-path/src/prefix.rs:15`).
- `TenantCtx::new` takes `(tdk, tenant_id, region)` and derives the prefix internally, so `ctx.prefix()` is
  always consistent with `ctx.tenant_id()` by construction — `crates/corelink-worker/src/tenant.rs:55-63`.

# Invariants

- No code path may construct a tenant key from a caller-supplied prefix; the prefix field is private and
  only the deriving constructor can populate it (`crates/corelink-worker/src/tenant.rs:36-44`).
- The container fails CLOSED with `401` when the tenant header is empty or a reserved sentinel — and the
  reserved-sentinel set now also includes the adapter prefixes `_oci` (the synthetic shared OCI
  Durable-Object id) and `_public` (`PUBLIC_NAMESPACE`, the cross-tenant dedup namespace) alongside
  `_anonymous`/`_unknown`/`_system`/`_pending`/`""`, so a request masquerading as a shared-namespace
  prefix is rejected, never treated as a tenant (`crates/corelink-container/src/auth_tenant.rs:35-43`;
  `crates/corelink-container/src/auth_tenant.rs:66-69`).
- The `TenantPrefix` newtype cannot be built from raw bytes outside its crate (its tuple field is private),
  so `derive_prefix` is the single trust boundary for namespacing (`crates/tenant-path/src/prefix.rs:91-92`;
  `crates/tenant-path/src/prefix.rs:148-166`; crate-doc rule `crates/tenant-path/src/lib.rs:27`).

# Gotchas

- The container trusts `x-corelink-tenant-id` *only because* the Worker strips any client-supplied copy
  and re-injects the PAT-resolved value; a request that reaches the container plane with a forged header
  has already had it overwritten upstream — the extractor's job is the fail-closed sentinel check, not
  origin authentication (`crates/corelink-container/src/auth_tenant.rs:1-3`).
- The empty string `""` is in the sentinel list, so a present-but-blank header is rejected exactly like a
  missing one (`crates/corelink-container/src/auth_tenant.rs:42`).

# Citations

1. `worker/src/index.ts:3013` — one DO instance per tenant via `idFromName(resolvedTenantId)` (the implementing call).
2. `worker/src/index.ts:645` — reserved `_system` sentinel DO name for non-tenant traffic (the `_health/container` route return).
3. `crates/corelink-worker/src/tenant.rs:36-44` — the `TenantCtx` struct with a private, derived prefix field.
4. `crates/corelink-worker/src/tenant.rs:55-63` — `TenantCtx::new` derives the prefix from `(tdk, tenant_id)`.
5. `crates/corelink-container/src/auth_tenant.rs:1-3` — the only trustworthy tenant source is the DO-injected header.
6. `crates/corelink-container/src/auth_tenant.rs:35-43` — the sentinel set (incl. `_oci`, `_public`/`PUBLIC_NAMESPACE`, and the empty string).
7. `crates/corelink-container/src/auth_tenant.rs:59-65` — the `AuthTenant` extractor reads + trims the header.
8. `crates/corelink-container/src/auth_tenant.rs:66-69` — fail-CLOSED `401` on empty/sentinel tenant.
8b. `crates/corelink-container/src/auth_tenant.rs:53-55` — shared `is_reserved_sentinel` source-of-truth reused across surfaces.
9. `crates/tenant-path/src/prefix.rs:148-166` — `derive_prefix`: HMAC-SHA256 derivation + 16-char truncation.
9b. `crates/tenant-path/src/prefix.rs:15` — `TENANT_PREFIX_LEN = 16`.
10. `crates/tenant-path/src/prefix.rs:91-92` — the `TenantPrefix` private-tuple newtype (single derivation trust boundary).
10b. `crates/tenant-path/src/lib.rs:27` — crate-doc public contract: do not construct `TenantPrefix` from raw bytes outside this crate.
