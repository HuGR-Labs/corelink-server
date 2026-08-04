---
type: "Plane"
title: "Cloudflare Worker edge plane"
description: "The HTTPS entry point: route table, PAT auth, server-trust header hygiene, per-tier quota, and forwarding to the per-tenant Durable Object."
source_files:
  - "worker/src/index.ts"
  - "worker/src/sentry-scrub.ts"
  - "worker/src/lib/tenant_residency_cache.ts"
  - "worker/src/lib/tenant_tier_cache.ts"
  - "worker/src/lib/onboarding_events.ts"
  - "worker/src/lib/internal_auth.ts"
  - "worker/src/lib/tenant_lookup.ts"
  - "worker/src/lib/session_exchange.ts"
  - "worker/src/lib/auth_rotate.ts"
  - "worker/src/lib/runner_mint.ts"
source_blobs:
  - "worker/src/index.ts@0c61b8a70e11a380b9ac25978ed9c7b599052e39"
  - "worker/src/sentry-scrub.ts@e9cd0d761cab3aaa83ea618f7d270e8b77adc316"
  - "worker/src/lib/tenant_residency_cache.ts@dc42b4123dae51e9887264168efcc5d8f6ab817b"
  - "worker/src/lib/tenant_tier_cache.ts@4a440e51a8199a471bcde1b80ed31a872aee2b53"
  - "worker/src/lib/onboarding_events.ts@13087a3f130b93729ade6e30e9568ea500344c22"
  - "worker/src/lib/internal_auth.ts@ebfd1530950819ba489c6230aac3c62f2988306e"
  - "worker/src/lib/tenant_lookup.ts@dd58aa2b958486c3f9803d850dc52f7327577537"
  - "worker/src/lib/session_exchange.ts@e376924a6f269a8d71615adee8880dbdafc4028a"
  - "worker/src/lib/auth_rotate.ts@28d5fe714a5bfcb3f6a41070b9342dba375ef15d"
  - "worker/src/lib/runner_mint.ts@3d2c2c16465710b7c2b777023314661eefdefa49"
checkpoint_sha: "4b3841bd99d7992dc2526004a0cf375e00041489"
provenance: "AUTHORED"
tags: ["planes", "worker", "edge", "auth", "routing"]
timestamp: "2026-06-26T00:00:00Z"
---

# Cloudflare Worker edge plane

The Worker is CoreLink's single public HTTPS entry point. Every request from the internet lands here
first: it resolves a request-id, matches the URL against the canonical route surface, authenticates the
Bearer PAT (HMAC fast-reject + D1 lookup), enforces per-tier storage and request quotas, strips every
client-suppliable "server-trust" header and re-establishes the trusted ones, then forwards to the
per-tenant `CoreLinkServer` Durable Object. It is deliberately thin — it owns auth, routing, and error
mapping, but never the cache logic, which lives in the Rust container plane. This is the plane that
makes tenant isolation structural (the DO id is derived solely from the PAT-resolved tenant) and that
keeps forged tokens cheap to reject before any expensive work.

# Role
- The sole HTTPS ingress and route dispatcher: the architecture header documents the
  `Internet → Worker → DO → container` topology (`worker/src/index.ts:1-20`).
- The auth authority for the native plane: it is the only layer that reads the D1 `pat.scope` and
  forwards it as a server-trust header (`worker/src/index.ts:351-381`).
- The declarer of the container's env contract: every operator secret the container reads is first a
  field on the Worker `Env` interface here (`worker/src/index.ts:216`), then materialized onto the
  container by the per-tenant DO's `container.start({ env })` forward — owned by
  [the Durable Object lifecycle](/planes/durable-object.md) concept. That DO forward now also carries the
  launch-checkout coupon id `STRIPE_LAUNCH_COUPON` alongside the `EMAIL_HASH_SALT` salt — unset ⇒ checkout
  falls back to `allow_promotion_codes=true`, zero regression; a var the container reads via `env::var`
  but neither declared on `Env` here nor forwarded by the DO would silently never reach the container.

# How it works
1. The exported handler is `baseHandler.fetch`, which resolves a request-id, handles CORS preflight,
   and matches the route before doing anything else (`worker/src/index.ts:1753-1794`).
2. `matchRoute` is an ordered first-match table mapping each URL to a `RouteKind` + tenant, with
   specificity ordering (signup/customer/onboarding before the generic `/v1/*` arm)
   (`worker/src/index.ts:656-968`).
3. Health routes short-circuit with no auth and no DO forward (`worker/src/index.ts:1796-1813`).
   The Artifact 1 `/v1/public/*` arm (the erasure-attestation verifier, `routeKind="public_attestation"`)
   is matched BEFORE the generic `/v1/*` PAT bucket and forwarded to the `_anonymous` DO → container as a
   pure pass-through with NO PAT gate and NO internal-auth (an erasure proof is publicly verifiable;
   client-forged `x-corelink-*` trust headers are still stripped) — matchRoute arm
   (`worker/src/index.ts:950-951`), forward arm (`worker/src/index.ts:2377-2405`).
3b. Three EXACT-path fabric/ingest carve-outs are matched BEFORE the generic `/v1/*` PAT arm and are pure
   pass-throughs to the `_system` DO → container (the container is the SOLE auth authority; the Worker
   applies NO edge PAT gate and forwards the caller's `x-corelink-internal-auth` unchanged):
   `/internal/v1/auth/introspect` (`worker/src/index.ts:850`) and the sibling
   `/internal/v1/auth/resolve-tenant` (`worker/src/index.ts:862`) — both share `routeKind="fabric_introspect"`
   and the same `FABRIC_INTROSPECT_AUTH_KEY` (+ optional `_HUGR`) gate; resolve-tenant lets a fabric consumer
   (githugr) map `clerk_org_id(=sub)→tenant_id` for isolation verification + per-tenant reads. The runner
   billing usage-push (`/internal/v1/billing/usage`, `routeKind="billing_ingest"`, `worker/src/index.ts:874`)
   is the same shape but gated by its own DEDICATED `BILLING_INGEST_AUTH_KEY`.
4. `extractAuth` fails CLOSED (503) when `PAT_SIGNING_KEY` is absent or decodes to < 32 bytes — the
   signing key is the sole possession gate for the native plane (`worker/src/index.ts:1082-1108`).
5. PAT validation is HMAC-SHA256 fast-reject (with rotation siblings) BEFORE any D1 round-trip, then a
   D1 lookup by `token_id` and an application-side expiry check (`worker/src/index.ts:1136-1294`). The
   expiry check honors the `expires_ms === 0` "never expires" sentinel
   (`row.expires_ms !== 0 && row.expires_ms <= now`), matching the container's `adapter_pat` SQL
   (`expires_ms = 0 OR expires_ms > now`) — so a no-TTL PAT is no longer a split-brain edge-reject that
   would pass in the container but die at the Worker (looking like a forged token). The `pat` D1 read is
   now served through the per-isolate PAT-verify cache (`verifyPatRowCached`,
   `worker/src/lib/pat_verify_cache.ts`), which owns the miss-path try/catch; a TRANSIENT D1 fault
   (network partition / DB unavailable) surfaces as the cache's `error` kind, which `extractAuth` maps to
   the distinct reason `d1_lookup_error` (`worker/src/index.ts:1274-1278`). The caller maps BOTH the config fault
   `signing_key_not_configured` (absent/short `PAT_SIGNING_KEY`) AND `d1_lookup_error` to a retryable
   `503` — a D1 infra hiccup is treated as "auth service unavailable", NOT a bad credential, so a
   transient outage can't masquerade as a 401 (which would trigger spurious CI failures, PAT rotation,
   and on-call chasing the wrong thing). Genuine bad/unknown PATs (`pat_not_found` / `pat_expired` /
   `invalid_*`) still fall through to `401` (`worker/src/index.ts:2659-2696`). NOTE: the
   `extractAuth` error-branch comment at `:1260-1262` MATCHES that behaviour — it states the caller maps
   `d1_lookup_error` to a 503 (transient, retryable; still fail-closed); the pre-cache inline `catch`
   previously lied ("for now we 401 to fail-closed"), a stale posture left over from before the H1
   caller-mapping fix landed.
6. `stripClientTrustHeaders` deletes every client-suppliable trust header on every forward, then the
   Worker re-sets its own verified values (`worker/src/index.ts:604-608`).
6b. The Worker is also the PRODUCER of the `first_cli_authed` onboarding signal. Immediately after the
   PAT resolves a real tenant and the path-spoof guard clears — and BEFORE quota/residency — a
   `GET /v1/users/me` (the CLI's first authenticated call) fires a fire-and-forget analytics event
   (`worker/src/index.ts:2745-2751`). It lives here rather than in the container because the container's
   single `D1_DATABASE_ID` points at the control plane while `analytics_events` lives only in
   `corelink-analytics-prod`, and its charter forbids `tokio::spawn` in `src/`. The write is dispatched
   over the `ANALYTICS_SVC` service binding (`worker/src/index.ts:253`) to the analytics Worker's
   `AnalyticsIngest.ingestServerEvent` RPC entrypoint — never the public hostname, which Cloudflare
   edge-rejects Worker→Worker with error 1014. Dedup is structural, not a round-trip: the event id is the
   deterministic `first_cli_authed:<tenant_id>` (`worker/src/lib/onboarding_events.ts:158-160`), so `analytics_events`'
   `PRIMARY KEY (id)` + ingest's `INSERT OR IGNORE` act as a once-per-tenant lock.
7. Per-tier quota (storage SUM + monthly request-count) runs after auth and before the DO forward —
   request-count fail-CLOSED; storage verb-aware (reads fail-open for availability, byte-adding writes
   fail-closed) (`worker/src/index.ts:2754-2984`).
7b. Data-residency (`primary_region`, for region fan-out) is resolved AFTER quota and BEFORE the DO
   forward through the three-tier cache `resolveTenantResidency` — **L1** per-isolate (5 s) → **L2** Workers
   KV (`tres:<tenant>`, 60 s) → **L3** D1 (`SELECT primary_region`) — replacing the former inline
   per-request D1-PRIMARY read so a far-from-D1 (e.g. SAM) caller no longer pays a synchronous round-trip
   for this near-immutable value (`worker/src/index.ts:3034-3039`,
   `worker/src/lib/tenant_residency_cache.ts:231-298`). It is FAIL-CLOSED: an unresolved region (a D1 fault
   with no cached fallback ⇒ the `RESIDENCY_UNRESOLVED` sentinel) returns `503 RESIDENCY_UNAVAILABLE`
   rather than IAD-leaking an EU tenant to US storage (`worker/src/index.ts:3041-3058`,
   `worker/src/lib/tenant_residency_cache.ts:284-291`); a `null` region (no tenant row / no pin) preserves
   the existing `primaryRegion === undefined` IAD-local fall-through (`worker/src/index.ts:3062`). A stale
   cached region can only MIS-ROUTE, never silently leak, because the container residency backstop
   (`residency.rs`) 409s any real cross-region mismatch.
7c. A response returned through **this PAT-gated native DO forward** carries a `Server-Timing` header
   splitting the request into `auth` (PAT verify, with `desc` naming the cache tier that served it) →
   `wdb` (the Worker-side reads between auth and the forward) → `origin` (the whole DO + container
   subrequest) → `total`. It is NOT emitted on any other exit — the OCI registry
   pass-through, the Clerk customer-plane forward, the `_system` container forwards, the
   public-attestation arm, the regional fan-out return, and the replication-coordinator, onboarding,
   billing-webhook, fabric-introspect and billing-ingest arms all return before this block. `wdb` is further broken into the four SERIAL awaits it
   is made of — `qmeter` (the monthly request-counter UPSERT), `qtier` (7 above), `qstor` (the storage
   `SUM(bytes_used)` read) and `qresid` (7b above) — because two of them are cache-backed and two are
   uncached D1, so the aggregate alone cannot say which read owns a slow request. **Every** phase that RAN
   is emitted even at `dur=0`, and only a phase that did NOT run is omitted, so "fast" and "did not happen"
   stay distinguishable on the wire; a Worker deployed before that contract suppressed `auth`/`wdb`/`origin`
   at 0 ms instead, which is why the 2026-08-04 probe saw `auth` on 3 of 30 responses. Durations are coarse
   by construction — a Worker's `Date.now()` advances only across I/O, so a CPU-only stretch reads 0 —
   which makes them directional attribution, never a profile
   (`worker/src/index.ts:3307-3336`). Two caveats a reader needs:
   **(a)** on a multi-region tenant the client sees the REGIONAL Worker's split; the primary Worker
   computes its own four sub-phases and discards them at the fan-out return, so a missing `qmeter` there
   means "*this* Worker did not meter", not "the request was not metered" (the primary already did — 7
   above). **(b)** `origin` is stamped before `applyTimingPad`, so on a 404 the header publishes the
   UN-padded upstream duration that the pad exists to conceal.
7d. Because the `qmeter` omission is keyed on the constant-time `x-corelink-fanout-from` match, an absent
   `qmeter` **while `qtier`/`qstor`/`qresid` are present** confirms to the caller that their marker equalled
   `CORELINK_INTERNAL_AUTH_KEY`. (The conjunction matters: `qmeter` is also absent for
   `_anonymous`/`_system`/`_pending`, where the other three are absent too.) Be precise about the stake —
   that variable is NOT a metering key, it is the SHARED internal-auth secret.
   Enumerated exhaustively (re-derive with `grep -rn 'CORELINK_INTERNAL_AUTH_KEY' worker/src/` — a count
   in prose is a claim, so make it re-checkable), it is read in three **roles**:
   **1. Inbound gate credential**, both fail-closed.
   **(a)** `resolveConsumerKey` (`worker/src/lib/internal_auth.ts:124-158`), per-consumer: serves the
   shared key only when that consumer's dedicated key is **unset**, and REFUSES fail-closed a key that is
   SET but under the floor. Two call sites — the `/_internal/*` edge gate and `requireConsumerAuth`
   (`worker/src/lib/internal_auth.ts:264`), the latter gating PAT-rotate
   (`worker/src/lib/auth_rotate.ts:167`) and runner mint + revoke (`worker/src/lib/runner_mint.ts:273`,
   `worker/src/lib/runner_mint.ts:599`) — so **four** gates.
   **(b)** `requireInternalAuth` (`worker/src/lib/internal_auth.ts:224-238`), shared-key **only**: no
   per-consumer key exists for these. Same `>= 32` floor, fails closed (503 unbound / 401 mismatch),
   gating githugr tenant-lookup (`worker/src/lib/tenant_lookup.ts:90`) and session exchange's internal
   arm (`worker/src/lib/session_exchange.ts:868`). **No per-consumer isolation**, so a shared-key holder
   passes them outright — the widest *inbound* exposure. Six inbound gates accept the shared key.
   **2. Outbound credential presented onward to the container.** `env.CORELINK_PAT_MINT_AUTH_KEY ??
   env.CORELINK_INTERNAL_AUTH_KEY` at `worker/src/lib/session_exchange.ts:505-510`,
   `worker/src/lib/session_exchange.ts:877`, `worker/src/lib/auth_rotate.ts:180` and
   `worker/src/lib/runner_mint.ts:286` — falls back on unset, but checks only `length === 0`: no `>= 32`
   floor, no fail-closed refusal. Separately the `onboarding` arm (`worker/src/index.ts:2113`, forwarded
   at `worker/src/index.ts:2159`) reads the shared key **directly, with no dedicated-key preference at
   all** — which matters for the ceiling: provisioning `CORELINK_PAT_MINT_AUTH_KEY` narrows the four `??`
   sites but can never narrow onboarding, so the shared key is the only credential that can authorize
   tier-select checkout to the container.
   **3. The fan-out marker itself** — this check (`worker/src/index.ts:2837-2839`), its sibling on the
   region branch (`worker/src/index.ts:3009-3011`), and the primary setting it on the service-binding
   forward (`worker/src/index.ts:3156-3157`).
   (`runner_mint`, `auth_rotate` and `session_exchange` appear in BOTH role 1 and role 2: strict inbound,
   loose outbound. That is why a per-FILE split kept producing wrong sentences — the split is per-ROLE.)
   `worker/src/durable_object.ts:585` propagates the key into the container env; that is plumbing, not a
   gate.
   Container-side, the erase and DSR-anchor gates use `resolve_dedicated_auth_key` with NO fallback at
   all (`docs/internal/secrets-checklist.md` finding H4), and that absence is itself a control. Which
   dedicated keys are actually provisioned is NOT determinable from this repo — CF secrets are
   write-only — so this is the ceiling, not an assertion about the live configuration. The signal is accepted anyway, for a reason that survives that blast radius: the
   compare is FULL-VALUE, so reaching this oracle requires already holding the complete key, and a key
   holder has a far more direct oracle in a 200-vs-401 on `/_internal/*` — so it grants no capability. The
   real close is ingress-stripping `x-corelink-fanout-from` at the public edge (which the edge does not do
   today); the primary sets that header only on the service-binding forward, which never traverses the
   public edge, so stripping it costs nothing and removes the forgery surface and this oracle together.
8. The request is routed to the per-tenant DO via `idFromName(resolvedTenantId)`
   (`worker/src/index.ts:3174-3176`) and dispatched with `stub.fetch` (`worker/src/index.ts:3258`). A
   multi-region tenant may first take the LOCAL (this-region) `_system` container branch above this
   forward (`worker/src/index.ts:1964-1966`); a non-resident tenant falls through to the per-tenant DO
   derivation here.
9. The forwarded request is augmented: strip-then-set the trusted tenant-id, scope, token-prefix, and
   client-ip headers (`worker/src/index.ts:3179-3210`).
10. The whole handler is wrapped by `Sentry.withSentry`, inert until `SENTRY_DSN` is set and with
    `sendDefaultPii=false` (`worker/src/index.ts:3363-3371`); its `beforeSend`/`beforeSendTransaction`
    run `scrubSentryEvent` (`worker/src/sentry-scrub.ts`) over EVERY event before it leaves the Worker —
    not just sensitive header KEYS but message/exception bodies, breadcrumbs, and `extra`/`contexts`
    VALUES (CoreLink PATs, bearer/basic auth, Stripe `sk_`/`pk_`/`whsec_` keys, emails are
    `[REDACTED]`), closing the WP4 PII/secret-leak gap (`worker/src/index.ts:3372-3377`).

# Invariants
- Tenant isolation is structural: the DO id is derived solely from the PAT-resolved tenant, never the
  URL path segment (`worker/src/index.ts:3174-3176`).
- A client can never smuggle a server-trust header: the strip list is applied on every forward path
  before the Worker sets its own values (`worker/src/index.ts:528-597`). The team-RBAC role header
  `x-corelink-role` (migration 0074) is a member of that strip list, so a client copy is always deleted;
  the Worker is its SOLE setter and stamps the D1-resolved role only on the customer-plane forward
  (`worker/src/index.ts:596`, set at `worker/src/index.ts:2571`) — this is what lets the container gate
  OWNER-only account deletion, and owner/admin-only team-management (invite/remove), without trusting a
  client-supplied role.
- The signing-key gate is mandatory — a missing/short `PAT_SIGNING_KEY` is a 503, never a silent skip
  (`worker/src/index.ts:1082-1108`).
- The Worker forwards the D1-resolved `scope` as `x-corelink-scope` and is its sole setter
  (`worker/src/index.ts:3210`); for a find-only PAT (`pat.find_only = 1`) the forwarded value is narrowed
  to `find-missing` rather than the stored base `read-only` (ADR-0071, `worker/src/index.ts:1334`).
- On the customer (Clerk-session) forward the Worker sets `x-corelink-scope` from the D1-resolved team
  role, a THREE-way branch (not the old "viewer vs everyone-else"): `viewer` → `read-only`, `member` →
  `read-write`, `owner`/`admin` → `read-write billing` — so the `billing` capability (billing portal /
  cancel subscription / financial PII) is now OWNER/ADMIN-only and a plain `member` no longer clears the
  container's F-018 billing/PII gate; the Worker is the sole setter (`worker/src/index.ts:2558-2565`).
- A 404 from the DO is timing-padded to defeat cross-tenant enumeration
  (`worker/src/index.ts:3272-3280`).
- Data-residency is resolved FAIL-CLOSED before the DO forward: an unresolvable region (a D1 fault with
  no cached fallback) is a `503`, never an IAD-local fall-through that could place an EU tenant's bytes in
  US storage; only a resolved `null` (no row / no pin) falls through to IAD-local
  (`worker/src/index.ts:3041-3058`, `worker/src/lib/tenant_residency_cache.ts:284-291`).

# Gotchas
- Argon2id is NOT run in the Worker (cpu_ms budget) — possession on the native CAS/AC/Bazel/Turbo plane
  rests solely on the HMAC gate; adapter routes (cargo/brew/npm/pip/oci) re-verify in the container.
- OCI, the Stripe webhook, and the fabric-introspect / resolve-tenant / billing-ingest arms are pure
  pass-throughs: the Worker applies no edge PAT gate and the container is the sole auth authority for
  them. `/internal/v1/auth/introspect` (`worker/src/index.ts:850`) and `/internal/v1/auth/resolve-tenant`
  (`worker/src/index.ts:862`) share `routeKind="fabric_introspect"` + the `FABRIC_INTROSPECT_AUTH_KEY` gate;
  `/internal/v1/billing/usage` (`worker/src/index.ts:874`) uses its own `BILLING_INGEST_AUTH_KEY`.

# Citations
1. `worker/src/index.ts:1-20` — the architecture header documenting the Worker → DO → container topology.
2. `worker/src/index.ts:351-381` — the `AuthResult` carrying the D1-resolved scope (the Worker is its sole authority).
3. `worker/src/index.ts:528-597` — the `CLIENT_TRUST_HEADERS` strip list.
4. `worker/src/index.ts:604-608` — `stripClientTrustHeaders` (delete-then-set discipline).
5. `worker/src/index.ts:656-968` — the `matchRoute` ordered route table.
5b. `worker/src/index.ts:850` / `worker/src/index.ts:862` / `worker/src/index.ts:874` — the three exact-path pure-pass-through carve-outs: `/internal/v1/auth/introspect` + `/internal/v1/auth/resolve-tenant` (both `fabric_introspect`, `FABRIC_INTROSPECT_AUTH_KEY`) and `/internal/v1/billing/usage` (`billing_ingest`, `BILLING_INGEST_AUTH_KEY`).
6. `worker/src/index.ts:1082-1108` — `extractAuth` fail-CLOSED on absent/short `PAT_SIGNING_KEY`.
7. `worker/src/index.ts:1136-1294` — HMAC fast-reject + cached D1 lookup + expiry check (incl. the `expires_ms === 0` never-expires sentinel guard at `:1277`, and the cached PAT-row lookup `verifyPatRowCached` whose `error` kind becomes `d1_lookup_error` at `:1259-1263`).
7b. `worker/src/index.ts:2659-2696` — caller reason→status mapping: BOTH `signing_key_not_configured` (config fault) AND `d1_lookup_error` (transient D1 fault) → retryable `503`; every other reason (`pat_not_found` / `pat_expired` / `invalid_*`) → `401`.
8. `worker/src/index.ts:1753-1794` — the `baseHandler.fetch` entry, request-id, CORS, route match.
9. `worker/src/index.ts:1796-1813` — health short-circuit (no auth, no DO).
9b. `worker/src/index.ts:2745-2751` — the `first_cli_authed` emit site (`GET /v1/users/me`, after the PAT-resolved tenant clears the path-spoof guard), handed to `ctx.waitUntil`. The producer module is `worker/src/lib/onboarding_events.ts:170-224` (`emitFirstCliAuthed` returns `void` and swallows every error, so the emit can never add latency to — or fail — the response); every skip path is silent by design, including the one where the binding resolved to the target's default `fetch` export because `entrypoint = "AnalyticsIngest"` is missing, which degrades to a no-op rather than a `TypeError`; the deterministic id is built at `worker/src/lib/onboarding_events.ts:158-160`; the `ANALYTICS_SVC` service binding is declared on `Env` at `worker/src/index.ts:253` and bound with `entrypoint = "AnalyticsIngest"` at `wrangler.toml` `[[env.prod.services]]`. There is NO caller-side ingest key any more: the RPC path is authenticated by the platform (`ingestServerEvent` passes `trusted = true` on that basis), which is what removed the `ANALYTICS_INGEST_KEY` operator prerequisite that kept this emit dark in prod.
10. `worker/src/index.ts:2754-2984` — per-tier quota enforcement after auth, before forward.
10a. `worker/src/index.ts:2945-2950` — `resolveTenantTierCached` three-tier tier resolution (L1 isolate → L2 KV `ttier:` 60 s → L3 D1 `getTierForTenant`) for the storage-quota header + request-count cap, replacing the former inline per-request tier D1-PRIMARY read (latency WP slice 2). FAIL-OPEN like the suspend gate: an unconfirmed (`d1Error`) result is returned unchanged and NEVER cached (a transient D1 fault can't pin a tenant to 'free' — F21 preserved), so a stale worker tier is bounded (`≤ 60 s`) and never authoritative (the DO quota FSM re-derives the hard caps). The cache module is `worker/src/lib/tenant_tier_cache.ts:182`.
10b. `worker/src/index.ts:3034-3039` — `resolveTenantResidency` three-tier residency resolution (L1 isolate → L2 KV `tres:` 60 s → L3 D1) replacing the former inline `SELECT primary_region` per-request D1-PRIMARY read; FAIL-CLOSED `503 RESIDENCY_UNAVAILABLE` on the `RESIDENCY_UNRESOLVED` sentinel (`worker/src/index.ts:3041-3058`), `null` region ⇒ IAD-local (`worker/src/index.ts:3062`). The cache module is `worker/src/lib/tenant_residency_cache.ts:231-298`, whose catch serves a cached region on a transient D1 fault but returns `RESIDENCY_UNRESOLVED` (never caches the error) when none is held (`worker/src/lib/tenant_residency_cache.ts:284-291`).
11. `worker/src/index.ts:3174-3176` — `idFromName(resolvedTenantId)` per-tenant DO routing.
12. `worker/src/index.ts:3179-3210` — the augmented forward (strip-then-set trust headers).
13. `worker/src/index.ts:3210` — forwarding the D1-resolved scope as `x-corelink-scope` (narrowed to `find-missing` for a find-only PAT, ADR-0071 `worker/src/index.ts:1334`).
14. `worker/src/index.ts:3258` — `stub.fetch` dispatch to the DO.
14a. `worker/src/index.ts:3307-3336` — the `Server-Timing` emission: `auth` / `wdb` / `origin` / `total`, with `wdb` decomposed into its four serial awaits (`qmeter` / `qtier` / `qstor` / `qresid`). A phase that ran is emitted even at `dur=0`; a phase that was skipped is omitted (the `-1` sentinel at `worker/src/index.ts:1780-1783`), so a cache-served phase is never mistaken for one that did not execute.
15. `worker/src/index.ts:3272-3280` — 404 timing-pad.
16. `worker/src/index.ts:3363-3371` — the `Sentry.withSentry` wrapper (inert until `SENTRY_DSN`; `sendDefaultPii=false`).
17. `worker/src/index.ts:3372-3377` — `beforeSend`/`beforeSendTransaction` → `scrubSentryEvent` (`worker/src/sentry-scrub.ts:101-188`): full-event PII/secret scrub (default-DENY sensitive keys + free-text secret/PII shapes), not just header keys.
