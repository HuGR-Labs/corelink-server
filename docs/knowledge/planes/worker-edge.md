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
source_blobs:
  - "worker/src/index.ts@dc5e60d15637947b7e2b5d7737ab8f293cadf8e8"
  - "worker/src/sentry-scrub.ts@e9cd0d761cab3aaa83ea618f7d270e8b77adc316"
  - "worker/src/lib/tenant_residency_cache.ts@dc42b4123dae51e9887264168efcc5d8f6ab817b"
  - "worker/src/lib/tenant_tier_cache.ts@4a440e51a8199a471bcde1b80ed31a872aee2b53"
  - "worker/src/lib/onboarding_events.ts@13087a3f130b93729ade6e30e9568ea500344c22"
  - "worker/src/lib/internal_auth.ts@0cf740cdb227a3a163c1a92c6272e93f65cad713"
checkpoint_sha: "5e1515dd3a19bb0302f03d83c22a39d96033ae48"
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
  forwards it as a server-trust header (`worker/src/index.ts:350-380`).
- The declarer of the container's env contract: every operator secret the container reads is first a
  field on the Worker `Env` interface here (`worker/src/index.ts:215`), then materialized onto the
  container by the per-tenant DO's `container.start({ env })` forward — owned by
  [the Durable Object lifecycle](/planes/durable-object.md) concept. That DO forward now also carries the
  launch-checkout coupon id `STRIPE_LAUNCH_COUPON` alongside the `EMAIL_HASH_SALT` salt — unset ⇒ checkout
  falls back to `allow_promotion_codes=true`, zero regression; a var the container reads via `env::var`
  but neither declared on `Env` here nor forwarded by the DO would silently never reach the container.

# How it works
1. The exported handler is `baseHandler.fetch`, which resolves a request-id, handles CORS preflight,
   and matches the route before doing anything else (`worker/src/index.ts:1752-1798`).
2. `matchRoute` is an ordered first-match table mapping each URL to a `RouteKind` + tenant, with
   specificity ordering (signup/customer/onboarding before the generic `/v1/*` arm)
   (`worker/src/index.ts:655-967`).
3. Health routes short-circuit with no auth and no DO forward (`worker/src/index.ts:1800-1817`).
   The Artifact 1 `/v1/public/*` arm (the erasure-attestation verifier, `routeKind="public_attestation"`)
   is matched BEFORE the generic `/v1/*` PAT bucket and forwarded to the `_anonymous` DO → container as a
   pure pass-through with NO PAT gate and NO internal-auth (an erasure proof is publicly verifiable;
   client-forged `x-corelink-*` trust headers are still stripped) — matchRoute arm
   (`worker/src/index.ts:949-950`), forward arm (`worker/src/index.ts:2381-2409`).
3b. Three EXACT-path fabric/ingest carve-outs are matched BEFORE the generic `/v1/*` PAT arm and are pure
   pass-throughs to the `_system` DO → container (the container is the SOLE auth authority; the Worker
   applies NO edge PAT gate and forwards the caller's `x-corelink-internal-auth` unchanged):
   `/internal/v1/auth/introspect` (`worker/src/index.ts:849`) and the sibling
   `/internal/v1/auth/resolve-tenant` (`worker/src/index.ts:861`) — both share `routeKind="fabric_introspect"`
   and the same `FABRIC_INTROSPECT_AUTH_KEY` (+ optional `_HUGR`) gate; resolve-tenant lets a fabric consumer
   (githugr) map `clerk_org_id(=sub)→tenant_id` for isolation verification + per-tenant reads. The runner
   billing usage-push (`/internal/v1/billing/usage`, `routeKind="billing_ingest"`, `worker/src/index.ts:873`)
   is the same shape but gated by its own DEDICATED `BILLING_INGEST_AUTH_KEY`.
4. `extractAuth` fails CLOSED (503) when `PAT_SIGNING_KEY` is absent or decodes to < 32 bytes — the
   signing key is the sole possession gate for the native plane (`worker/src/index.ts:1081-1107`).
5. PAT validation is HMAC-SHA256 fast-reject (with rotation siblings) BEFORE any D1 round-trip, then a
   D1 lookup by `token_id` and an application-side expiry check (`worker/src/index.ts:1135-1293`). The
   expiry check honors the `expires_ms === 0` "never expires" sentinel
   (`row.expires_ms !== 0 && row.expires_ms <= now`), matching the container's `adapter_pat` SQL
   (`expires_ms = 0 OR expires_ms > now`) — so a no-TTL PAT is no longer a split-brain edge-reject that
   would pass in the container but die at the Worker (looking like a forged token). The `pat` D1 read is
   now served through the per-isolate PAT-verify cache (`verifyPatRowCached`,
   `worker/src/lib/pat_verify_cache.ts`), which owns the miss-path try/catch; a TRANSIENT D1 fault
   (network partition / DB unavailable) surfaces as the cache's `error` kind, which `extractAuth` maps to
   the distinct reason `d1_lookup_error` (`worker/src/index.ts:1273-1277`). The caller maps BOTH the config fault
   `signing_key_not_configured` (absent/short `PAT_SIGNING_KEY`) AND `d1_lookup_error` to a retryable
   `503` — a D1 infra hiccup is treated as "auth service unavailable", NOT a bad credential, so a
   transient outage can't masquerade as a 401 (which would trigger spurious CI failures, PAT rotation,
   and on-call chasing the wrong thing). Genuine bad/unknown PATs (`pat_not_found` / `pat_expired` /
   `invalid_*`) still fall through to `401` (`worker/src/index.ts:2663-2700`). NOTE: the
   `extractAuth` error-branch comment at `:1274-1276` MATCHES that behaviour — it states the caller maps
   `d1_lookup_error` to a 503 (transient, retryable; still fail-closed); the pre-cache inline `catch`
   previously lied ("for now we 401 to fail-closed"), a stale posture left over from before the H1
   caller-mapping fix landed.
6. `stripClientTrustHeaders` deletes every client-suppliable trust header on every forward, then the
   Worker re-sets its own verified values (`worker/src/index.ts:603-607`).
6b. The Worker is also the PRODUCER of the `first_cli_authed` onboarding signal. Immediately after the
   PAT resolves a real tenant and the path-spoof guard clears — and BEFORE quota/residency — a
   `GET /v1/users/me` (the CLI's first authenticated call) fires a fire-and-forget analytics event
   (`worker/src/index.ts:2749-2755`). It lives here rather than in the container because the container's
   single `D1_DATABASE_ID` points at the control plane while `analytics_events` lives only in
   `corelink-analytics-prod`, and its charter forbids `tokio::spawn` in `src/`. The write is dispatched
   over the `ANALYTICS_SVC` service binding (`worker/src/index.ts:252`) to the analytics Worker's
   `AnalyticsIngest.ingestServerEvent` RPC entrypoint — never the public hostname, which Cloudflare
   edge-rejects Worker→Worker with error 1014. Dedup is structural, not a round-trip: the event id is the
   deterministic `first_cli_authed:<tenant_id>` (`worker/src/lib/onboarding_events.ts:158-160`), so `analytics_events`'
   `PRIMARY KEY (id)` + ingest's `INSERT OR IGNORE` act as a once-per-tenant lock.
7. Per-tier quota (storage SUM + monthly request-count) runs after auth and before the DO forward —
   request-count fail-CLOSED; storage verb-aware (reads fail-open for availability, byte-adding writes
   fail-closed) (`worker/src/index.ts:2758-2946`). The tier is resolved FIRST (cache-served) and the two
   UNCACHED statements — the monthly-counter UPSERT and the storage `SUM(bytes_used)` — then travel as a
   SINGLE `db.batch()` round trip (`runQuotaBatch` — see the [edge quota & tier serving](/launch/edge-quota-tier-serving.md) concept for that module; the call site is `worker/src/index.ts:2907-2911`) rather than two
   serial awaits: the 2026-08-04 prod measurement showed those two WERE the whole `wdb` phase (qmeter
   152/158/163 ms + qstor 120/126/130 ms = wdb 277/284/302 ms) and neither reads the other's result. The
   counter stays SYNCHRONOUS and un-deferred on purpose — its post-increment value IS the 429 decision,
   and `ctx.waitUntil` has no durability guarantee, so deferring it would silently under-count a billing
   counter. One consequence is named rather than hidden: a D1 batch is a transaction, so a fault rolls the
   increment back too — an UNDER-count, the direction the fail-open design already tolerates, never an
   over-count.
7b. Data-residency (`primary_region`, for region fan-out) is resolved AFTER quota and BEFORE the DO
   forward through the three-tier cache `resolveTenantResidency` — **L1** per-isolate (5 s) → **L2** Workers
   KV (`tres:<tenant>`, 60 s) → **L3** D1 (`SELECT primary_region`) — replacing the former inline
   per-request D1-PRIMARY read so a far-from-D1 (e.g. SAM) caller no longer pays a synchronous round-trip
   for this near-immutable value (`worker/src/index.ts:2995-3000`,
   `worker/src/lib/tenant_residency_cache.ts:231-298`). It is FAIL-CLOSED: an unresolved region (a D1 fault
   with no cached fallback ⇒ the `RESIDENCY_UNRESOLVED` sentinel) returns `503 RESIDENCY_UNAVAILABLE`
   rather than IAD-leaking an EU tenant to US storage (`worker/src/index.ts:3002-3019`,
   `worker/src/lib/tenant_residency_cache.ts:284-291`); a `null` region (no tenant row / no pin) preserves
   the existing `primaryRegion === undefined` IAD-local fall-through (`worker/src/index.ts:3023`). A stale
   cached region can only MIS-ROUTE, never silently leak, because the container residency backstop
   (`residency.rs`) 409s any real cross-region mismatch.
7c. A response returned through **this PAT-gated native DO forward** carries a `Server-Timing` header
   splitting the request into `auth` (PAT verify, with `desc` naming the cache tier that served it) →
   `wdb` (the Worker-side reads between auth and the forward) → `origin` (the whole DO + container
   subrequest) → `total`. It is NOT emitted on any other exit — the OCI registry
   pass-through, the Clerk customer-plane forward, the `_system` container forwards, the
   public-attestation arm, the regional fan-out return, and the replication-coordinator, onboarding,
   billing-webhook, fabric-introspect and billing-ingest arms all return before this block. `wdb` is further broken into the awaits it
   is made of — `qtier` (7 above), `qbatch` (the ONE D1 round trip carrying both uncached quota
   statements, the monthly-counter UPSERT and the storage `SUM(bytes_used)`) and `qresid` (7b above) —
   because the cache-backed phases and the D1 round trip cannot be told apart in the aggregate, so it
   alone cannot say which read owns a slow request. `qbatch` is what the earlier `qmeter` + `qstor` pair
   became when the two serial round trips were merged; those two names are retired because the round
   trips they measured no longer exist. **Every** phase that RAN
   is emitted even at `dur=0`, and only a phase that did NOT run is omitted, so "fast" and "did not happen"
   stay distinguishable on the wire; a Worker deployed before that contract suppressed `auth`/`wdb`/`origin`
   at 0 ms instead, which is why the 2026-08-04 probe saw `auth` on 3 of 30 responses. Durations are coarse
   by construction — a Worker's `Date.now()` advances only across I/O, so a CPU-only stretch reads 0 —
   which makes them directional attribution, never a profile
   (`worker/src/index.ts:3268-3298`). Two caveats a reader needs:
   **(a)** on a multi-region tenant the client sees the REGIONAL Worker's split; the primary Worker
   computes its own sub-phases and discards them at the fan-out return, so the regional `qbatch` does not
   include a metering statement — that does NOT mean the request was not metered (the primary already
   did — 7 above). **(b)** `origin` is stamped before `applyTimingPad`, so on a 404 the header publishes the
   UN-padded upstream duration that the pad exists to conceal.
7d. **RETIRED by the `qbatch` merge, kept for the reasoning.** While `qmeter` was its own phase, its
   omission was keyed on the constant-time `x-corelink-fanout-from` match, so an absent `qmeter` **while
   `qtier`/`qstor`/`qresid` were present** confirmed to the caller that their marker equalled
   `CORELINK_INTERNAL_AUTH_KEY`. `qbatch` does not say which statements were in the batch, so that
   confirmation is gone; what remains is far narrower (`qbatch` is omitted only when BOTH statements are
   skipped — a fan-out from an unlimited-STORAGE tenant, i.e. enterprise). Be precise about the stake —
   that variable is NOT a metering key, it is the SHARED internal-auth secret, and the fallback that
   reaches it fires only when a consumer's dedicated key is **unset**: a key that is SET but under the
   length floor is REFUSED fail-closed (`worker/src/lib/internal_auth.ts:172-197`), precisely so a
   misconfiguration cannot silently widen the blast radius. The full picture — three roles, six inbound
   gates of which two carry no per-consumer isolation at all, and the `onboarding`/tier-select-checkout
   arm that reads the shared key with NO dedicated-key preference and therefore can never be narrowed by
   provisioning — is enumerated in the `internal_auth.ts` module header, together with the grep that
   re-derives the enumeration. It lives with the code because it is a property of the auth surface, not
   of this phase. The signal is accepted anyway, for a reason that survives that blast radius: the
   compare is FULL-VALUE, so reaching this oracle requires already holding the complete key, and a key
   holder has a far more direct oracle in a 200-vs-401 on `/_internal/*` — so it grants no capability. The
   real close is ingress-stripping `x-corelink-fanout-from` at the public edge (which the edge does not do
   today); the primary sets that header only on the service-binding forward, which never traverses the
   public edge, so stripping it costs nothing and removes the forgery surface and this oracle together.
8. The request is routed to the per-tenant DO via `idFromName(resolvedTenantId)`
   (`worker/src/index.ts:3135-3137`) and dispatched with `stub.fetch` (`worker/src/index.ts:3219`). A
   multi-region tenant may first take the LOCAL (this-region) `_system` container branch above this
   forward (`worker/src/index.ts:1968-1970`); a non-resident tenant falls through to the per-tenant DO
   derivation here.
9. The forwarded request is augmented: strip-then-set the trusted tenant-id, scope, token-prefix, and
   client-ip headers (`worker/src/index.ts:3140-3171`).
10. The whole handler is wrapped by `Sentry.withSentry`, inert until `SENTRY_DSN` is set and with
    `sendDefaultPii=false` (`worker/src/index.ts:3325-3333`); its `beforeSend`/`beforeSendTransaction`
    run `scrubSentryEvent` (`worker/src/sentry-scrub.ts`) over EVERY event before it leaves the Worker —
    not just sensitive header KEYS but message/exception bodies, breadcrumbs, and `extra`/`contexts`
    VALUES (CoreLink PATs, bearer/basic auth, Stripe `sk_`/`pk_`/`whsec_` keys, emails are
    `[REDACTED]`), closing the WP4 PII/secret-leak gap (`worker/src/index.ts:3334-3339`).

# Invariants
- Tenant isolation is structural: the DO id is derived solely from the PAT-resolved tenant, never the
  URL path segment (`worker/src/index.ts:3135-3137`).
- A client can never smuggle a server-trust header: the strip list is applied on every forward path
  before the Worker sets its own values (`worker/src/index.ts:527-596`). The team-RBAC role header
  `x-corelink-role` (migration 0074) is a member of that strip list, so a client copy is always deleted;
  the Worker is its SOLE setter and stamps the D1-resolved role only on the customer-plane forward
  (`worker/src/index.ts:595`, set at `worker/src/index.ts:2575`) — this is what lets the container gate
  OWNER-only account deletion, and owner/admin-only team-management (invite/remove), without trusting a
  client-supplied role.
- The signing-key gate is mandatory — a missing/short `PAT_SIGNING_KEY` is a 503, never a silent skip
  (`worker/src/index.ts:1081-1107`).
- The Worker forwards the D1-resolved `scope` as `x-corelink-scope` and is its sole setter
  (`worker/src/index.ts:3171`); for a find-only PAT (`pat.find_only = 1`) the forwarded value is narrowed
  to `find-missing` rather than the stored base `read-only` (ADR-0071, `worker/src/index.ts:1333`).
- On the customer (Clerk-session) forward the Worker sets `x-corelink-scope` from the D1-resolved team
  role, a THREE-way branch (not the old "viewer vs everyone-else"): `viewer` → `read-only`, `member` →
  `read-write`, `owner`/`admin` → `read-write billing` — so the `billing` capability (billing portal /
  cancel subscription / financial PII) is now OWNER/ADMIN-only and a plain `member` no longer clears the
  container's F-018 billing/PII gate; the Worker is the sole setter (`worker/src/index.ts:2562-2569`).
- A 404 from the DO is timing-padded to defeat cross-tenant enumeration
  (`worker/src/index.ts:3233-3241`).
- Data-residency is resolved FAIL-CLOSED before the DO forward: an unresolvable region (a D1 fault with
  no cached fallback) is a `503`, never an IAD-local fall-through that could place an EU tenant's bytes in
  US storage; only a resolved `null` (no row / no pin) falls through to IAD-local
  (`worker/src/index.ts:3002-3019`, `worker/src/lib/tenant_residency_cache.ts:284-291`).

# Gotchas
- Argon2id is NOT run in the Worker (cpu_ms budget) — possession on the native CAS/AC/Bazel/Turbo plane
  rests solely on the HMAC gate; adapter routes (cargo/brew/npm/pip/oci) re-verify in the container.
- OCI, the Stripe webhook, and the fabric-introspect / resolve-tenant / billing-ingest arms are pure
  pass-throughs: the Worker applies no edge PAT gate and the container is the sole auth authority for
  them. `/internal/v1/auth/introspect` (`worker/src/index.ts:849`) and `/internal/v1/auth/resolve-tenant`
  (`worker/src/index.ts:861`) share `routeKind="fabric_introspect"` + the `FABRIC_INTROSPECT_AUTH_KEY` gate;
  `/internal/v1/billing/usage` (`worker/src/index.ts:873`) uses its own `BILLING_INGEST_AUTH_KEY`.

# Citations
1. `worker/src/index.ts:1-20` — the architecture header documenting the Worker → DO → container topology.
2. `worker/src/index.ts:350-380` — the `AuthResult` carrying the D1-resolved scope (the Worker is its sole authority).
3. `worker/src/index.ts:527-596` — the `CLIENT_TRUST_HEADERS` strip list.
4. `worker/src/index.ts:603-607` — `stripClientTrustHeaders` (delete-then-set discipline).
5. `worker/src/index.ts:655-967` — the `matchRoute` ordered route table.
5b. `worker/src/index.ts:849` / `worker/src/index.ts:861` / `worker/src/index.ts:873` — the three exact-path pure-pass-through carve-outs: `/internal/v1/auth/introspect` + `/internal/v1/auth/resolve-tenant` (both `fabric_introspect`, `FABRIC_INTROSPECT_AUTH_KEY`) and `/internal/v1/billing/usage` (`billing_ingest`, `BILLING_INGEST_AUTH_KEY`).
6. `worker/src/index.ts:1081-1107` — `extractAuth` fail-CLOSED on absent/short `PAT_SIGNING_KEY`.
7. `worker/src/index.ts:1135-1293` — HMAC fast-reject + cached D1 lookup + expiry check (incl. the `expires_ms === 0` never-expires sentinel guard at `:1291`, and the cached PAT-row lookup `verifyPatRowCached` whose `error` kind becomes `d1_lookup_error` at `:1273-1277`).
7b. `worker/src/index.ts:2663-2700` — caller reason→status mapping: BOTH `signing_key_not_configured` (config fault) AND `d1_lookup_error` (transient D1 fault) → retryable `503`; the distinct `tenant_suspended` reason → `403` (an authorization denial, `:2690-2695`); and only then the 401 fall-through for a genuinely bad credential (`pat_not_found` / `pat_expired` / `invalid_*`). The mapping is THREE-way, not two-way — an earlier draft of this line said "every other reason → 401" while citing a range that contains the 403 arm.
8. `worker/src/index.ts:1752-1798` — the `baseHandler.fetch` entry, request-id, CORS, route match.
9. `worker/src/index.ts:1800-1817` — health short-circuit (no auth, no DO).
9b. `worker/src/index.ts:2749-2755` — the `first_cli_authed` emit site (`GET /v1/users/me`, after the PAT-resolved tenant clears the path-spoof guard), handed to `ctx.waitUntil`. The producer module is `worker/src/lib/onboarding_events.ts:170-224` (`emitFirstCliAuthed` returns `void` and swallows every error, so the emit can never add latency to — or fail — the response); every skip path is silent by design, including the one where the binding resolved to the target's default `fetch` export because `entrypoint = "AnalyticsIngest"` is missing, which degrades to a no-op rather than a `TypeError`; the deterministic id is built at `worker/src/lib/onboarding_events.ts:158-160`; the `ANALYTICS_SVC` service binding is declared on `Env` at `worker/src/index.ts:252` and bound with `entrypoint = "AnalyticsIngest"` at `wrangler.toml` `[[env.prod.services]]`. There is NO caller-side ingest key any more: the RPC path is authenticated by the platform (`ingestServerEvent` passes `trusted = true` on that basis), which is what removed the `ANALYTICS_INGEST_KEY` operator prerequisite that kept this emit dark in prod.
10. `worker/src/index.ts:2758-2946` — per-tier quota enforcement after auth, before forward.
10a. `worker/src/index.ts:2861-2866` — `resolveTenantTierCached` three-tier tier resolution (L1 isolate → L2 KV `ttier:` 60 s → L3 D1 `getTierForTenant`) for the storage-quota header + request-count cap, replacing the former inline per-request tier D1-PRIMARY read (latency WP slice 2). FAIL-OPEN like the suspend gate: an unconfirmed (`d1Error`) result is returned unchanged and NEVER cached (a transient D1 fault can't pin a tenant to 'free' — F21 preserved), so a stale worker tier is bounded (`≤ 60 s`) and never authoritative (the DO quota FSM re-derives the hard caps). The cache module is `worker/src/lib/tenant_tier_cache.ts:182`.
10b. `worker/src/index.ts:2995-3000` — `resolveTenantResidency` three-tier residency resolution (L1 isolate → L2 KV `tres:` 60 s → L3 D1) replacing the former inline `SELECT primary_region` per-request D1-PRIMARY read; FAIL-CLOSED `503 RESIDENCY_UNAVAILABLE` on the `RESIDENCY_UNRESOLVED` sentinel (`worker/src/index.ts:3002-3019`), `null` region ⇒ IAD-local (`worker/src/index.ts:3023`). The cache module is `worker/src/lib/tenant_residency_cache.ts:231-298`, whose catch serves a cached region on a transient D1 fault but returns `RESIDENCY_UNRESOLVED` (never caches the error) when none is held (`worker/src/lib/tenant_residency_cache.ts:284-291`).
11. `worker/src/index.ts:3135-3137` — `idFromName(resolvedTenantId)` per-tenant DO routing.
12. `worker/src/index.ts:3140-3171` — the augmented forward (strip-then-set trust headers).
13. `worker/src/index.ts:3171` — forwarding the D1-resolved scope as `x-corelink-scope` (narrowed to `find-missing` for a find-only PAT, ADR-0071 `worker/src/index.ts:1333`).
14. `worker/src/index.ts:3219` — `stub.fetch` dispatch to the DO.
14a. `worker/src/index.ts:3268-3298` — the `Server-Timing` emission: `auth` / `wdb` / `origin` / `total`, with `wdb` decomposed into `qtier` / `qbatch` / `qresid` (`qbatch` = the one D1 round trip carrying both quota statements; it replaced the retired `qmeter` + `qstor` pair when those two serial round trips were merged). A phase that ran is emitted even at `dur=0`; a phase that was skipped is omitted (the `-1` sentinel at `worker/src/index.ts:1785-1787`), so a cache-served phase is never mistaken for one that did not execute.
15. `worker/src/index.ts:3233-3241` — 404 timing-pad.
16. `worker/src/index.ts:3325-3333` — the `Sentry.withSentry` wrapper (inert until `SENTRY_DSN`; `sendDefaultPii=false`).
17. `worker/src/index.ts:3334-3339` — `beforeSend`/`beforeSendTransaction` → `scrubSentryEvent` (`worker/src/sentry-scrub.ts:101-188`): full-event PII/secret scrub (default-DENY sensitive keys + free-text secret/PII shapes), not just header keys.
