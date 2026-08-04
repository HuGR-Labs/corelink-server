---
type: "RequestFlow"
title: "Introspection flow (runners fabric)"
description: "How the corelink-runners fabric resolves a PAT to a tenant, a cache plan, and a separate-axis runner entitlement via the dedicated-secret-gated /internal/v1/auth/introspect endpoint."
source_files:
  - "crates/corelink-container/src/routes/auth_introspect.rs"
  - "crates/corelink-container/src/adapter_pat.rs"
checkpoint_sha: "73d55138b8432c820aec34eb244ff50b0a6ce048"
provenance: "AUTHORED"
tags: ["flows", "auth", "introspect", "runners", "request-flow"]
timestamp: "2026-06-26T00:00:00Z"
---

# Introspection flow (runners fabric)

The corelink-runners fabric admits a runner placement only after it knows WHO the request belongs to (tenant), WHAT cache plan they hold, and WHETHER they carry a runners entitlement — and it must never guess any of those. So before placing compute, the fabric calls the container's `/internal/v1/auth/introspect`, which is the single place that owns PAT verification: the fabric sends an opaque PAT and gets back a frozen, minimal result. This flow traces that call across the introspection route (`routes/auth_introspect.rs`) and the shared verifier it delegates to (`adapter_pat.rs`); the verification it runs is the [PAT verification gauntlet](/flows/pat-gauntlet.md).

# Role

This endpoint is the runners fabric's authorization oracle. It is the boundary that converts an opaque, HMAC-and-Argon2id PAT into the three facts the fabric needs to admit or reject a placement — tenant, plan, and runner entitlement — while keeping PAT secret material entirely inside the container.

# How it works

1. Mount gating: the route is built only when a DEDICATED `FABRIC_INTROSPECT_AUTH_KEY` (>= 32 chars) is present, plus the PAT verifier and D1 client; otherwise it is NOT mounted (warn, fail-closed) (`crates/corelink-container/src/routes/auth_introspect.rs:960-1003`). The same router also mounts the sibling `POST /internal/v1/auth/resolve-tenant` (`clerk_org_id`→`tenant_id` via `tenant_org_map`, migration 0083) on the SAME state and internal-auth gate (`crates/corelink-container/src/routes/auth_introspect.rs:544-547`). Per the ratified A1 ordering contract, that resolver stays lookup-only ON PURPOSE — a `404 org_not_mapped` is a TRANSIENT not-yet-provisioned answer (`crates/corelink-container/src/routes/auth_introspect.rs:799-803`).
2. Caller auth: the presented `X-Corelink-Internal-Auth` header is compared against EVERY configured consumer key with a non-short-circuiting OR of constant-time compares, so timing reveals neither validity nor WHICH consumer matched (`crates/corelink-container/src/routes/auth_introspect.rs:570-580`).
3. Body parse: only after the auth gate passes is the body parsed; an invalid shape is 400 and the token is never logged (`crates/corelink-container/src/routes/auth_introspect.rs:582-594`).
4. PAT verification: `state.verifier.verify(&req.token)` runs the full HMAC + D1 liveness + Argon2id + scope [PAT verification gauntlet](/flows/pat-gauntlet.md), returning the owning tenant (`crates/corelink-container/src/routes/auth_introspect.rs:596-597`; `crates/corelink-container/src/adapter_pat.rs:1199-1203`).
5. Plan resolution: `tier_for_tenant` resolves the cache plan; a tier-query fault fails closed 503 — never serve a wrong plan (`crates/corelink-container/src/routes/auth_introspect.rs:601-602`; `crates/corelink-container/src/routes/auth_introspect.rs:490-490`).
6. Entitlement resolution: `runner_concurrency_for_tenant` reads the SEPARATE `runners_entitlement` D1 table (migrations 0070 + 0072) in one lookup for both the concurrency cap and the monthly vCPU-h ceiling — NOT derived from the plan (`crates/corelink-container/src/routes/auth_introspect.rs:610-625`; `crates/corelink-container/src/routes/auth_introspect.rs:308-318`).
7. Entitlement decode: an absent `max_concurrency` row means no entitlement (omitted -> fabric rejects the placement); an absent `max_vcpu_h` is a wall-off, not an error (`crates/corelink-container/src/routes/auth_introspect.rs:382-394`).
8. Response: a valid PAT yields 200 `{valid, tenant_id, plan, max_concurrency?, max_vcpu_h?}`; an invalid PAT yields 200 `{valid:false}` with no tenant and no reason; a verifier backend fault is 503 (`crates/corelink-container/src/routes/auth_introspect.rs:597-642`).

# Invariants

- The route is fail-closed at mount: absent or too-short `FABRIC_INTROSPECT_AUTH_KEY` means the endpoint is simply not exposed (`crates/corelink-container/src/routes/auth_introspect.rs:960-968`).
- The caller-auth compare is constant-time and consumer-blind: all configured keys are always evaluated so there is no consumer-identity timing oracle (`crates/corelink-container/src/routes/auth_introspect.rs:570-580`).
- The body is parsed ONLY after the auth gate passes, and the token is never logged (`crates/corelink-container/src/routes/auth_introspect.rs:582-594`).
- Every resolution fault fails closed 503 — a tier or entitlement D1 fault never serves a guessed plan or cap (`crates/corelink-container/src/routes/auth_introspect.rs:601-642`).
- The runner entitlement is a SEPARATE axis from the cache tier, read from `runners_entitlement`, not derived from `plan` (`crates/corelink-container/src/routes/auth_introspect.rs:610-625`).
- An invalid PAT returns a uniform `valid:false` with no tenant id and no reason (no oracle) (`crates/corelink-container/src/routes/auth_introspect.rs:634-636`).

# Gotchas

- `valid:false` is returned with HTTP 200, NOT 401 — the auth failure is on the introspected PAT, not the caller; a 503 is the only signal that the verifier or D1 was unreachable.
- An empty `runners_entitlement` table is the default state; an absent row deliberately means "no runner cap = reject the placement," so a cache-only tenant simply gets `max_concurrency` omitted (`crates/corelink-container/src/routes/auth_introspect.rs:382-394`).
- The dedicated introspect secret is distinct from the Worker<->container, mint, and billing-ingest secrets, so a leak of one cannot introspect — supply additional consumer keys (e.g. the HuGR variant) without widening that blast radius.

# Citations

1. `crates/corelink-container/src/routes/auth_introspect.rs:308-318` — the `runners_entitlement` SQL (separate-axis cap + vCPU-h, one lookup).
2. `crates/corelink-container/src/routes/auth_introspect.rs:382-394` — entitlement decode (absent cap -> reject).
3. `crates/corelink-container/src/routes/auth_introspect.rs:490-490` — `tier_for_tenant` plan resolution.
4. `crates/corelink-container/src/routes/auth_introspect.rs:558-643` — `handle_introspect`: constant-time auth, parse, verify, plan + entitlement resolution, response.
5. `crates/corelink-container/src/routes/auth_introspect.rs:960-1003` — `build_state_from_env`: dedicated-secret mount gating (fail-closed).
6. `crates/corelink-container/src/adapter_pat.rs:1199-1203` — `PatVerifier::verify` (the shared verification the route delegates to).
</content>
