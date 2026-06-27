---
type: "RequestFlow"
title: "Introspection flow (runners fabric)"
description: "How the corelink-runners fabric resolves a PAT to a tenant, a cache plan, and a separate-axis runner entitlement via the dedicated-secret-gated /internal/v1/auth/introspect endpoint."
source_files:
  - "crates/corelink-container/src/routes/auth_introspect.rs"
  - "crates/corelink-container/src/adapter_pat.rs"
checkpoint_sha: "41d84e271568cb47df664806fa3dc9798c134249"
provenance: "AUTHORED"
tags: ["flows", "auth", "introspect", "runners", "request-flow"]
timestamp: "2026-06-26T00:00:00Z"
---

# Introspection flow (runners fabric)

The corelink-runners fabric admits a runner placement only after it knows WHO the request belongs to (tenant), WHAT cache plan they hold, and WHETHER they carry a runners entitlement — and it must never guess any of those. So before placing compute, the fabric calls the container's `/internal/v1/auth/introspect`, which does NOT itself own PAT verification — it **delegates to the shared native PAT gauntlet** (`adapter_pat.rs`'s `PatVerifier::verify`) and then layers tenant→plan + runner-entitlement resolution on top. The fabric sends an opaque PAT and gets back a frozen, minimal result. This flow traces that call across the introspection route (`routes/auth_introspect.rs`) and the shared verifier it delegates to (`adapter_pat.rs`); the verification it runs is the [PAT verification gauntlet](/flows/pat-gauntlet.md).

# Role

This endpoint is the runners fabric's authorization oracle. It is the boundary that converts an opaque, HMAC-and-Argon2id PAT into the three facts the fabric needs to admit or reject a placement — tenant, plan, and runner entitlement — while keeping PAT secret material entirely inside the container.

# How it works

1. Mount gating: the route is built only when a DEDICATED `FABRIC_INTROSPECT_AUTH_KEY` (>= 32 chars) is present, plus the PAT verifier and D1 client; otherwise it is NOT mounted (warn, fail-closed) (`crates/corelink-container/src/routes/auth_introspect.rs:660-683`).
2. Caller auth: the presented `X-Corelink-Internal-Auth` header is compared against EVERY configured consumer key with a non-short-circuiting OR of constant-time compares, so timing reveals neither validity nor WHICH consumer matched (`crates/corelink-container/src/routes/auth_introspect.rs:566-576`).
3. Body parse: only after the auth gate passes is the body parsed; an invalid shape is 400 and the token is never logged (`crates/corelink-container/src/routes/auth_introspect.rs:578-590`).
4. PAT verification: `state.verifier.verify(&req.token)` runs the full HMAC + D1 liveness + Argon2id + scope [PAT verification gauntlet](/flows/pat-gauntlet.md), returning the owning tenant (`crates/corelink-container/src/routes/auth_introspect.rs:592-593`; `crates/corelink-container/src/adapter_pat.rs:731-735`).
5. Plan resolution: `tier_for_tenant` resolves the cache plan; a tier-query fault fails closed 503 — never serve a wrong plan (`crates/corelink-container/src/routes/auth_introspect.rs:595-597`; `crates/corelink-container/src/routes/auth_introspect.rs:490-490`).
6. Entitlement resolution: `runner_concurrency_for_tenant` reads the SEPARATE `runners_entitlement` D1 table (migrations 0070 + 0072) in one lookup for both the concurrency cap and the monthly vCPU-h ceiling — NOT derived from the plan (`crates/corelink-container/src/routes/auth_introspect.rs:606-621`; `crates/corelink-container/src/routes/auth_introspect.rs:308-318`).
7. Entitlement decode: an absent `max_concurrency` row means no entitlement (omitted -> fabric rejects the placement) — `decode_runner_cap` (`crates/corelink-container/src/routes/auth_introspect.rs:382-394`); an absent/NULL `max_vcpu_h` is a wall-off (`Ok(None)`), not an error — the executed `if raw.is_null() { return Ok(None) }` in `decode_runner_vcpu_h` (`crates/corelink-container/src/routes/auth_introspect.rs:426-455`).
8. Response: a valid PAT yields 200 `{valid, tenant_id, plan, max_concurrency?, max_vcpu_h?}`; an invalid PAT yields 200 `{valid:false}` with no tenant and no reason; a verifier backend fault is 503 (`crates/corelink-container/src/routes/auth_introspect.rs:594-637`).

# Invariants

- The route is fail-closed at mount: absent or too-short `FABRIC_INTROSPECT_AUTH_KEY` means the endpoint is simply not exposed — `build_state_from_env` returns `None` (`crates/corelink-container/src/routes/auth_introspect.rs:660-683`).
- The caller-auth compare is constant-time and consumer-blind: all configured keys are always evaluated so there is no consumer-identity timing oracle (`crates/corelink-container/src/routes/auth_introspect.rs:566-576`).
- The body is parsed ONLY after the auth gate passes, and the token is never logged (`crates/corelink-container/src/routes/auth_introspect.rs:578-590`).
- Every resolution fault fails closed 503 — a tier or entitlement D1 fault never serves a guessed plan or cap (`crates/corelink-container/src/routes/auth_introspect.rs:595-637`).
- The runner entitlement is a SEPARATE axis from the cache tier, read from `runners_entitlement`, not derived from `plan` (`crates/corelink-container/src/routes/auth_introspect.rs:606-621`).
- An invalid PAT returns a uniform `valid:false` with no tenant id and no reason (no oracle) (`crates/corelink-container/src/routes/auth_introspect.rs:629-631`).

# Gotchas

- `valid:false` is returned with HTTP 200, NOT 401 — the auth failure is on the introspected PAT, not the caller; a 503 is the only signal that the verifier or D1 was unreachable.
- An empty `runners_entitlement` table is the default state; an absent row deliberately means "no runner cap = reject the placement," so a cache-only tenant simply gets `max_concurrency` omitted (`crates/corelink-container/src/routes/auth_introspect.rs:382-394`).
- The dedicated introspect secret is distinct from the Worker<->container, mint, and billing-ingest secrets, so a leak of one cannot introspect — supply additional consumer keys (e.g. the HuGR variant) without widening that blast radius.

# Citations

1. `crates/corelink-container/src/routes/auth_introspect.rs:308-318` — the `runners_entitlement` SQL (separate-axis cap + vCPU-h, one lookup).
2. `crates/corelink-container/src/routes/auth_introspect.rs:382-394` — `decode_runner_cap`: concurrency-cap decode (absent `max_concurrency` -> omitted -> fabric rejects). The `max_vcpu_h` wall-off is a SEPARATE decoder: `decode_runner_vcpu_h`'s `if raw.is_null() { return Ok(None) }` (`crates/corelink-container/src/routes/auth_introspect.rs:426-455`).
3. `crates/corelink-container/src/routes/auth_introspect.rs:490-490` — `tier_for_tenant` plan resolution.
4. `crates/corelink-container/src/routes/auth_introspect.rs:554-639` — `handle_introspect`: constant-time auth, parse, verify, plan + entitlement resolution, response.
5. `crates/corelink-container/src/routes/auth_introspect.rs:660-683` — `build_state_from_env`: dedicated-secret mount gating (fail-closed).
6. `crates/corelink-container/src/adapter_pat.rs:731-735` — `PatVerifier::verify` (the shared verification the route delegates to).
</content>
