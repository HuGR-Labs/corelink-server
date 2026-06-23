# Brutal control-plane red-team — 2026-06-23

Attacker-grade audit of the `_system` Durable-Object control plane, multi-tenant
isolation, the NEW runners-entitlement seed + ASK-2 billing ingest, and the DO
lifecycle (F-019/F-020 class). Read-only; no destructive/charging probes against
prod. Findings below are CONFIRMED against code with file:line; the rest is the
"what I tried and it held" section so the negative result is auditable.

Scope read end-to-end: `worker/src/index.ts` (all forward arms), `durable_object.ts`,
`lib/{internal_auth,runner_mint,auth_rotate,session_exchange}.ts`,
`crates/corelink-container/src/routes/{internal_pat,auth_introspect,billing_ingest,admin}.rs`,
`crates/corelink-stripe-real/src/webhook.rs`,
`crates/corelink-billing-stripe-materializer/src/{handler,runners}.rs`, `main.rs`.

---

## CONFIRMED FINDING — CP-1 (MEDIUM): per-consumer internal-auth key split is INERT on the container plane — the DO never forwards the dedicated keys, so every container internal gate silently falls back to the single shared `CORELINK_INTERNAL_AUTH_KEY`

**Attack class:** defense-in-depth defeat (blast-radius isolation bypass) + latent
availability landmine (silent 401 on key rotation).

**Chain / evidence:**
- The red-team-#3 design is a per-consumer key split: each internal surface
  (`pat_mint` / `admin` / `erase` / `runner_mint`) authenticates with its OWN
  secret so a single leak does not unlock all internal surfaces. Both planes claim
  to honor it:
  - Worker edge: `worker/src/lib/internal_auth.ts:73-90` (`resolveConsumerKey`) and
    `worker/src/index.ts:236-246` (`internalConsumerForPath`) resolve the dedicated
    key, falling back to shared.
  - Container: `crates/corelink-container/src/routes/admin.rs:401-428`
    (`resolve_internal_auth_key`) reads `env::var("CORELINK_PAT_MINT_AUTH_KEY")` /
    `..._ADMIN_..._` / `..._ERASE_..._`, falling back to `CORELINK_INTERNAL_AUTH_KEY`.
    The mint route uses it: `internal_pat.rs:698` →
    `resolve_internal_auth_key("CORELINK_PAT_MINT_AUTH_KEY")`.
- **But the Durable Object's `container.start({ env })` block forwards NONE of the
  four dedicated keys** — it forwards only `CORELINK_INTERNAL_AUTH_KEY`
  (`worker/src/durable_object.ts:544`). Verified: `grep -c` for
  `PAT_MINT_AUTH_KEY|RUNNER_MINT_AUTH_KEY|ADMIN_AUTH_KEY|ERASE_AUTH_KEY` in
  `durable_object.ts` = **0**. (Contrast: `FABRIC_INTROSPECT_AUTH_KEY` and
  `BILLING_INGEST_AUTH_KEY` ARE forwarded at lines 586/597 — those two consumers
  are genuinely isolated; the four `/_internal/*`-family consumers are not.)
- Consequence on the container: `env::var("CORELINK_PAT_MINT_AUTH_KEY")` is always
  `Err` inside the container, so `resolve_internal_auth_key` always takes the
  shared-fallback branch (`admin.rs:417`). **Every container internal gate
  (`/_internal/pat/mint`, `/_internal/dsr/erase`, `/_internal/admin/*`, CAS-erase)
  verifies against the SHARED key, never a dedicated one** — the exact thing the
  split was built to prevent. A leak of the single shared `CORELINK_INTERNAL_AUTH_KEY`
  still unlocks mint + erase + admin on the container (the edge split is the only
  thing narrowing blast radius, and it is bypassable the moment anyone can reach the
  container port — or simply does not help, since the shared key is what the
  signup-worker already holds).

**Latent availability landmine (the part that bites operationally):** the Worker
edge forwards the RESOLVED consumer key to the container
(`index.ts:1622` `h.set("x-corelink-internal-auth", internalAuthKey)`). The moment
an operator follows the documented hardening and provisions a DEDICATED
`CORELINK_ERASE_AUTH_KEY` (≠ shared) as a Worker secret, the edge will forward that
dedicated value, but the container's erase gate still resolves to the SHARED key
(the dedicated one never reaches the container env) → constant-time compare fails →
**401 on every erase/DSR call.** Same for admin and for `pat_mint` (which would
break signup PAT provisioning + session/token-exchange + runner-mint + auth-rotate,
since they all present the resolved `pat_mint`/shared key to the container mint
route). The split therefore cannot actually be turned on without an outage — it is
security theater plus a tripwire.

**Reachability:** insider / operator-config (the isolation is silently absent today
because no env-contract check forwards the keys; `scripts/check-env-contract.py`
does not list them — verified `grep` = 0 hits). No direct anon exploit, because the
container internal port is not internet-reachable and the edge still gates on a
sized key. Severity is MEDIUM: it nullifies a shipped, documented security control
(red-team #3) and is a guaranteed self-inflicted outage on the intended hardening.

**Fix:** forward the four dedicated keys through `container.start({ env })` (mirror
the `FABRIC_INTROSPECT_AUTH_KEY` / `BILLING_INGEST_AUTH_KEY` lines) AND add them to
`scripts/check-env-contract.py` so a future container `env::var` read can't silently
no-op (this is the exact `ERASURE_SALT_KEY`-class gap the env-contract check was
created to catch). Until then, the CHANGELOG/docs claim of per-consumer isolation on
`/_internal/*` is false on the container plane and should be softened.

---

## Surfaces probed that HELD (negative results, auditable)

**Stripe webhook → runners-entitlement / tier seed (forged cap, cross-tenant via
metadata.tenant_id):** NOT exploitable. The seed reads `tenant_id` and the runner
cap from `data.object.metadata.tenant_id` / `data.object.plan.id`
(`handler.rs:247-262, 369-380`), but the entire body must pass HMAC-SHA256
signature verification first (`webhook.rs:41-78`, constant-time `ct_eq`, 300s replay
window, multi-`v1` rotation). An attacker cannot craft a body with arbitrary
`tenant_id`/cap without the `whsec`. The cap is also not attacker-chosen: it is
resolved from the trusted `RUNNER_PRICE_ENV_TABLE` (`main.rs:107-114`) keyed on the
live Stripe price id, not from the event — so even a legitimately-signed event for
your own tenant only yields the cap you actually bought. Status-gated
(`subscription_status_grants_access`, `handler.rs:385`) so a `past_due`/`unpaid`
event cannot seed. Held.

**`auth_rotate` cross-tenant PAT hijack (F-006 regression check):** still fixed.
`owner_tenant` is MANDATORY (`auth_rotate.ts:210-216`, hard 400 on absent/empty) and
enforced `=== oldRow.tenant_id` (`:254-261`, 403 otherwise). A holder of the
pat_mint/shared key cannot mint a fresh working credential for a PAT it does not
name correctly. The 404-before-403 ordering gives no existence/revocation oracle.
NOT regressed.

**`runner_revoke` cross-tenant DoS (F-007 check):** still fixed. `owner_tenant`
mandatory (`runner_mint.ts:264-267`) and the UPDATE is tenant-scoped
(`:280` `WHERE pat_id=?2 AND tenant_id=?3`). A leaked runner key cannot revoke
another tenant's primary PAT. NOT regressed.

**`runner_mint` privilege:** bounded by design. Scope is hard-capped to
`cas:rw`/`read-write` (`runner_mint.ts:61`, admin/owner refused 400), requires a
`runners_entitlement` row for the named `owner_tenant` (`:172-185`, 403 otherwise),
and mints through the single audited authority with the per-principal throttle. A
compromised runner-mint key can mint a `cas:rw` PAT for any RUNNERS-ENTITLED tenant —
but that IS the dispatcher's intended power, it cannot reach `admin`, and the
entitlement-row precondition limits the target set. Not an escalation beyond the
documented dispatcher trust model.

**ASK-2 billing ingest (`/internal/v1/billing/usage`) — forge/poison another
tenant's billing:** held. Dedicated `BILLING_INGEST_AUTH_KEY` (≥32, route NOT
mounted otherwise — `billing_ingest.rs:548-556`), auth gate runs on raw bytes
BEFORE JSON parse (`:459`), strict per-record validation (uuid tenant, `YYYY-MM`,
3-char region, 64-hex idem_key, deny_unknown_fields), all-or-nothing batch,
idempotent by `(tenant_id, idem_key)` (`STAGE_INSERT_SQL` ON CONFLICT DO NOTHING),
1024-record cap, 503 fail-CLOSED on backend fault. A holder of the ingest key COULD
stage usage rows attributed to an arbitrary `tenant_id` — but `runner_slot_seconds`
is explicitly NOT a Stripe-billable meter (concurrency-priced model, module
docstring `:6-12`); it feeds only dashboard/capacity reconciliation, so there is no
money-path poisoning. The dedicated key is genuinely isolated (forwarded at
`durable_object.ts:597`, unlike CP-1's keys). Held.

**`auth_introspect` (`/internal/v1/auth/introspect`):** held. Dedicated
`FABRIC_INTROSPECT_AUTH_KEY` + optional `_HUGR` consumer, multi-key gate OR-combined
without short-circuit (no consumer-identity timing oracle, `auth_introspect.rs:566-569`),
uniform `valid:false` with no tenant/reason oracle, fail-CLOSED 503 on any D1 fault
(never serves a guessed plan/cap). The runner cap/vCPU-h is read from the dedicated
`runners_entitlement` table, range-checked, fail-CLOSED on out-of-contract values.
Genuinely isolated (forwarded at `durable_object.ts:586/591`).

**`_system`/`_oci` DO routing — cross-tenant via sentinel:** held. `_system`/`_oci`
DO IDs are set by the Worker ONLY for fixed route kinds (internal/oci/health/the
mint-bearing exchange handlers) — never derived from a customer PAT. The data-plane
forward routes on the D1-resolved `tenant_id` (a UUID), and the path-spoof guard
(`index.ts:2122+`) rejects a URL tenant ≠ PAT tenant. A customer PAT cannot steer a
request into the `_system` DO. `CLIENT_TRUST_HEADERS` strip
(`index.ts:427-477`) removes any client-smuggled `x-corelink-tenant-id` /
`-internal-auth` / `-scope` / `-token-prefix` on EVERY forward (F-012 closed).

**DO lifecycle wedge (F-019/F-020 class) — wedge or double-start:** held. The
stale-`"starting"` self-heal (`durable_object.ts:434-453`, `STALE_STARTING_MS =
STARTUP_TIMEOUT_MS + 30s`) recovers a persisted dead start. The concurrent-start
guard is sound: `updateLifecycleState` sets `this.lifecycleState` SYNCHRONOUSLY
(`:887`) before the `await storage.put`, and `startContainer` re-checks
`status === "starting"` (`:489`) in the same microtask as the synchronous flip
(`:496-500`) with no intervening await — so a second concurrently-queued fetch sees
`"starting"` and defers to `waitForContainerReady` rather than double-calling
`container.start`. STALE-RUNNING self-heal (`:419-425`) covers the deploy-rotation
case. No persistent unauth wedge found post-F-020.

**Webhook fail-open on absent secret:** held. The Stripe webhook route is only
merged when `STRIPE_WEBHOOK_SECRET` is set (`main.rs:581`); absent → route NOT
mounted (`main.rs:722`), not an open endpoint.

---

## Summary

1 CONFIRMED finding (CP-1, MEDIUM). The control plane is otherwise solid: every
internal surface is fail-CLOSED, the Stripe HMAC + dedicated-key gates hold, the
F-006/F-007/F-012/F-019/F-020 fixes have NOT regressed, cross-tenant isolation via
the DO routing + trust-header strip holds, and the new runners-seed / ASK-2 ingest
add no money-path forgery. CP-1 is not a direct break (it fails closed) but it
silently nullifies the shipped red-team-#3 per-consumer isolation on the container
plane and is a guaranteed outage if the documented dedicated-key hardening is ever
turned on — worth fixing before that hardening is attempted.
