---
type: "RequestFlow"
title: "Introspection flow (runners fabric)"
description: "How the corelink-runners fabric resolves a PAT to a tenant, a cache plan, and a separate-axis runner entitlement via the dedicated-secret-gated /internal/v1/auth/introspect endpoint."
source_files:
  - "crates/corelink-container/src/routes/auth_introspect/part-00.rs"
  - "crates/corelink-container/src/routes/auth_introspect/part-00-01.rs"
  - "crates/corelink-container/src/routes/auth_introspect/part-01.rs"
  - "crates/corelink-container/src/adapter_pat_verifier/part-01.rs"
  - "crates/corelink-container/src/adapter_pat_verifier/part-02.rs"
source_blobs:
  - "crates/corelink-container/src/routes/auth_introspect/part-00.rs@62d7d3db5ba1af3f33aea46919a092e52d228f0b"
  - "crates/corelink-container/src/routes/auth_introspect/part-00-01.rs@0648b938a24681c83a9c0c4af68fa8f2386c6471"
  - "crates/corelink-container/src/routes/auth_introspect/part-01.rs@a48802270fff8530cd9ffa5eb1265529d42def07"
  - "crates/corelink-container/src/adapter_pat_verifier/part-01.rs@85b83a82ad06623908a0bfa3612f7f79e1f18c11"
  - "crates/corelink-container/src/adapter_pat_verifier/part-02.rs@b59252c1e5d8ecd2b3abf6fdf91e52e816d8bf8f"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["flows", "auth", "introspect", "runners", "request-flow"]
timestamp: "2026-06-26T00:00:00Z"
---

# Introspection flow (runners fabric)

The corelink-runners fabric admits a runner placement only after it knows WHO the request belongs to (tenant), WHAT cache plan they hold, and WHETHER they carry a runners entitlement — and it must never guess any of those. So before placing compute, the fabric calls the container's `/internal/v1/auth/introspect`, which is the single place that owns PAT verification: the fabric sends an opaque PAT and gets back a frozen, minimal result. This flow traces that call across the introspection route (`routes/auth_introspect.rs`) and the shared verifier implementation in `adapter_pat_verifier/part-01.rs` and `part-02.rs`; the verification it runs is the [PAT verification gauntlet](/flows/pat-gauntlet.md).

# Role

This endpoint is the runners fabric's authorization oracle. It is the boundary that converts an opaque, HMAC-and-Argon2id PAT into the three facts the fabric needs to admit or reject a placement — tenant, plan, and runner entitlement — while keeping PAT secret material entirely inside the container.

# How it works

1. Mount gating: the route is built only when a DEDICATED `FABRIC_INTROSPECT_AUTH_KEY` (>= 32 chars) is present, plus the PAT verifier and D1 client; otherwise it is NOT mounted (warn, fail-closed) (`crates/corelink-container/src/routes/auth_introspect/part-01.rs:238-267`). The same router also mounts the sibling `POST /internal/v1/auth/resolve-tenant` (`clerk_org_id`→`tenant_id` via `tenant_org_map`, migration 0083) on the SAME state and internal-auth gate (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:86-98`). Per the ratified A1 ordering contract, that resolver stays lookup-only ON PURPOSE — a `404 org_not_mapped` is a TRANSIENT not-yet-provisioned answer (`crates/corelink-container/src/routes/auth_introspect/part-01.rs:22-84`).
2. Caller auth: the presented `X-Corelink-Internal-Auth` header is compared against EVERY configured consumer key with a non-short-circuiting OR of constant-time compares, so timing reveals neither validity nor WHICH consumer matched (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:103-127`).
3. Body parse: only after the auth gate passes is the body parsed; an invalid shape is 400 and the token is never logged (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:127-142`).
4. PAT verification: `state.verifier.verify(&req.token)` runs the full HMAC + D1 liveness + Argon2id + scope [PAT verification gauntlet](/flows/pat-gauntlet.md), returning the owning tenant (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:142-153`; `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-37`).
5. Plan resolution: `tier_for_tenant` resolves the cache plan; a tier-query fault fails closed 503 — never serve a wrong plan (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:149-195`; `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:35-58`).
6. Entitlement resolution: `runner_concurrency_for_tenant` reads the SEPARATE `runners_entitlement` D1 table (migrations 0070 + 0072) in one lookup for both the concurrency cap and the monthly vCPU-h ceiling — NOT derived from the plan (`crates/corelink-container/src/routes/auth_introspect/part-00.rs:356-373`; `crates/corelink-container/src/routes/auth_introspect/part-00.rs:356-373`).
6a. **Steps 5 and 6 are numbered for readability, NOT for ordering — they run CONCURRENTLY.** They are independent D1 reads over different tables and neither consumes the other's result, so the handler issues both under one `futures::try_join!`. From the container these are D1-over-HTTP (~80-100 ms each, measured), which is the dominant cost of this handler, so overlapping them halves the success path. Fail-CLOSED is unchanged — either fault still yields 503, and the two error arms keep distinct log lines so an operator can tell which table faulted. The stated cost: with a join, BOTH queries are issued even when one is going to fault, where the previous nested form short-circuited — one extra D1 read on the ERROR path, in exchange for halving the SUCCESS path (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:142-195`).
7. Entitlement decode: an absent `max_concurrency` row means no entitlement (omitted -> fabric rejects the placement); an absent `max_vcpu_h` is a wall-off, not an error (`crates/corelink-container/src/routes/auth_introspect/part-00.rs:384-430`).
8. Response: a valid PAT yields 200 `{valid, tenant_id, plan, max_concurrency?, max_vcpu_h?}`; an invalid PAT yields 200 `{valid:false}` with no tenant and no reason; a verifier backend fault OR an Argon2id permit-pool shed is 503, which the fabric maps to `Err(Unreachable)` (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:142-226`; `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:209-226`).

# Invariants

- The route is fail-closed at mount: absent or too-short `FABRIC_INTROSPECT_AUTH_KEY` means the endpoint is simply not exposed (`crates/corelink-container/src/routes/auth_introspect/part-01.rs:238-267`).
- The caller-auth compare is constant-time and consumer-blind: all configured keys are always evaluated so there is no consumer-identity timing oracle (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:103-127`).
- The body is parsed ONLY after the auth gate passes, and the token is never logged (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:127-142`).
- Every resolution fault fails closed 503 — a tier or entitlement D1 fault never serves a guessed plan or cap (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:149-226`).
- The runner entitlement is a SEPARATE axis from the cache tier, read from `runners_entitlement`, not derived from `plan` (`crates/corelink-container/src/routes/auth_introspect/part-00.rs:356-373`).
- An invalid PAT returns a uniform `valid:false` with no tenant id and no reason (no oracle) (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:209-226`).

# Gotchas

- `valid:false` is returned with HTTP 200, NOT 401 — the auth failure is on the introspected PAT, not the caller; a 503 is the only signal that the verifier or D1 was unreachable.
- ⚠️ The 200/503 split follows the `VerifyError` VARIANT, so it inherits the container's uniform shed (`INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`): a saturated verifier answers 503 even for an UNKNOWN token, where it used to answer `200 {valid:false}`. Accepted deliberately — the verifier reached no verdict, and special-casing it back would rebuild the row-existence oracle. This route is internal-auth gated, so it is not an external surface (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:209-226`).
- An empty `runners_entitlement` table is the default state; an absent row deliberately means "no runner cap = reject the placement," so a cache-only tenant simply gets `max_concurrency` omitted (`crates/corelink-container/src/routes/auth_introspect/part-00.rs:384-430`).
- The dedicated introspect secret is distinct from the Worker<->container, mint, and billing-ingest secrets, so a leak of one cannot introspect — supply additional consumer keys (e.g. the HuGR variant) without widening that blast radius.

# Citations

1. `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:86-226` — router, internal-auth-first request parsing, verification, concurrent plan and entitlement resolution, and uniform response outcomes.
2. `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:35-58` — cache tier lookup and conservative fallback; `:149-195` — query joins and backend faults.
3. `crates/corelink-container/src/routes/auth_introspect/part-01.rs:22-84` — organization resolver auth gate and lookup-only 404/503 behavior; `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:287-300` — its exact D1 lookup.
4. `crates/corelink-container/src/routes/auth_introspect/part-01.rs:140-215` — installation-to-tenant read helper and decoder.
5. `crates/corelink-container/src/routes/auth_introspect/part-01.rs:238-267` — the dedicated fabric auth key and fail-closed route mount.
6. `crates/corelink-container/src/routes/auth_introspect/part-00.rs:356-373` — runner entitlement query; `:384-430` — decode behavior for absent caps.
7. `crates/corelink-container/src/adapter_pat_verifier/part-01.rs:20-28` — route-facing verifier wrapper; `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-37` — HMAC and D1 start of the shared verification pipeline.
