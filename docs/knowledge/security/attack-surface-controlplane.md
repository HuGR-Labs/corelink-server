---
type: "SecurityControl"
title: "Control-plane attack surface"
description: "The attacker-grade map of CoreLink's _system Durable-Object control plane: findings are a point-in-time record — **CP-1 (per-consumer internal-auth key split inert on the container plane) has since been RESOLVED in code**: `worker/src/durable_object.ts` now forwards the dedicated per-consumer keys and `crates/corelink-container/src/routes/admin.rs` prefers them. The audited negative results (Stripe HMAC, dedicated-key gates, cross-tenant isolation, DO lifecycle) still HELD."
source_files:
  - "docs/security/2026-06-23-brutal-controlplane.md"
  - "worker/src/durable_object.ts"
  - "crates/corelink-container/src/routes/admin.rs"
checkpoint_sha: "8af9ed65caf286d3f800e91d3f823face3aefd31"
provenance: "AUTHORED"
tags: ["security", "control-plane", "red-team", "tenant-isolation", "internal-auth"]
timestamp: "2026-06-26T00:00:00Z"
---

# Control-plane attack surface

The control plane is the `_system` Durable-Object plane behind every internal surface — PAT mint, admin,
erase, runner mint, billing ingest, introspection, and the DO lifecycle — and a brutal 2026-06-23
red-team audited it end-to-end against code with file:line evidence. At the time of that audit, exactly
**one confirmed finding (CP-1, MEDIUM)** existed — the shipped per-consumer internal-auth key split was
*inert on the container plane* because the Durable Object forwarded only the single shared
`CORELINK_INTERNAL_AUTH_KEY` — alongside a long list of surfaces that were attacked and **held** (Stripe
HMAC, the genuinely-isolated dedicated keys, cross-tenant routing isolation, the
F-006/F-007/F-012/F-019/F-020 fixes).

> ✅ **CP-1 — RESOLVED in code.** `worker/src/durable_object.ts` (comments there say "CP-1 (go-live
> audit)") now forwards `CORELINK_PAT_MINT_AUTH_KEY`, `CORELINK_ADMIN_AUTH_KEY`, `CORELINK_ERASE_AUTH_KEY`,
> `CORELINK_ADMIN_APPROVER_AUTH_KEY`, `CORELINK_DSR_ANCHOR_AUTH_KEY`, and `CORELINK_QUOTA_READ_AUTH_KEY` to
> the container alongside the shared key, and `crates/corelink-container/src/routes/admin.rs`'s
> `resolve_internal_auth_key` prefers the dedicated key when present, falling back to the shared one only
> when unset. **The mechanism is architecturally sound, not inert.** Whether isolation is ACTUALLY in
> effect for a given consumer still depends on operator provisioning — see the residual note below: as of
> this checkpoint, `CORELINK_PAT_MINT_AUTH_KEY`, `CORELINK_ERASE_AUTH_KEY`, `CORELINK_ADMIN_APPROVER_AUTH_KEY`,
> `CORELINK_QUOTA_READ_AUTH_KEY`, and (on the global `corelink-prod` Worker only) `CORELINK_DSR_ANCHOR_AUTH_KEY`
> ARE bound in prod (verified via the Cloudflare Workers secrets-list API — names only, values are
> write-only), but **`CORELINK_ADMIN_AUTH_KEY` is NOT bound on any of the 5 prod Workers**, so the
> admin-mutate surface still falls back to the shared key today.

This concept is the auditable record of both the finding and its resolution, so the negative results are
as durable as the fix. It complements the [introspection endpoint](/auth/introspect.md)
authz mechanism, the [introspection flow](/flows/introspection-fabric.md), and the
[2-level PAT moat](/auth/pat-moat.md).

# Role
- The control-plane threat map: which internal surfaces an attacker can reach and what gates them.
- The CP-1 record: a fixed isolation control — code-resolved, with one residual operator-provisioning gap
  (the dedicated admin-mutate key is not yet bound in prod).
- The auditable negative result: the surfaces that were probed and held, so "it held" is not just folklore.

# How it works
1. The audit read every internal surface end-to-end — the Worker forward arms, the DO, the mint/rotate/
   exchange libs, and the container internal routes (`docs/security/2026-06-23-brutal-controlplane.md:1-13`).
2. **RESOLVED.** Both planes claim a per-consumer key split (each internal surface authenticates with its
   OWN secret); at audit time the DO's `container.start({ env })` forwarded NONE of the dedicated keys —
   only the shared one (`docs/security/2026-06-23-brutal-controlplane.md:17-51`). **Fixed:** the DO now
   forwards `CORELINK_PAT_MINT_AUTH_KEY` / `CORELINK_ADMIN_AUTH_KEY` / `CORELINK_ERASE_AUTH_KEY` /
   `CORELINK_ADMIN_APPROVER_AUTH_KEY` / `CORELINK_DSR_ANCHOR_AUTH_KEY` / `CORELINK_QUOTA_READ_AUTH_KEY`
   alongside the shared key (`worker/src/durable_object.ts:795-826`).
3. Consequently each container internal gate (`/_internal/pat/mint`, `/_internal/dsr/*`,
   `/_internal/admin/*`, CAS-erase) now verifies against its OWN dedicated key WHEN BOUND, falling back to
   the shared key only if the dedicated key is unset (`crates/corelink-container/src/routes/admin.rs:567-605`).
   **Residual gap:** `CORELINK_ADMIN_AUTH_KEY` is not currently bound in prod (verified via the CF Workers
   secrets-list API on all 5 prod Workers), so `/_internal/admin/*` mutate routes still resolve to the
   shared key until an operator provisions it.
4. The Stripe webhook → runners/tier seed HELD: the body must pass HMAC-SHA256 with constant-time `ct_eq`
   + a 300s replay window before any `metadata.tenant_id`/cap is read, and the cap comes from a trusted
   price table, not the event (`docs/security/2026-06-23-brutal-controlplane.md:84-95`).
5. The ASK-2 billing ingest HELD: a dedicated `BILLING_INGEST_AUTH_KEY` (≥32, route unmounted otherwise),
   auth on raw bytes before JSON parse, idempotent by `(tenant_id, idem_key)`, and the meter is explicitly
   NOT Stripe-billable so there is no money-path poisoning
   (`docs/security/2026-06-23-brutal-controlplane.md:118-129`).
6. The introspection endpoint HELD: dedicated key, OR-combined multi-key gate with no consumer-identity
   timing oracle, uniform `valid:false`, fail-CLOSED 503 on any D1 fault, range-checked entitlement
   (`docs/security/2026-06-23-brutal-controlplane.md:131-137`).
7. Cross-tenant DO routing HELD: `_system`/`_oci` DO IDs are set only for fixed route kinds (never derived
   from a customer PAT), the data plane routes on the D1-resolved tenant UUID, and the client-trust-header
   strip removes any smuggled `x-corelink-*` (`docs/security/2026-06-23-brutal-controlplane.md:139-146`).

# Invariants
- Every internal control-plane surface is fail-CLOSED: a missing/short dedicated key leaves its route
  unmounted, and any backend fault returns 503 rather than a guessed result
  (`docs/security/2026-06-23-brutal-controlplane.md:165-174`).
- **CP-1 self-inflicted-outage risk is CLOSED.** At audit time CP-1 was not a direct break — it failed
  closed — but it nullified a shipped, documented control and risked a guaranteed self-inflicted outage
  the moment the dedicated-key hardening was turned on (the edge would forward the dedicated value, the
  container would never accept it → 401) (`docs/security/2026-06-23-brutal-controlplane.md:53-71`). The DO
  now forwards every dedicated key the container understands, so binding a dedicated secret no longer
  401s the container (`worker/src/durable_object.ts:796-800`).
- The cross-tenant fixes F-006/F-007/F-012/F-019/F-020 have NOT regressed: `owner_tenant` is mandatory and
  tenant-scoped on rotate/revoke, the path-spoof guard rejects URL-tenant ≠ PAT-tenant, and the
  trust-header strip runs on every forward (`docs/security/2026-06-23-brutal-controlplane.md:97-146`).
- The DO concurrent-start guard is sound: the lifecycle state flips synchronously before the await, so a
  second queued fetch defers rather than double-starting the container
  (`docs/security/2026-06-23-brutal-controlplane.md:148-157`).

# Gotchas
- **⚠️ CP-1 is code-RESOLVED but not fully provisioned in prod.** Do not read "the mechanism is fixed" as
  "isolation is fully in effect everywhere" — `CORELINK_ADMIN_AUTH_KEY` (the dedicated admin-mutate key) is
  not bound on any of the 5 prod Workers as of this checkpoint (verified via the CF Workers secrets-list
  API), so `/_internal/admin/*` mutate routes still resolve to the shared `CORELINK_INTERNAL_AUTH_KEY`
  until an operator runs `wrangler secret put CORELINK_ADMIN_AUTH_KEY --env prod` (+ the 4 regional envs).
  `CORELINK_PAT_MINT_AUTH_KEY`, `CORELINK_ERASE_AUTH_KEY`, `CORELINK_ADMIN_APPROVER_AUTH_KEY`,
  `CORELINK_QUOTA_READ_AUTH_KEY`, and `CORELINK_DSR_ANCHOR_AUTH_KEY` ARE bound and isolated today.
- CP-1's container internal port was never internet-reachable and the edge always gated on a sized key,
  so the finding's severity was MEDIUM, not HIGH, even before the fix
  (`docs/security/2026-06-23-brutal-controlplane.md:65-78`).
- Two consumers (`FABRIC_INTROSPECT_AUTH_KEY`, `BILLING_INGEST_AUTH_KEY`) were ALWAYS genuinely forwarded
  and isolated, even before the CP-1 fix — only the four `/_internal/*`-family keys were affected
  (`docs/security/2026-06-23-brutal-controlplane.md:38-41`).
- The CP-1 fix should also add the newly-forwarded keys to `scripts/check-env-contract.py` if it has not
  already, or a future container `env::var` read can silently no-op again — the same env-contract gap
  class (`docs/security/2026-06-23-brutal-controlplane.md:73-78`); not independently re-verified in this
  pass.

# Citations
1. `docs/security/2026-06-23-brutal-controlplane.md:1-13` — audit scope (surfaces read end-to-end).
2. `docs/security/2026-06-23-brutal-controlplane.md:17-51` — CP-1: per-consumer key split inert on the container plane.
3. `docs/security/2026-06-23-brutal-controlplane.md:38-41` — the two genuinely-isolated consumers (introspect / billing-ingest).
4. `docs/security/2026-06-23-brutal-controlplane.md:42-51` — every container internal gate falls back to the shared key.
5. `docs/security/2026-06-23-brutal-controlplane.md:53-71` — CP-1 latent availability landmine (401 on hardening).
6. `docs/security/2026-06-23-brutal-controlplane.md:65-78` — reachability/severity + the false isolation claim to soften.
7. `docs/security/2026-06-23-brutal-controlplane.md:73-78` — fix must also extend the env-contract check.
8. `docs/security/2026-06-23-brutal-controlplane.md:84-95` — Stripe webhook → seed HELD (HMAC + trusted price table).
9. `docs/security/2026-06-23-brutal-controlplane.md:97-146` — F-006/F-007/F-012 + cross-tenant routing held.
10. `docs/security/2026-06-23-brutal-controlplane.md:118-129` — ASK-2 billing ingest HELD (no money-path poisoning).
11. `docs/security/2026-06-23-brutal-controlplane.md:131-137` — introspect endpoint HELD.
12. `docs/security/2026-06-23-brutal-controlplane.md:139-146` — cross-tenant DO routing + trust-header strip held.
13. `docs/security/2026-06-23-brutal-controlplane.md:148-157` — DO lifecycle concurrent-start guard held.
14. `docs/security/2026-06-23-brutal-controlplane.md:165-174` — summary: 1 finding, otherwise fail-CLOSED solid.
15. `worker/src/durable_object.ts:795-826` — CP-1 fix: the DO forwards all six dedicated per-consumer keys (comments explicitly reference "CP-1 (go-live audit)").
16. `crates/corelink-container/src/routes/admin.rs:567-605` — `resolve_internal_auth_key`: dedicated key preferred, shared key only as fallback.
17. CF Workers secrets-list API (`accounts/{id}/workers/scripts/{name}/secrets`, names only) checked against `corelink-prod` + the 4 regional Workers on this checkpoint's date: `CORELINK_PAT_MINT_AUTH_KEY` / `CORELINK_ERASE_AUTH_KEY` / `CORELINK_ADMIN_APPROVER_AUTH_KEY` / `CORELINK_QUOTA_READ_AUTH_KEY` bound on all 5; `CORELINK_DSR_ANCHOR_AUTH_KEY` bound on `corelink-prod` only; `CORELINK_ADMIN_AUTH_KEY` bound on NONE.
