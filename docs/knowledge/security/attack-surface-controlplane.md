---
type: "SecurityControl"
title: "Control-plane attack surface"
description: "The attacker-grade map of CoreLink's _system Durable-Object control plane: the one confirmed MEDIUM finding (CP-1 — per-consumer internal-auth key split, now FULLY CLOSED on the container plane: all three container dedicated-key gates are forwarded by the DO) and the audited negative results (Stripe HMAC, dedicated-key gates, cross-tenant isolation, DO lifecycle) that HELD."
source_files:
  - "docs/security/2026-06-23-brutal-controlplane.md"
  - "worker/src/durable_object.ts"
  - "crates/corelink-container/src/routes/internal_pat.rs"
  - "crates/corelink-container/src/routes/admin.rs"
  - "worker/src/lib/internal_auth.ts"
  - "worker/src/lib/runner_mint.ts"
checkpoint_sha: "bdf83b18abf324a57d93b0bc8db8366ddea6e182"
provenance: "AUTHORED"
tags: ["security", "control-plane", "red-team", "tenant-isolation", "internal-auth"]
timestamp: "2026-06-26T00:00:00Z"
---

# Control-plane attack surface

The control plane is the `_system` Durable-Object plane behind every internal surface — PAT mint, admin,
erase, runner mint, billing ingest, introspection, and the DO lifecycle — and a brutal 2026-06-23
red-team audited it end-to-end against code with file:line evidence. The result is sharp: exactly **one
confirmed finding (CP-1, MEDIUM) — now FULLY CLOSED on the container plane** — the shipped per-consumer
internal-auth key split was *inert on the container plane* because the Durable Object forwarded only the
single shared `CORELINK_INTERNAL_AUTH_KEY`. The container has exactly **three** dedicated-key gates —
PAT-mint (`crates/corelink-container/src/routes/internal_pat.rs:698`), admin
(`crates/corelink-container/src/routes/admin.rs:439`), and erase
(`crates/corelink-container/src/routes/admin.rs:460`) — and the DO now forwards **all three** dedicated
keys: `CORELINK_PAT_MINT_AUTH_KEY` / `CORELINK_ADMIN_AUTH_KEY` / `CORELINK_ERASE_AUTH_KEY`
(`worker/src/durable_object.ts:552-554`). So per-consumer isolation is real for every container gate and
CP-1 is closed on that plane. There is **no fourth container runner-mint gate**
(`grep -rn RUNNER_MINT crates/corelink-container/src` returns zero): `CORELINK_RUNNER_MINT_AUTH_KEY` is a
**Worker-edge key** (`worker/src/lib/internal_auth.ts:81`) that gates the runner dispatcher's
`/internal/v1/runner/{mint,revoke}` routes, and runner mint then reuses the container's audited
`/_internal/pat/mint` via the `_system` DO using the already-forwarded PAT-mint key
(`worker/src/lib/runner_mint.ts:18,127`). Runner-mint isolation therefore lives at the worker edge, not
in the container. Alongside CP-1 sits a long list of surfaces that were attacked and **held** (Stripe
HMAC, the dedicated-key gates, cross-tenant routing isolation, the F-006/F-007/F-012/F-019/F-020 fixes).
This concept is the auditable record of both, so the negative results are as durable as the finding. It
complements the [introspection endpoint](/auth/introspect.md) authz mechanism, the
[introspection flow](/flows/introspection-fabric.md), and the [2-level PAT moat](/auth/pat-moat.md).

# Role
- The control-plane threat map: which internal surfaces an attacker can reach and what gates them.
- The CP-1 record: a shipped isolation control once silently inert on the container plane, now FULLY
  CLOSED there — all three container dedicated-key gates (pat-mint / admin / erase) are forwarded.
- The runner-mint placement record: runner mint has no container gate; its dedicated key is a worker-edge
  control and the actual mint reuses the already-forwarded container pat-mint gate.
- The auditable negative result: the surfaces that were probed and held, so "it held" is not just folklore.

# How it works
1. The audit read every internal surface end-to-end — the Worker forward arms, the DO, the mint/rotate/
   exchange libs, and the container internal routes (`docs/security/2026-06-23-brutal-controlplane.md:1-13`).
2. CP-1 as found: both planes claim a per-consumer key split (each internal surface authenticates with its
   OWN secret), but the DO's `container.start({ env })` forwarded NONE of the dedicated keys
   (`docs/security/2026-06-23-brutal-controlplane.md:17-51`). The fix forwards the container's three
   dedicated keys — `CORELINK_PAT_MINT_AUTH_KEY` / `CORELINK_ADMIN_AUTH_KEY` / `CORELINK_ERASE_AUTH_KEY`
   (`worker/src/durable_object.ts:552-554`).
3. Those three are the ONLY dedicated-key gates the container exposes: PAT-mint reads
   `CORELINK_PAT_MINT_AUTH_KEY` (`crates/corelink-container/src/routes/internal_pat.rs:698`), admin reads
   `CORELINK_ADMIN_AUTH_KEY` (`crates/corelink-container/src/routes/admin.rs:439`), erase reads
   `CORELINK_ERASE_AUTH_KEY` (`crates/corelink-container/src/routes/admin.rs:460`), each with a 32-char
   floor and a shared-key fallback when its dedicated key is absent. With all three forwarded, the
   pat-mint / admin / erase container gates verify against their OWN dedicated key when one is
   provisioned — isolation is real, and CP-1 is closed on the container plane.
4. Runner mint has no container gate at all: `grep -rn RUNNER_MINT crates/corelink-container/src` returns
   zero. `CORELINK_RUNNER_MINT_AUTH_KEY` is a Worker-edge consumer key
   (`worker/src/lib/internal_auth.ts:81`) that gates the runner dispatcher's
   `/internal/v1/runner/{mint,revoke}` routes; the dispatcher then mints by calling the container's audited
   `/_internal/pat/mint` through the `_system` DO, reusing the already-forwarded PAT-mint key
   (`worker/src/lib/runner_mint.ts:18,127`). So runner-mint blast-radius isolation is enforced at the
   worker edge, and provisioning `CORELINK_RUNNER_MINT_AUTH_KEY` cannot self-inflict a container 401.
5. The Stripe webhook → runners/tier seed HELD: the body must pass HMAC-SHA256 with constant-time `ct_eq`
   + a 300s replay window before any `metadata.tenant_id`/cap is read, and the cap comes from a trusted
   price table, not the event (`docs/security/2026-06-23-brutal-controlplane.md:84-95`).
6. The ASK-2 billing ingest HELD: a dedicated `BILLING_INGEST_AUTH_KEY` (≥32, route unmounted otherwise),
   auth on raw bytes before JSON parse, idempotent by `(tenant_id, idem_key)`, and the meter is explicitly
   NOT Stripe-billable so there is no money-path poisoning
   (`docs/security/2026-06-23-brutal-controlplane.md:118-129`).
7. The introspection endpoint HELD: dedicated key, OR-combined multi-key gate with no consumer-identity
   timing oracle, uniform `valid:false`, fail-CLOSED 503 on any D1 fault, range-checked entitlement
   (`docs/security/2026-06-23-brutal-controlplane.md:131-137`).
8. Cross-tenant DO routing HELD: `_system`/`_oci` DO IDs are set only for fixed route kinds (never derived
   from a customer PAT), the data plane routes on the D1-resolved tenant UUID, and the client-trust-header
   strip removes any smuggled `x-corelink-*` (`docs/security/2026-06-23-brutal-controlplane.md:139-146`).

# Invariants
- Every internal control-plane surface is fail-CLOSED: a missing/short dedicated key leaves its route
  unmounted (or fails the gate closed), and any backend fault returns 503 rather than a guessed result
  (`docs/security/2026-06-23-brutal-controlplane.md:165-174`).
- CP-1 is NOT a direct break — it fails closed. It is now FULLY CLOSED on the container plane: the DO
  forwards the dedicated pat-mint / admin / erase keys (`worker/src/durable_object.ts:552-554`), which are
  the only three container dedicated-key gates (`crates/corelink-container/src/routes/internal_pat.rs:698`,
  `crates/corelink-container/src/routes/admin.rs:439`, `crates/corelink-container/src/routes/admin.rs:460`),
  so provisioning any of them no longer self-inflicts a 401 outage and per-consumer isolation is real for
  all of them.
- Runner-mint isolation is a worker-edge property, not a container residual: there is no container
  runner-mint gate (`grep -rn RUNNER_MINT crates/corelink-container/src` = 0);
  `CORELINK_RUNNER_MINT_AUTH_KEY` gates the dispatcher routes at the edge
  (`worker/src/lib/internal_auth.ts:81`) and the actual mint reuses the forwarded container pat-mint gate
  (`worker/src/lib/runner_mint.ts:18,127`).
- The cross-tenant fixes F-006/F-007/F-012/F-019/F-020 have NOT regressed: `owner_tenant` is mandatory and
  tenant-scoped on rotate/revoke, the path-spoof guard rejects URL-tenant ≠ PAT-tenant, and the
  trust-header strip runs on every forward (`docs/security/2026-06-23-brutal-controlplane.md:97-146`).
- The DO concurrent-start guard is sound: the lifecycle state flips synchronously before the await, so a
  second queued fetch defers rather than double-starting the container
  (`docs/security/2026-06-23-brutal-controlplane.md:148-157`).

# Gotchas
- CP-1's container internal port is not internet-reachable and the edge still gates on a sized key, so the
  severity was MEDIUM, not HIGH. It is now FULLY CLOSED on the container plane — the per-consumer-isolation
  claim is TRUE for every container gate (pat-mint / admin / erase), all three of which the DO forwards
  (`worker/src/durable_object.ts:552-554`).
- Do NOT describe runner-mint as a container "residual": the container has no runner-mint gate at all
  (`grep -rn RUNNER_MINT crates/corelink-container/src` = 0). `CORELINK_RUNNER_MINT_AUTH_KEY` is a
  worker-edge key (`worker/src/lib/internal_auth.ts:81`); runner mint reaches the container only through
  the already-forwarded `/_internal/pat/mint` gate (`worker/src/lib/runner_mint.ts:18,127`).
- The genuinely-forwarded set on the container is five consumers: `FABRIC_INTROSPECT_AUTH_KEY`,
  `BILLING_INGEST_AUTH_KEY`, and the three `/_internal/*`-family keys (pat-mint / admin / erase). Runner
  mint is not in this set because it is a worker-edge consumer, not a container gate
  (`worker/src/durable_object.ts:552-554`, `worker/src/lib/internal_auth.ts:81`).

# Citations
1. `docs/security/2026-06-23-brutal-controlplane.md:1-13` — audit scope (surfaces read end-to-end).
2. `docs/security/2026-06-23-brutal-controlplane.md:17-51` — CP-1 as found: per-consumer key split inert on the container plane (DO forwarded none).
2b. `worker/src/durable_object.ts:552-554` — the DO forwards all three container dedicated keys (PAT_MINT/ADMIN/ERASE); CP-1 FULLY CLOSED on the container plane.
2c. `crates/corelink-container/src/routes/internal_pat.rs:698` — the PAT-mint dedicated-key gate (`CORELINK_PAT_MINT_AUTH_KEY`, 32-char floor, shared fallback).
2d. `crates/corelink-container/src/routes/admin.rs:439` — the admin dedicated-key gate (`CORELINK_ADMIN_AUTH_KEY`).
2e. `crates/corelink-container/src/routes/admin.rs:460` — the erase dedicated-key gate (`CORELINK_ERASE_AUTH_KEY`).
2f. `worker/src/lib/internal_auth.ts:81` — `CORELINK_RUNNER_MINT_AUTH_KEY` is a worker-edge consumer key (no container gate exists).
2g. `worker/src/lib/runner_mint.ts:18-127` — runner mint reuses the container's `/_internal/pat/mint` via the `_system` DO (the already-forwarded pat-mint gate; the dispatch path spans :18→:127).
3. `docs/security/2026-06-23-brutal-controlplane.md:38-41` — the genuinely-isolated edge consumers (introspect / billing-ingest).
4. `docs/security/2026-06-23-brutal-controlplane.md:84-95` — Stripe webhook → seed HELD (HMAC + trusted price table).
5. `docs/security/2026-06-23-brutal-controlplane.md:97-146` — F-006/F-007/F-012 + cross-tenant routing held.
6. `docs/security/2026-06-23-brutal-controlplane.md:118-129` — ASK-2 billing ingest HELD (no money-path poisoning).
7. `docs/security/2026-06-23-brutal-controlplane.md:131-137` — introspect endpoint HELD.
8. `docs/security/2026-06-23-brutal-controlplane.md:139-146` — cross-tenant DO routing + trust-header strip held.
9. `docs/security/2026-06-23-brutal-controlplane.md:148-157` — DO lifecycle concurrent-start guard held.
10. `docs/security/2026-06-23-brutal-controlplane.md:165-174` — summary: 1 finding, otherwise fail-CLOSED solid.
