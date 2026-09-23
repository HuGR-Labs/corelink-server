---
type: "Runbook"
title: "Runners fabric (introspect-gated compute)"
description: "How the corelink-runners compute fabric authorizes placement: the dedicated-key-gated POST /internal/v1/auth/introspect endpoint that resolves tenant + plan + a SEPARATE runners-entitlement axis (max_concurrency / max_vcpu_h), fail-CLOSED on any D1 fault."
source_files:
  - "crates/corelink-container/src/routes/auth_introspect/part-00.rs"
  - "crates/corelink-container/src/routes/auth_introspect/part-00-01.rs"
  - "crates/corelink-container/src/routes/auth_introspect/part-01.rs"
  - "crates/corelink-container/src/routes/auth_introspect/part-01.rs"
  - "docs/launch/2026-06-18-sota-tooling-roadmap.md"
source_blobs:
  - "crates/corelink-container/src/routes/auth_introspect/part-00.rs@62d7d3db5ba1af3f33aea46919a092e52d228f0b"
  - "crates/corelink-container/src/routes/auth_introspect/part-00-01.rs@0648b938a24681c83a9c0c4af68fa8f2386c6471"
  - "crates/corelink-container/src/routes/auth_introspect/part-01.rs@a48802270fff8530cd9ffa5eb1265529d42def07"
  - "crates/corelink-container/src/routes/auth_introspect/part-01.rs@a48802270fff8530cd9ffa5eb1265529d42def07"
  - "docs/launch/2026-06-18-sota-tooling-roadmap.md@8a6010944382f85754c5eb9dbff995a0e3189f26"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
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
by its own dedicated secret (`FABRIC_INTROSPECT_AUTH_KEY`) and fails CLOSED (503) on any D1 fault so the
fabric never places a job against a guessed plan or cap. The handler's constant-time gate can compare each
configured key without short-circuiting, but the production state builder installs only the dedicated key;
it explicitly refuses a secondary consumer key for the isolation-sensitive tenant lookup. This is
the operational runbook for that gate; its authz mechanism is documented as the
[introspection endpoint](/auth/introspect.md) and its request path as the
[introspection flow](/flows/introspection-fabric.md).

# Role
- The fabric's authorization oracle: turns a PAT into (tenant_id, plan, max_concurrency, max_vcpu_h).
- The entitlement boundary: runner capacity is a decoupled axis, so cache tier and runner caps move
  independently (`crates/corelink-container/src/routes/auth_introspect/part-00.rs:320-321`).
- The fail-closed seam: a backend fault yields 503 and never a serveable plan/cap.

# How it works
1. The handler checks each configured key with a constant-time compare and a non-short-circuiting OR
   (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:108-125`). Production boot currently
   installs only `FABRIC_INTROSPECT_AUTH_KEY`; it deliberately rejects a secondary consumer key because this
   endpoint also resolves tenant identity (`crates/corelink-container/src/routes/auth_introspect/part-01.rs:257-269`).
2. The request body is parsed ONLY after the auth gate passes; the token is never logged
   (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:127-139`).
3. The PAT is verified (HMAC + D1 liveness + Argon2id + scope) before any tenant resolution
   (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:141-143`).
4. The plan (cache tier wire string) is resolved; a tier-query fault fails CLOSED with 503
   (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:166-195`).
5. The runner entitlement is read in one keyed lookup from `runners_entitlement` (migrations 0070+0072),
   returning both the concurrency cap and the monthly vCPU-hour ceiling
   (`crates/corelink-container/src/routes/auth_introspect/part-00.rs:356-369`).
6. The SQL is a single keyed select `SELECT max_concurrency, max_vcpu_h FROM runners_entitlement WHERE
   tenant_id = ?1` (`crates/corelink-container/src/routes/auth_introspect/part-00.rs:320-321`).
7. A valid 200 carries `max_concurrency` only when the tenant has a row and `max_vcpu_h` only when that
   NULLABLE column is set; both are `Option<u32>` with skip-serialization
   (`crates/corelink-container/src/routes/auth_introspect/part-00.rs:249`, `crates/corelink-container/src/routes/auth_introspect/part-00.rs:260`).
8. The route is only mounted when `FABRIC_INTROSPECT_AUTH_KEY` is present and ≥32 chars; otherwise it
   returns `None` and is not mounted (fail-CLOSED) (`crates/corelink-container/src/routes/auth_introspect/part-01.rs:238-270`).
9. The fabric's compute itself is being moved off the shared founder Mac onto off-Mac Linux capacity
   (Blacksmith is the roadmap meta-fix for the chronic single-runner fragility)
   (`docs/launch/2026-06-18-sota-tooling-roadmap.md:40-56`).
10. For the multi-tenant runner-CI path (cf-multitenant), the fabric plane shares the SAME identity/authorization
    source tables as the Worker mint (single source of truth, no divergent copy): `resolve_tenant_for_installation`
    resolves a GitHub App `installation_id` → isolated tenant via `tenant_gh_installation_map` (migration 0084),
    lookup-only — a miss is `Ok(None)` → transient `404 installation_not_mapped`, never an auto-provision
    (`crates/corelink-container/src/routes/auth_introspect/part-01.rs:140-151`); `repo_on_tenant_allowlist` reads
    `runner_repo_allowlist` (migration 0085) to confirm a `repo_full_name` is explicitly allowed for the tenant,
    fail-CLOSED (`crates/corelink-container/src/routes/auth_introspect/part-01.rs:174-206`).

# Invariants
- Any D1 fault during plan OR entitlement resolution fails CLOSED with 503 — the fabric maps 503 to
  unreachable and never serves a wrong/guessed plan or cap
  (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:187-219`).
- The runner cap is NOT derived from the cache plan: an absent `runners_entitlement` row means no Runners
  entitlement and the fabric rejects the placement (empty table = no cap = reject)
  (`crates/corelink-container/src/routes/auth_introspect/part-00.rs:323-369`).
- An invalid PAT returns a uniform `valid:false` with no tenant_id and no reason — no existence/plan oracle
  (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:197-200`).
- The dedicated fabric secret MUST be ≥32 chars or the route is not mounted at all
  (`crates/corelink-container/src/routes/auth_introspect/part-01.rs:223-245`).

# Gotchas
- `FABRIC_INTROSPECT_AUTH_KEY` is a DEDICATED secret, NOT `CORELINK_INTERNAL_AUTH_KEY`. Production boot
  does not load the optional `_HUGR` key: the builder explicitly rejects secondary consumers for this
  isolation-sensitive tenant-resolution surface (`crates/corelink-container/src/routes/auth_introspect/part-01.rs:257-269`).
- `max_concurrency` and `max_vcpu_h` are asymmetric: an absent cap ⇒ reject placement, but an absent
  vCPU-h ⇒ wall-off — they are not interchangeable signals
  (`crates/corelink-container/src/routes/auth_introspect/part-00.rs:281-303`).
- The `plan` field is informational only for the runners decision — capacity comes from the entitlement
  table, so do not authorize a runner off the cache tier
  (`crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:176-195`).

# Citations
1. `crates/corelink-container/src/routes/auth_introspect/part-00.rs:249` — `max_concurrency: Option<u32>` wire field.
2. `crates/corelink-container/src/routes/auth_introspect/part-00.rs:260` — `max_vcpu_h: Option<u32>` wire field.
3. `crates/corelink-container/src/routes/auth_introspect/part-00.rs:281-303` — `valid()` constructor: cap=reject, vcpu_h=wall-off asymmetry.
4. `crates/corelink-container/src/routes/auth_introspect/part-00.rs:320-321` — the `runners_entitlement` keyed SQL (separate axis).
5. `crates/corelink-container/src/routes/auth_introspect/part-00.rs:356-369` — `runner_concurrency_for_tenant` one-lookup resolver.
6. `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:108-125` — constant-time OR multi-key auth gate.
7. `crates/corelink-container/src/routes/auth_introspect/part-01.rs:238-269` — dedicated key validation and one-key production state construction, including the explicit no-secondary-key rationale.
8. `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:127-139` — body parsed only after the gate; token never logged.
9. `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:141-143` — PAT verify (HMAC+D1+Argon2id+scope).
10. `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:166-195` — plan resolution.
11. `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:176-195` — 200 valid response assembly (plan informational).
12. `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:187-219` — fail-CLOSED 503 on entitlement/tier/backend fault.
13. `crates/corelink-container/src/routes/auth_introspect/part-00-01.rs:197-200` — uniform `valid:false`, no oracle.
14. `crates/corelink-container/src/routes/auth_introspect/part-01.rs:223-245` — `FABRIC_INTROSPECT_AUTH_KEY` ≥32 or route not mounted.
15. `docs/launch/2026-06-18-sota-tooling-roadmap.md:40-56` — move compute off the shared Mac (Blacksmith / Linux runner).


# Revalidation

This concept was revalidated against the cumulative implementation tree; its existing source citations remain the controlling evidence for the behavior described above.
