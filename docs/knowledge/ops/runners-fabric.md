---
type: "Runbook"
title: "Runners fabric (introspect-gated compute)"
description: "How the corelink-runners compute fabric authorizes placement: the dedicated-key-gated POST /internal/v1/auth/introspect endpoint that resolves tenant + plan + a SEPARATE runners-entitlement axis (max_concurrency / max_vcpu_h), fail-CLOSED on any D1 fault."
source_files:
  - "crates/corelink-container/src/routes/auth_introspect.rs"
  - "docs/launch/2026-06-18-sota-tooling-roadmap.md"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["ops", "runners", "introspect", "entitlement", "fabric", "runbook"]
timestamp: "2026-06-26T00:00:00Z"
---

# Runners fabric (introspect-gated compute)

The runners fabric is CoreLink's ephemeral build-compute plane, and it never trusts a presented PAT
directly: before placing a job it calls back into the container's **introspection endpoint**
(`POST /internal/v1/auth/introspect`) to resolve the owning tenant, the tenant's plan, and — critically —
the tenant's **runner entitlement**, which is a SEPARATE entitlement axis from the cache tier (read from a
dedicated `runners_entitlement` D1 table, not derived from the cache-plan ladder). The endpoint is gated
by its own dedicated secret (`FABRIC_INTROSPECT_AUTH_KEY`), is constant-time and consumer-anonymous, and
fails CLOSED (503) on any D1 fault so the fabric never places a job against a guessed plan or cap. This is
the operational runbook for that gate; its authz mechanism is documented as the
[introspection endpoint](/auth/introspect.md) and its request path as the
[introspection flow](/flows/introspection-fabric.md).

# Role
- The fabric's authorization oracle: turns a PAT into (tenant_id, plan, max_concurrency, max_vcpu_h).
- The entitlement boundary: runner capacity is a decoupled axis, so cache tier and runner caps move
  independently (`crates/corelink-container/src/routes/auth_introspect.rs:318`).
- The fail-closed seam: a backend fault yields 503 and never a serveable plan/cap.

# How it works
1. The endpoint authenticates against EVERY configured consumer key with a constant-time compare
   OR-combined without short-circuit, so response time reveals neither validity nor WHICH consumer matched
   (`crates/corelink-container/src/routes/auth_introspect.rs:566-576`).
2. The request body is parsed ONLY after the auth gate passes; the token is never logged
   (`crates/corelink-container/src/routes/auth_introspect.rs:578-590`).
3. The PAT is verified (HMAC + D1 liveness + Argon2id + scope) before any tenant resolution
   (`crates/corelink-container/src/routes/auth_introspect.rs:592-593`).
4. The plan (cache tier wire string) is resolved; a tier-query fault fails CLOSED with 503
   (`crates/corelink-container/src/routes/auth_introspect.rs:597-598`).
5. The runner entitlement is read in one keyed lookup from `runners_entitlement` (migrations 0070+0072),
   returning both the concurrency cap and the monthly vCPU-hour ceiling
   (`crates/corelink-container/src/routes/auth_introspect.rs:353`).
6. The SQL is a single keyed select `SELECT max_concurrency, max_vcpu_h FROM runners_entitlement WHERE
   tenant_id = ?1` (`crates/corelink-container/src/routes/auth_introspect.rs:318`).
7. A valid 200 carries `max_concurrency` only when the tenant has a row and `max_vcpu_h` only when that
   NULLABLE column is set; both are `Option<u32>` with skip-serialization
   (`crates/corelink-container/src/routes/auth_introspect.rs:246`, `crates/corelink-container/src/routes/auth_introspect.rs:257`).
8. The route is only mounted when `FABRIC_INTROSPECT_AUTH_KEY` is present and ≥32 chars; otherwise it
   returns `None` and is not mounted (fail-CLOSED) (`crates/corelink-container/src/routes/auth_introspect.rs:660-663`).
9. The customer build-compute fabric (corelink-runners) runs on **Cloudflare Containers** (Firecracker
   microVMs in the SAME CF account as the R2 CAS — the proximity advantage); it was NEVER on the shared
   founder Mac. Do NOT conflate it with the Blacksmith/"move CI off the shared Mac" roadmap item — that
   line is about CoreLink's OWN self-hosted **CI** runners (the founder's Mac), a different thing from the
   customer runner fabric (`docs/launch/2026-06-18-sota-tooling-roadmap.md:40-56` is the CI roadmap, not
   the fabric's compute).

# Invariants
- Any D1 fault during plan OR entitlement resolution fails CLOSED with 503 — the fabric maps 503 to
  unreachable and never serves a wrong/guessed plan or cap
  (`crates/corelink-container/src/routes/auth_introspect.rs:617-637`).
- The runner cap is NOT derived from the cache plan: an absent `runners_entitlement` row means no Runners
  entitlement and the fabric rejects the placement (empty table = no cap = reject)
  (`crates/corelink-container/src/routes/auth_introspect.rs:318`).
- An invalid PAT returns a uniform `valid:false` with no tenant_id and no reason — no existence/plan oracle
  (`crates/corelink-container/src/routes/auth_introspect.rs:630-631`).
- The dedicated fabric secret MUST be ≥32 chars or the route is not mounted at all
  (`crates/corelink-container/src/routes/auth_introspect.rs:660-663`).

# Gotchas
- `FABRIC_INTROSPECT_AUTH_KEY` is a DEDICATED secret, NOT `CORELINK_INTERNAL_AUTH_KEY` — a leak of the mint
  secret cannot introspect (the dedicated key is read in `build_state_from_env`,
  `crates/corelink-container/src/routes/auth_introspect.rs:661`), and an optional `_HUGR` second consumer
  key is supported additively (`crates/corelink-container/src/routes/auth_introspect.rs:691-700`).
- `max_concurrency` and `max_vcpu_h` are asymmetric: an absent cap ⇒ reject placement, but an absent
  vCPU-h ⇒ wall-off — they are not interchangeable signals
  (`crates/corelink-container/src/routes/auth_introspect.rs:280-298`).
- The `plan` field is informational only for the runners decision — capacity comes from the entitlement
  table, so do not authorize a runner off the cache tier
  (`crates/corelink-container/src/routes/auth_introspect.rs:606-616`).

# Citations
1. `crates/corelink-container/src/routes/auth_introspect.rs:246` — `max_concurrency: Option<u32>` wire field.
2. `crates/corelink-container/src/routes/auth_introspect.rs:257` — `max_vcpu_h: Option<u32>` wire field.
3. `crates/corelink-container/src/routes/auth_introspect.rs:280-298` — `valid()` constructor: cap=reject, vcpu_h=wall-off asymmetry.
4. `crates/corelink-container/src/routes/auth_introspect.rs:318` — the `runners_entitlement` keyed SQL (separate axis).
5. `crates/corelink-container/src/routes/auth_introspect.rs:353` — `runner_concurrency_for_tenant` one-lookup resolver.
6. `crates/corelink-container/src/routes/auth_introspect.rs:566-576` — constant-time OR multi-key auth gate.
7. `crates/corelink-container/src/routes/auth_introspect.rs:661` — dedicated `FABRIC_INTROSPECT_AUTH_KEY` read; `crates/corelink-container/src/routes/auth_introspect.rs:691-700` — optional `_HUGR` additive consumer key.
8. `crates/corelink-container/src/routes/auth_introspect.rs:578-590` — body parsed only after the gate; token never logged.
9. `crates/corelink-container/src/routes/auth_introspect.rs:592-593` — PAT verify (HMAC+D1+Argon2id+scope).
10. `crates/corelink-container/src/routes/auth_introspect.rs:597-598` — plan resolution.
11. `crates/corelink-container/src/routes/auth_introspect.rs:606-616` — 200 valid response assembly (plan informational).
12. `crates/corelink-container/src/routes/auth_introspect.rs:617-637` — fail-CLOSED 503 on entitlement/tier/backend fault.
13. `crates/corelink-container/src/routes/auth_introspect.rs:630-631` — uniform `valid:false`, no oracle.
14. `crates/corelink-container/src/routes/auth_introspect.rs:660-663` — `FABRIC_INTROSPECT_AUTH_KEY` ≥32 or route not mounted.
15. `docs/launch/2026-06-18-sota-tooling-roadmap.md:40-56` — the Blacksmith / "move CI off the shared Mac" roadmap item is about CoreLink's OWN self-hosted CI runners (the founder's Mac), NOT the customer runner fabric (which runs on CF Containers).
