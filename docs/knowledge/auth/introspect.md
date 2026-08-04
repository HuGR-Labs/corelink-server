---
type: "AuthMechanism"
title: "Introspection endpoint (runners fabric authz)"
description: "The internal POST /internal/v1/auth/introspect endpoint that resolves a PAT to its tenant, plan, and runner entitlement for the compute fabric."
source_files:
  - "crates/corelink-container/src/routes/auth_introspect.rs"
checkpoint_sha: "a298cd91a4f0429d6e3d31a64191d2b0e87eced5"
provenance: "AUTHORED"
tags: ["auth", "pat", "introspect", "runners", "fabric"]
timestamp: "2026-06-26T00:00:00Z"
---

# Introspection endpoint (runners fabric authz)

The corelink-runners fabric is handed an inbound `Bearer` PAT per job and must resolve the owning tenant
and the tenant's plan before it places work on a runner. Rather than re-implement PAT verification (HMAC
+ Argon2id + D1 liveness) in the fabric, the fabric calls `POST /internal/v1/auth/introspect`, which
reuses the container's one full verification pipeline and returns a frozen, minimal result. The endpoint
is the authorization seam between the cache product and the runners entitlement axis — and it is
fail-CLOSED at every step, so the fabric never serves a plan it could not resolve.

# Role

Introspection is the runners-side consumer of [the 2-level PAT moat](/auth/pat-moat.md): it delegates to
the shared verifier ([the Argon2id verify](/auth/argon2id-verify.md)) and then layers two D1 reads — the
cache tier and the SEPARATE runner entitlement. It is an internal control surface, gated like the mint
route but with its OWN dedicated secret so the two blast radii stay disjoint.

# How it works

- The route is mounted only when its dedicated secret is present and ≥32 chars; absent/short →
  `build_state_from_env` reads `FABRIC_INTROSPECT_AUTH_KEY`, checks `len() < MIN_FABRIC_AUTH_KEY_LEN`,
  warn-logs and returns `None`, so the route is NOT mounted (fail-CLOSED)
  (`crates/corelink-container/src/routes/auth_introspect.rs:975-983`).
- The caller gate checks the `X-Corelink-Internal-Auth` header against EVERY configured consumer key
  with non-short-circuiting `|=`, so timing reveals no consumer identity; no match → 401
  (`crates/corelink-container/src/routes/auth_introspect.rs:570-580`).
- Only after the gate passes is the body parsed and the PAT verified via the shared verifier
  (`crates/corelink-container/src/routes/auth_introspect.rs:582-597`).
- On a valid PAT the tenant's effective plan is resolved from D1; a tier-query fault → 503
  (`crates/corelink-container/src/routes/auth_introspect.rs:601-630`).
- The runner entitlement (`max_concurrency`, `max_vcpu_h`) is a SEPARATE D1 lookup against
  `runners_entitlement`, NOT derived from the plan; a D1 fault → 503
  (`crates/corelink-container/src/routes/auth_introspect.rs:610-625`).
- An invalid PAT returns a uniform `200 {"valid": false}` with no tenant_id and no reason; a genuine
  backend fault — and an Argon2id permit-pool SHED, which is also `VerifyError::Backend` — returns 503
  (`crates/corelink-container/src/routes/auth_introspect.rs:634-656`).
- ⚠️ Because this route classifies by VARIANT, the shed's symmetry
  (`INV-AUTH-PAT-OVERLOAD-SHED-UNIFORM`) reclassifies one narrow case here: a saturated verifier
  presented with an UNKNOWN token answers 503 "unreachable" rather than `200 {"valid": false}`. That
  is the honest answer — under saturation the verifier never judged the credential — and it is
  deliberately NOT special-cased, because re-splitting the two shed arms by row existence at this
  layer would rebuild the row-existence oracle the invariant closes
  (`crates/corelink-container/src/routes/auth_introspect.rs:655`).
- A sibling endpoint `POST /internal/v1/auth/resolve-tenant` is mounted on the SAME router and state
  (`crates/corelink-container/src/routes/auth_introspect.rs:544-547`): it resolves a Clerk `clerk_org_id`
  → isolated CoreLink `tenant_id` via the `tenant_org_map` table (migration 0083), behind the IDENTICAL
  multi-key internal-auth gate (parsed only after the gate passes). A mapped org → `200 {tenant_id}`; an
  unmapped org → `404 {"org_not_mapped"}` (lookup-only — it NEVER auto-provisions); a D1 fault → 503
  (fail-CLOSED, never a guessed tenant) (`crates/corelink-container/src/routes/auth_introspect.rs:775-825`).
  Per the ratified A1 ordering contract (Option 2), provisioning is the SOLE `tenant_org_map` writer and
  this handler NEVER writes, so a `404 org_not_mapped` is a TRANSIENT "not-yet-provisioned" answer during
  Svix webhook-delivery lag — consumers must treat it as retryable
  (`crates/corelink-container/src/routes/auth_introspect.rs:814-818`).

# Invariants

- The route uses a DEDICATED `FABRIC_INTROSPECT_AUTH_KEY`, never the Worker↔container mint secret, so a
  fabric-key leak cannot mint and a mint-key leak cannot introspect
  (`crates/corelink-container/src/routes/auth_introspect.rs:19-30`).
- Not reachable from the public internet — mounted on the container listener, reached only via the DO /
  fabric forwarder (`crates/corelink-container/src/routes/auth_introspect.rs:16-18`).
- A `valid: false` response carries no `tenant_id` and no reason (uniform with `VerifyError`)
  (`crates/corelink-container/src/routes/auth_introspect.rs:634-636`).
- Any tier or entitlement resolution fault fails CLOSED with 503 — never a guessed plan or cap
  (`crates/corelink-container/src/routes/auth_introspect.rs:610-656`).

# Gotchas

- `max_concurrency` and `max_vcpu_h` carry a deliberate asymmetry: an absent concurrency row ⇒ reject
  the placement (empty table = no cap = no Runners entitlement), but an absent `max_vcpu_h` ⇒ wall-off
  (let the job through) (`crates/corelink-container/src/routes/auth_introspect.rs:68-92`).
- `plan` is the cache tier and is informational only for runners — it NEVER feeds the concurrency cap
  (`crates/corelink-container/src/routes/auth_introspect.rs:74-83`).
- The constant-time gate is reused from `internal_pat::internal_auth_ok`, not reinvented here
  (`crates/corelink-container/src/routes/auth_introspect.rs:24-27`).

# Citations

1. `crates/corelink-container/src/routes/auth_introspect.rs:1-12` — why the endpoint exists (fabric tenant/plan resolution).
2. `crates/corelink-container/src/routes/auth_introspect.rs:16-18` — not public; container-listener only.
3. `crates/corelink-container/src/routes/auth_introspect.rs:975-983` — `build_state_from_env`: reads the dedicated `FABRIC_INTROSPECT_AUTH_KEY`, enforces `len() < MIN_FABRIC_AUTH_KEY_LEN`, returns `None` (route NOT mounted) when absent/short — fail-CLOSED.
4. `crates/corelink-container/src/routes/auth_introspect.rs:24-27` — reuses the constant-time `internal_auth_ok` gate.
5. `crates/corelink-container/src/routes/auth_introspect.rs:68-92` — runner entitlement axis + the cap/vCPU asymmetry.
6. `crates/corelink-container/src/routes/auth_introspect.rs:74-83` — entitlement is separate from `plan`.
7. `crates/corelink-container/src/routes/auth_introspect.rs:570-580` — non-short-circuit multi-key caller gate.
8. `crates/corelink-container/src/routes/auth_introspect.rs:597-657` — verify → tier → entitlement → uniform invalid / 503.
9. `crates/corelink-container/src/routes/auth_introspect.rs:775-825` — sibling `POST /internal/v1/auth/resolve-tenant` handler (same gate; `clerk_org_id`→`tenant_id` via `tenant_org_map`; 404 unmapped — TRANSIENT/retryable per the A1 ordering contract, 503 fail-CLOSED).
