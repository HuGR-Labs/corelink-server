---
type: "SecurityControl"
title: "Control-plane attack surface"
description: "The attacker-grade map of CoreLink's _system Durable-Object control plane: the one confirmed MEDIUM finding (CP-1 — per-consumer internal-auth key split, now PARTIALLY CLOSED: 3 of 4 dedicated keys forwarded) and the audited negative results (Stripe HMAC, dedicated-key gates, cross-tenant isolation, DO lifecycle) that HELD."
source_files:
  - "docs/security/2026-06-23-brutal-controlplane.md"
  - "worker/src/durable_object.ts"
checkpoint_sha: "bdf83b18abf324a57d93b0bc8db8366ddea6e182"
provenance: "AUTHORED"
tags: ["security", "control-plane", "red-team", "tenant-isolation", "internal-auth"]
timestamp: "2026-06-26T00:00:00Z"
---

# Control-plane attack surface

The control plane is the `_system` Durable-Object plane behind every internal surface — PAT mint, admin,
erase, runner mint, billing ingest, introspection, and the DO lifecycle — and a brutal 2026-06-23
red-team audited it end-to-end against code with file:line evidence. The result is sharp: exactly **one
confirmed finding (CP-1, MEDIUM) — now PARTIALLY CLOSED** — the shipped per-consumer internal-auth key
split was *inert on the container plane* because the Durable Object forwarded only the single shared
`CORELINK_INTERNAL_AUTH_KEY`; the DO now forwards **3 of the 4** dedicated keys —
`CORELINK_PAT_MINT_AUTH_KEY` / `CORELINK_ADMIN_AUTH_KEY` / `CORELINK_ERASE_AUTH_KEY`
(`worker/src/durable_object.ts:552-554`) — so per-consumer isolation is now real for the mint/admin/erase
gates; only the runner-mint dedicated key (`CORELINK_RUNNER_MINT_AUTH_KEY`) remains un-forwarded and still
falls back to the shared key. CP-1 is therefore down to a single residual gate — and a long list of surfaces that were
attacked and **held** (Stripe HMAC, the genuinely-isolated dedicated keys, cross-tenant routing isolation,
the F-006/F-007/F-012/F-019/F-020 fixes). This concept is the auditable record of both, so the negative
results are as durable as the finding. It complements the [introspection endpoint](/auth/introspect.md)
authz mechanism, the [introspection flow](/flows/introspection-fabric.md), and the
[2-level PAT moat](/auth/pat-moat.md).

# Role
- The control-plane threat map: which internal surfaces an attacker can reach and what gates them.
- The CP-1 record: a shipped isolation control once silently inert on the container plane, now PARTIALLY
  CLOSED — 3 of 4 dedicated keys forwarded, only runner-mint residual.
- The auditable negative result: the surfaces that were probed and held, so "it held" is not just folklore.

# How it works
1. The audit read every internal surface end-to-end — the Worker forward arms, the DO, the mint/rotate/
   exchange libs, and the container internal routes (`docs/security/2026-06-23-brutal-controlplane.md:1-13`).
2. CP-1: both planes claim a per-consumer key split (each internal surface authenticates with its OWN
   secret). The audit found the DO's `container.start({ env })` forwarded NONE of the four dedicated keys
   (`docs/security/2026-06-23-brutal-controlplane.md:17-51`); the DO now forwards **3 of 4** —
   `CORELINK_PAT_MINT_AUTH_KEY` / `CORELINK_ADMIN_AUTH_KEY` / `CORELINK_ERASE_AUTH_KEY`
   (`worker/src/durable_object.ts:552-554`) — leaving only `CORELINK_RUNNER_MINT_AUTH_KEY` un-forwarded.
3. Consequently the pat-mint / admin / erase container gates (`/_internal/pat/mint`, `/_internal/admin/*`,
   `/_internal/dsr/erase`, CAS-erase) now verify against their OWN dedicated key when one is provisioned —
   isolation is real for these. Only the runner-mint gate still falls back to the SHARED key (the residual
   slice of what the split was built to prevent) (`worker/src/durable_object.ts:552-554`,
   `docs/security/2026-06-23-brutal-controlplane.md:42-51`).
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
- CP-1 is NOT a direct break — it fails closed. It is now PARTIALLY CLOSED: the DO forwards the dedicated
  pat-mint / admin / erase keys (`worker/src/durable_object.ts:552-554`), so provisioning those no longer
  self-inflicts a 401 outage and per-consumer isolation is real for them. The residual self-inflicted-outage
  landmine survives ONLY for the still-un-forwarded `CORELINK_RUNNER_MINT_AUTH_KEY` (provision it and the
  edge forwards a dedicated value the container never accepts → 401)
  (`worker/src/durable_object.ts:552-554`, `docs/security/2026-06-23-brutal-controlplane.md:53-71`).
- The cross-tenant fixes F-006/F-007/F-012/F-019/F-020 have NOT regressed: `owner_tenant` is mandatory and
  tenant-scoped on rotate/revoke, the path-spoof guard rejects URL-tenant ≠ PAT-tenant, and the
  trust-header strip runs on every forward (`docs/security/2026-06-23-brutal-controlplane.md:97-146`).
- The DO concurrent-start guard is sound: the lifecycle state flips synchronously before the await, so a
  second queued fetch defers rather than double-starting the container
  (`docs/security/2026-06-23-brutal-controlplane.md:148-157`).

# Gotchas
- CP-1's container internal port is not internet-reachable and the edge still gates on a sized key, so the
  severity is MEDIUM, not HIGH. It is now PARTIALLY CLOSED — the per-consumer-isolation claim is TRUE on the
  container plane for pat-mint / admin / erase (`worker/src/durable_object.ts:552-554`) and remains false
  ONLY for runner-mint, which still gates on the shared key
  (`docs/security/2026-06-23-brutal-controlplane.md:65-78`).
- The genuinely-forwarded set is now five consumers: `FABRIC_INTROSPECT_AUTH_KEY`,
  `BILLING_INGEST_AUTH_KEY`, and the three `/_internal/*`-family keys (pat-mint / admin / erase). Only the
  fourth `/_internal/*`-family key, `CORELINK_RUNNER_MINT_AUTH_KEY`, is inert — so do not assume the whole
  key-split is broken (`worker/src/durable_object.ts:552-554`,
  `docs/security/2026-06-23-brutal-controlplane.md:38-41`).
- Closing the residual runner-mint slice must ALSO add the key to `scripts/check-env-contract.py`, or a
  future container `env::var` read can silently no-op again — the same env-contract gap class
  (`docs/security/2026-06-23-brutal-controlplane.md:73-78`).

# Citations
1. `docs/security/2026-06-23-brutal-controlplane.md:1-13` — audit scope (surfaces read end-to-end).
2. `docs/security/2026-06-23-brutal-controlplane.md:17-51` — CP-1: per-consumer key split inert on the container plane (as audited).
2b. `worker/src/durable_object.ts:552-554` — the DO now forwards 3 of 4 dedicated keys (PAT_MINT/ADMIN/ERASE); CP-1 PARTIALLY CLOSED, only runner-mint residual.
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
