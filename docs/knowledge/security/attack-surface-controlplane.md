---
type: "SecurityControl"
title: "Control-plane attack surface"
description: "The attacker-grade map of CoreLink's _system Durable-Object control plane: the one confirmed MEDIUM finding (CP-1 — per-consumer internal-auth key split) and where it actually stands — admin + erase are genuinely closed (the Path-A proxy resolves and forwards the per-consumer key), but pat-mint isolation is NOT real because its live callers still send the SHARED key, so provisioning the dedicated PAT-mint key self-inflicts a 401 until the worker is updated. Plus the audited negative results (Stripe HMAC, dedicated-key gates, cross-tenant isolation, DO lifecycle) that HELD."
source_files:
  - "docs/security/2026-06-23-brutal-controlplane.md"
  - "worker/src/durable_object.ts"
  - "worker/src/index.ts"
  - "crates/corelink-container/src/routes/internal_pat.rs"
  - "crates/corelink-container/src/routes/admin.rs"
  - "worker/src/lib/internal_auth.ts"
  - "worker/src/lib/session_exchange.ts"
  - "worker/src/lib/runner_mint.ts"
checkpoint_sha: "a367df9b6df02af27b91ef22a6d3a53824eca42d"
provenance: "AUTHORED"
tags: ["security", "control-plane", "red-team", "tenant-isolation", "internal-auth"]
timestamp: "2026-06-26T00:00:00Z"
---

# Control-plane attack surface

The control plane is the `_system` Durable-Object plane behind every internal surface — PAT mint, admin,
erase, runner mint, billing ingest, introspection, and the DO lifecycle — and a brutal 2026-06-23
red-team audited it end-to-end against code with file:line evidence. The result is sharp: exactly **one
confirmed finding (CP-1, MEDIUM)** — the shipped per-consumer internal-auth key split was *inert on the
container plane* because the Durable Object originally forwarded only the single shared
`CORELINK_INTERNAL_AUTH_KEY`. The DO now forwards all three container dedicated keys
(`worker/src/durable_object.ts:552-554`), but **CP-1 is only 2/3 closed**, and the difference is which
worker path reaches each container gate:

- **admin + erase are genuinely closed.** They are reached through the Worker's Path-A internal proxy,
  which resolves the **per-consumer** key for the route (`worker/src/index.ts:1565`,
  `resolveConsumerKey`), authenticates the caller against it, and forwards exactly that resolved key to
  the container (`worker/src/index.ts:1622`). So once `CORELINK_ADMIN_AUTH_KEY` /
  `CORELINK_ERASE_AUTH_KEY` are provisioned, the worker resolves them, the caller is verified against
  them, and the container's dedicated-preferred gates (`crates/corelink-container/src/routes/admin.rs:439`,
  `:460`) match — per-consumer isolation is real.
- **pat-mint is NOT isolated, and provisioning its dedicated key is a live landmine.** The only live
  pat-mint callers — token exchange (`worker/src/lib/session_exchange.ts:374`) and runner dispatch
  (`worker/src/lib/runner_mint.ts:130`) — read the **SHARED** `CORELINK_INTERNAL_AUTH_KEY` and hand it
  to `mintScopedPat`, which hardwires it into `x-corelink-internal-auth`
  (`worker/src/lib/session_exchange.ts:493`). These callers build a fresh request straight to the
  `_system` DO; they do NOT pass through the Path-A per-consumer resolver. So even though the DO forwards
  `CORELINK_PAT_MINT_AUTH_KEY`, the moment an operator provisions it the container's dedicated-preferred
  gate (`crates/corelink-container/src/routes/internal_pat.rs:698`) starts expecting the dedicated key,
  the worker keeps sending the shared one, and **every mint 401s** — a self-inflicted outage. The
  landmine the original audit feared did not vanish; it MOVED from the (non-existent) runner-mint gate to
  pat-mint, and it stays armed until `mintScopedPat` / token exchange / runner dispatch are taught to send
  the dedicated PAT-mint key.

There is **no fourth container runner-mint gate** (`grep -rn RUNNER_MINT crates/corelink-container/src`
returns zero): `CORELINK_RUNNER_MINT_AUTH_KEY` is a **Worker-edge key**
(`worker/src/lib/internal_auth.ts:81`) that gates the runner dispatcher's
`/internal/v1/runner/{mint,revoke}` routes; runner mint then mints by calling the container's audited
`/_internal/pat/mint` via `mintScopedPat` — which means runner mint sends the SHARED key too and is
subject to the same pat-mint landmine. Alongside CP-1 sits a long list of surfaces that were attacked and
**held** (Stripe HMAC, the dedicated-key gates, cross-tenant routing isolation, the
F-006/F-007/F-012/F-019/F-020 fixes). This concept is the auditable record of both, so the negative
results are as durable as the finding. It complements the [introspection endpoint](/auth/introspect.md)
authz mechanism, the [introspection flow](/flows/introspection-fabric.md), and the
[2-level PAT moat](/auth/pat-moat.md).

# Role
- The control-plane threat map: which internal surfaces an attacker can reach and what gates them.
- The CP-1 record: a shipped isolation control once silently inert on the container plane, now **2/3
  closed** — admin + erase are genuinely per-consumer-isolated (Path-A proxy resolves and forwards their
  dedicated key), but pat-mint is not, because its live callers still send the shared key.
- The pat-mint landmine record: the dedicated `CORELINK_PAT_MINT_AUTH_KEY` is forwarded by the DO and
  honored by the container, but the live worker callers (token exchange, runner dispatch) send the shared
  key — so provisioning the dedicated key 401s every mint until the worker is updated.
- The runner-mint placement record: runner mint has no container gate; its dedicated key is a worker-edge
  control and the actual mint reuses the container pat-mint route via `mintScopedPat`.
- The auditable negative result: the surfaces that were probed and held, so "it held" is not just folklore.

# How it works
1. The audit read every internal surface end-to-end — the Worker forward arms, the DO, the mint/rotate/
   exchange libs, and the container internal routes (`docs/security/2026-06-23-brutal-controlplane.md:1-13`).
2. CP-1 as found: both planes claim a per-consumer key split (each internal surface authenticates with its
   OWN secret), but the DO's `container.start({ env })` forwarded NONE of the dedicated keys
   (`docs/security/2026-06-23-brutal-controlplane.md:17-51`). The fix forwards the container's three
   dedicated keys — `CORELINK_PAT_MINT_AUTH_KEY` / `CORELINK_ADMIN_AUTH_KEY` / `CORELINK_ERASE_AUTH_KEY`
   (`worker/src/durable_object.ts:552-554`).
3. Those three are the ONLY dedicated-key gates the container exposes, each dedicated-PREFERRED with a
   shared-key fallback and a 32-char floor: PAT-mint reads `CORELINK_PAT_MINT_AUTH_KEY`
   (`crates/corelink-container/src/routes/internal_pat.rs:698`), admin reads `CORELINK_ADMIN_AUTH_KEY`
   (`crates/corelink-container/src/routes/admin.rs:439`), erase reads `CORELINK_ERASE_AUTH_KEY`
   (`crates/corelink-container/src/routes/admin.rs:460`).
4. admin + erase are genuinely closed because they arrive through the Worker's Path-A internal proxy:
   `/_internal/*` requests resolve the **per-consumer** key for the route
   (`worker/src/index.ts:1565`, `resolveConsumerKey`), authenticate the caller against it
   (constant-time), then re-set `x-corelink-internal-auth` to that exact resolved key before forwarding to
   the `_system` DO (`worker/src/index.ts:1622`). So when the dedicated admin/erase key is provisioned the
   worker resolves it, verifies against it, and the container gate matches — isolation is real.
5. pat-mint is NOT isolated. Its live callers bypass the Path-A resolver entirely: token exchange reads
   the SHARED `CORELINK_INTERNAL_AUTH_KEY` (`worker/src/lib/session_exchange.ts:374`) and runner dispatch
   reads the same SHARED key (`worker/src/lib/runner_mint.ts:130`); both pass it to `mintScopedPat`, which
   builds a fresh request straight to the `_system` DO with `x-corelink-internal-auth` = that shared key
   (`worker/src/lib/session_exchange.ts:493`). So provisioning `CORELINK_PAT_MINT_AUTH_KEY` flips the
   container gate (`crates/corelink-container/src/routes/internal_pat.rs:698`) to expect the dedicated key
   while the worker still sends the shared one → 401 on every mint. The pat-mint isolation is only real
   once `mintScopedPat` / token exchange / runner dispatch send the dedicated key.
6. Runner mint has no container gate at all: `grep -rn RUNNER_MINT crates/corelink-container/src` returns
   zero. `CORELINK_RUNNER_MINT_AUTH_KEY` is a Worker-edge consumer key
   (`worker/src/lib/internal_auth.ts:81`) that gates the runner dispatcher's
   `/internal/v1/runner/{mint,revoke}` routes; the dispatcher then mints by calling the container's
   `/_internal/pat/mint` through `mintScopedPat` (`worker/src/lib/runner_mint.ts:130,188`), sending the
   shared key — so runner mint is subject to the same pat-mint landmine, not a separate one. Provisioning
   `CORELINK_RUNNER_MINT_AUTH_KEY` itself cannot self-inflict a container 401 (no container runner-mint
   gate reads it).
7. The Stripe webhook → runners/tier seed HELD: the body must pass HMAC-SHA256 with constant-time `ct_eq`
   + a 300s replay window before any `metadata.tenant_id`/cap is read, and the cap comes from a trusted
   price table, not the event (`docs/security/2026-06-23-brutal-controlplane.md:84-95`).
8. The ASK-2 billing ingest HELD: a dedicated `BILLING_INGEST_AUTH_KEY` (≥32, route unmounted otherwise),
   auth on raw bytes before JSON parse, idempotent by `(tenant_id, idem_key)`, and the meter is explicitly
   NOT Stripe-billable so there is no money-path poisoning
   (`docs/security/2026-06-23-brutal-controlplane.md:118-129`).
9. The introspection endpoint HELD: dedicated key, OR-combined multi-key gate with no consumer-identity
   timing oracle, uniform `valid:false`, fail-CLOSED 503 on any D1 fault, range-checked entitlement
   (`docs/security/2026-06-23-brutal-controlplane.md:131-137`).
10. Cross-tenant DO routing HELD: `_system`/`_oci` DO IDs are set only for fixed route kinds (never derived
    from a customer PAT), the data plane routes on the D1-resolved tenant UUID, and the client-trust-header
    strip removes any smuggled `x-corelink-*` (`docs/security/2026-06-23-brutal-controlplane.md:139-146`).

# Invariants
- Every internal control-plane surface is fail-CLOSED: a missing/short dedicated key leaves its route
  unmounted (or fails the gate closed), and any backend fault returns 503 rather than a guessed result
  (`docs/security/2026-06-23-brutal-controlplane.md:165-174`).
- CP-1 is NOT a direct break — it fails closed. It is **2/3 closed**: admin + erase are genuinely
  per-consumer-isolated because the Path-A proxy resolves and forwards their dedicated key
  (`worker/src/index.ts:1565,1622`) and the container gates honor it
  (`crates/corelink-container/src/routes/admin.rs:439`, `:460`). pat-mint is the open third: the DO
  forwards `CORELINK_PAT_MINT_AUTH_KEY` (`worker/src/durable_object.ts:552-554`) and the container honors
  it dedicated-preferred (`crates/corelink-container/src/routes/internal_pat.rs:698`), but its live
  callers send the SHARED key (`worker/src/lib/session_exchange.ts:374,493`,
  `worker/src/lib/runner_mint.ts:130`), so provisioning the dedicated key self-inflicts a 401 until the
  worker callers are updated.
- Runner-mint isolation is a worker-edge property, not a container residual: there is no container
  runner-mint gate (`grep -rn RUNNER_MINT crates/corelink-container/src` = 0);
  `CORELINK_RUNNER_MINT_AUTH_KEY` gates the dispatcher routes at the edge
  (`worker/src/lib/internal_auth.ts:81`) and the actual mint goes through the container pat-mint route via
  `mintScopedPat` (`worker/src/lib/runner_mint.ts:130,188`) carrying the shared key.
- The cross-tenant fixes F-006/F-007/F-012/F-019/F-020 have NOT regressed: `owner_tenant` is mandatory and
  tenant-scoped on rotate/revoke, the path-spoof guard rejects URL-tenant ≠ PAT-tenant, and the
  trust-header strip runs on every forward (`docs/security/2026-06-23-brutal-controlplane.md:97-146`).
- The DO concurrent-start guard is sound: the lifecycle state flips synchronously before the await, so a
  second queued fetch defers rather than double-starting the container
  (`docs/security/2026-06-23-brutal-controlplane.md:148-157`).

# Gotchas
- CP-1's container internal port is not internet-reachable and the edge still gates on a sized key, so the
  severity was MEDIUM, not HIGH. But do NOT describe it as "fully closed" or "isolation real for all": the
  per-consumer claim is TRUE for admin + erase (Path-A proxy forwards the resolved per-consumer key,
  `worker/src/index.ts:1622`) and FALSE for pat-mint, whose live callers still send the shared key
  (`worker/src/lib/session_exchange.ts:374,493`, `worker/src/lib/runner_mint.ts:130`).
- **pat-mint dedicated-key landmine.** Do NOT provision `CORELINK_PAT_MINT_AUTH_KEY` alone: the container
  flips to dedicated-preferred (`crates/corelink-container/src/routes/internal_pat.rs:698`) while
  `mintScopedPat` still sends the shared key (`worker/src/lib/session_exchange.ts:493`), so every signup /
  session-exchange / runner mint 401s. Land the worker change (send the dedicated PAT-mint key) in the
  SAME deploy, or leave both unprovisioned (shared fallback works).
- Do NOT describe runner-mint as a container "residual": the container has no runner-mint gate at all
  (`grep -rn RUNNER_MINT crates/corelink-container/src` = 0). `CORELINK_RUNNER_MINT_AUTH_KEY` is a
  worker-edge key (`worker/src/lib/internal_auth.ts:81`); runner mint reaches the container only through
  the pat-mint route via `mintScopedPat` (`worker/src/lib/runner_mint.ts:130,188`), so it inherits the
  pat-mint landmine, not a separate one.
- The genuinely-forwarded set on the container is five consumers: `FABRIC_INTROSPECT_AUTH_KEY`,
  `BILLING_INGEST_AUTH_KEY`, and the three `/_internal/*`-family keys (pat-mint / admin / erase). Runner
  mint is not in this set because it is a worker-edge consumer, not a container gate
  (`worker/src/durable_object.ts:552-554`, `worker/src/lib/internal_auth.ts:81`).

# Citations
1. `docs/security/2026-06-23-brutal-controlplane.md:1-13` — audit scope (surfaces read end-to-end).
2. `docs/security/2026-06-23-brutal-controlplane.md:17-51` — CP-1 as found: per-consumer key split inert on the container plane (DO forwarded none).
2b. `worker/src/durable_object.ts:552-554` — the DO forwards all three container dedicated keys (PAT_MINT/ADMIN/ERASE).
2c. `crates/corelink-container/src/routes/internal_pat.rs:698` — the PAT-mint dedicated-preferred gate (`CORELINK_PAT_MINT_AUTH_KEY` first, shared fallback, 32-char floor).
2d. `crates/corelink-container/src/routes/admin.rs:439` — the admin dedicated-preferred gate (`CORELINK_ADMIN_AUTH_KEY`).
2e. `crates/corelink-container/src/routes/admin.rs:460` — the erase dedicated-preferred gate (`CORELINK_ERASE_AUTH_KEY`).
2f. `worker/src/index.ts:1565` — Path-A proxy resolves the PER-CONSUMER key (`resolveConsumerKey`) for `/_internal/*` (admin / erase).
2g. `worker/src/index.ts:1622` — Path-A proxy forwards that resolved per-consumer key → admin + erase genuinely isolated.
2h. `worker/src/lib/session_exchange.ts:374` — the live pat-mint caller reads the SHARED `CORELINK_INTERNAL_AUTH_KEY`.
2i. `worker/src/lib/session_exchange.ts:493` — `mintScopedPat` sets `x-corelink-internal-auth` to that shared key (proof pat-mint sends shared, not the dedicated key).
2j. `worker/src/lib/runner_mint.ts:130` — runner dispatch ALSO reads the SHARED key and feeds it to `mintScopedPat` (proof, not the old wrong "already-forwarded PAT-mint key" claim).
2k. `worker/src/lib/internal_auth.ts:81` — `CORELINK_RUNNER_MINT_AUTH_KEY` is a worker-edge consumer key (no container gate exists).
3. `docs/security/2026-06-23-brutal-controlplane.md:38-41` — the genuinely-isolated edge consumers (introspect / billing-ingest).
4. `docs/security/2026-06-23-brutal-controlplane.md:84-95` — Stripe webhook → seed HELD (HMAC + trusted price table).
5. `docs/security/2026-06-23-brutal-controlplane.md:97-146` — F-006/F-007/F-012 + cross-tenant routing held.
6. `docs/security/2026-06-23-brutal-controlplane.md:118-129` — ASK-2 billing ingest HELD (no money-path poisoning).
7. `docs/security/2026-06-23-brutal-controlplane.md:131-137` — introspect endpoint HELD.
8. `docs/security/2026-06-23-brutal-controlplane.md:139-146` — cross-tenant DO routing + trust-header strip held.
9. `docs/security/2026-06-23-brutal-controlplane.md:148-157` — DO lifecycle concurrent-start guard held.
10. `docs/security/2026-06-23-brutal-controlplane.md:165-174` — summary: 1 finding, otherwise fail-CLOSED solid.
