---
type: "AuthMechanism"
title: "Introspection endpoint (runners fabric authz)"
description: "The internal POST /internal/v1/auth/introspect endpoint that resolves a PAT to its tenant, plan, and runner entitlement for the compute fabric."
source_files:
  - "crates/corelink-container/src/routes/auth_introspect.rs"
checkpoint_sha: "41d84e271568cb47df664806fa3dc9798c134249"
provenance: "AUTHORED"
tags: ["auth", "pat", "introspect", "runners", "fabric"]
timestamp: "2026-06-26T00:00:00Z"
---

# Introspection endpoint (runners fabric authz)

The corelink-runners fabric is handed an inbound `Bearer` PAT per job and must resolve the owning tenant
and the tenant's plan before it places work on a runner. Rather than re-implement PAT verification (HMAC
+ Argon2id + D1 liveness) in the fabric, the fabric calls `POST /internal/v1/auth/introspect`, which
reuses the container's one full verification pipeline and returns a frozen, minimal result. The endpoint
is the authorization seam between the cache product and the runners entitlement axis. PAT verification,
the tier/plan resolve, and the concurrency-cap decode all fail-CLOSED (a fault → 503, an empty
concurrency row → reject), so the fabric never serves a plan or a concurrency cap it could not resolve —
with ONE deliberate exception: the metered vCPU-hours axis fails **OPEN**. An absent `max_vcpu_h` row is
decoded as "wall-off / let the job through", so a tenant with no `max_vcpu_h` runs unbounded vCPU-hours
(see Gotchas). The headline "fail-CLOSED at every step" holds for tenant/plan/concurrency but NOT for the
vCPU-h cap.

# Role

Introspection is the runners-side consumer of [the 2-level PAT moat](/auth/pat-moat.md): it delegates to
the shared verifier ([the Argon2id verify](/auth/argon2id-verify.md)) and then layers two D1 reads — the
cache tier and the SEPARATE runner entitlement. It is an internal control surface, gated like the mint
route but with its OWN dedicated secret so the two blast radii stay disjoint.

# How it works

- The route is mounted only when its dedicated secret is present and ≥32 chars; absent/short →
  `build_state_from_env` returns `None` and the route is NOT mounted (fail-CLOSED)
  (`crates/corelink-container/src/routes/auth_introspect.rs:660-683`).
- The caller gate checks the `X-Corelink-Internal-Auth` header against EVERY configured consumer key
  with non-short-circuiting `|=`, so timing reveals no consumer identity; no match → 401
  (`crates/corelink-container/src/routes/auth_introspect.rs:559-576`).
- Only after the gate passes is the body parsed and the PAT verified via the shared verifier
  (`crates/corelink-container/src/routes/auth_introspect.rs:592-594`).
- On a valid PAT the tenant's effective plan is resolved from D1; a tier-query fault → 503
  (`crates/corelink-container/src/routes/auth_introspect.rs:595-597`).
- The runner entitlement (`max_concurrency`, `max_vcpu_h`) is a SEPARATE D1 lookup against
  `runners_entitlement`, NOT derived from the plan; a D1 fault → 503
  (`crates/corelink-container/src/routes/auth_introspect.rs:598-621`).
- An invalid PAT returns a uniform `200 {"valid": false}` with no tenant_id and no reason; a genuine
  backend fault returns 503 (`crates/corelink-container/src/routes/auth_introspect.rs:629-637`).

# Invariants

- The route uses a DEDICATED `FABRIC_INTROSPECT_AUTH_KEY`, never the Worker↔container mint secret, so a
  fabric-key leak cannot mint and a mint-key leak cannot introspect — `build_state_from_env` reads that
  dedicated key and gates the mount on it (`crates/corelink-container/src/routes/auth_introspect.rs:660-683`).
- Not reachable from the public internet — mounted on the container listener, reached only via the DO /
  fabric forwarder (`crates/corelink-container/src/routes/auth_introspect.rs:16-18`).
- A `valid: false` response carries no `tenant_id` and no reason (uniform with `VerifyError`)
  (`crates/corelink-container/src/routes/auth_introspect.rs:629-631`).
- Any tier or entitlement resolution fault fails CLOSED with 503 — never a guessed plan or cap
  (`crates/corelink-container/src/routes/auth_introspect.rs:617-637`).

# Gotchas

- `max_concurrency` and `max_vcpu_h` carry a deliberate asymmetry: an absent concurrency row ⇒ reject
  the placement (empty table = no cap = no Runners entitlement), decoded by `decode_runner_cap`
  (`crates/corelink-container/src/routes/auth_introspect.rs:382-394`); an absent `max_vcpu_h` ⇒ wall-off
  (let the job through).
- `plan` is the cache tier and is informational only for runners — it NEVER feeds the concurrency cap,
  which comes solely from the SEPARATE `runners_entitlement` decode
  (`crates/corelink-container/src/routes/auth_introspect.rs:382-394`).
- The constant-time gate is reused from `internal_pat::internal_auth_ok`, not reinvented here — the
  multi-key caller gate loops it with non-short-circuiting `|=`
  (`crates/corelink-container/src/routes/auth_introspect.rs:566-576`).

# Citations

1. `crates/corelink-container/src/routes/auth_introspect.rs:1-12` — why the endpoint exists (fabric tenant/plan resolution).
2. `crates/corelink-container/src/routes/auth_introspect.rs:16-18` — not public; container-listener only.
3. `crates/corelink-container/src/routes/auth_introspect.rs:660-683` — `build_state_from_env` mount-gate: dedicated `FABRIC_INTROSPECT_AUTH_KEY` present + ≥32 chars, else `None` (route NOT mounted) — fail-CLOSED.
4. `crates/corelink-container/src/routes/auth_introspect.rs:566-576` — the multi-key caller gate (`|=` over every configured key, reusing the constant-time `internal_auth_ok`).
5. `crates/corelink-container/src/routes/auth_introspect.rs:382-394` — `decode_runner_cap`: the runner entitlement decode (empty → reject; out-of-contract → 503) — the cap/vCPU asymmetry origin, separate from `plan`.
6. `crates/corelink-container/src/routes/auth_introspect.rs:382-394` — entitlement decode is the SEPARATE axis; `plan` (cache tier) never feeds the cap.
7. `crates/corelink-container/src/routes/auth_introspect.rs:559-576` — non-short-circuit multi-key caller gate.
8. `crates/corelink-container/src/routes/auth_introspect.rs:592-637` — verify → tier → entitlement → uniform invalid / 503.
