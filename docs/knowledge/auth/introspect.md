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
the tier/plan resolve, and the concurrency-cap decode fail-CLOSED on a *fault* (a tier or entitlement
query fault → 503), so the fabric never serves a plan or a cap it could not resolve. An *absent*
concurrency row is NOT a fault and NOT a reject: `decode_runner_cap` returns `Ok(None)` (the
`max_concurrency` field is simply omitted — a cache-only tenant carries no Runners entitlement, and the
FABRIC, not this endpoint, declines to place runner work). And there is ONE deliberate fail-**OPEN**
axis: the metered vCPU-hours cap. An absent `max_vcpu_h` row is
decoded as "wall-off / let the job through", so a tenant with no `max_vcpu_h` runs unbounded vCPU-hours
(see Gotchas). The headline "fail-CLOSED on a fault" holds for tenant/plan/concurrency (a *fault* → 503;
an *absent* concurrency row → `Ok(None)`, no entitlement) but NOT for the vCPU-h cap (absent → wall-off,
fail-OPEN).

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
- Not reachable from the public internet — this is a DEPLOYMENT/TOPOLOGY fact (the route is mounted on the
  internal container listener, reachable only via the DO / fabric forwarder), NOT a code-enforced
  invariant: there is no in-process enforcer for it; `crates/corelink-container/src/routes/auth_introspect.rs:16-18`
  is the module `//!` DOCUMENTING the topology, not an executed gate. (The executed access control on this
  route is the dedicated-key caller gate, below.)
- A `valid: false` response carries no `tenant_id` and no reason (uniform with `VerifyError`)
  (`crates/corelink-container/src/routes/auth_introspect.rs:629-631`).
- Any tier or entitlement resolution fault fails CLOSED with 503 — never a guessed plan or cap
  (`crates/corelink-container/src/routes/auth_introspect.rs:617-637`).

# Gotchas

- `max_concurrency` and `max_vcpu_h` carry a deliberate asymmetry: an absent concurrency row ⇒
  `decode_runner_cap` returns `Ok(None)` — the field is OMITTED (empty table = no cap = no Runners
  entitlement), NOT a hard reject from this endpoint; the fabric then declines to place runner work for a
  cache-only tenant (`crates/corelink-container/src/routes/auth_introspect.rs:382-394`). An absent
  `max_vcpu_h` ⇒ wall-off (let the job through, fail-OPEN) — decoded by the SEPARATE
  `decode_runner_vcpu_h`, whose `if raw.is_null() { return Ok(None) }` (plus the missing-column arm) is the
  executed wall-off (`crates/corelink-container/src/routes/auth_introspect.rs:426-454`, the `is_null` arm
  at `:434-436`). A row that EXISTS but is out-of-contract (non-positive / out of `u32` range) is the only
  hard fail-CLOSED arm here → `Err` → 503.
- `plan` is the cache tier and is informational only for runners — it NEVER feeds the concurrency cap,
  which comes solely from the SEPARATE `runners_entitlement` decode
  (`crates/corelink-container/src/routes/auth_introspect.rs:382-394`).
- The constant-time gate is reused from `internal_pat::internal_auth_ok`, not reinvented here — the
  multi-key caller gate loops it with non-short-circuiting `|=`
  (`crates/corelink-container/src/routes/auth_introspect.rs:566-576`).

# Citations

1. `crates/corelink-container/src/routes/auth_introspect.rs:1-12` — why the endpoint exists (fabric tenant/plan resolution).
2. `crates/corelink-container/src/routes/auth_introspect.rs:16-18` — the module `//!` DOCUMENTING the "not public; container-listener only" deployment/topology fact (no in-process code enforcer for it).
3. `crates/corelink-container/src/routes/auth_introspect.rs:660-683` — `build_state_from_env` mount-gate: dedicated `FABRIC_INTROSPECT_AUTH_KEY` present + ≥32 chars, else `None` (route NOT mounted) — fail-CLOSED.
4. `crates/corelink-container/src/routes/auth_introspect.rs:566-576` — the multi-key caller gate (`|=` over every configured key, reusing the constant-time `internal_auth_ok`).
5. `crates/corelink-container/src/routes/auth_introspect.rs:382-394` — `decode_runner_cap`: the CONCURRENCY (`max_concurrency`) entitlement decode (absent row → `Ok(None)` / field omitted; out-of-contract existing row → 503), separate from `plan`. NOTE: this fn carries NO vCPU logic.
5b. `crates/corelink-container/src/routes/auth_introspect.rs:426-454` — `decode_runner_vcpu_h`: the SEPARATE `max_vcpu_h` decode and the deliberate fail-OPEN wall-off; the executed wall-off is the `if raw.is_null() { return Ok(None) }` arm (+ the missing-column arm) at `:434-436`. A present, non-null, out-of-contract value → `Err` → 503.
6. `crates/corelink-container/src/routes/auth_introspect.rs:382-394` — entitlement decode is the SEPARATE axis; `plan` (cache tier) never feeds the cap.
7. `crates/corelink-container/src/routes/auth_introspect.rs:559-576` — non-short-circuit multi-key caller gate.
8. `crates/corelink-container/src/routes/auth_introspect.rs:592-637` — verify → tier → entitlement → uniform invalid / 503.
