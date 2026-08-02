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
checkpoint_sha: "3397e91bca1044cf84948d7310f0d9aeedb82048"
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
  forwards it as a server-trust header (`worker/src/index.ts:348-378`).
- The declarer of the container's env contract: every operator secret the container reads is first a
  field on the Worker `Env` interface here (`worker/src/index.ts:214`), then materialized onto the
  container by the per-tenant DO's `container.start({ env })` forward — owned by
  [the Durable Object lifecycle](/planes/durable-object.md) concept. That DO forward now also carries the
  launch-checkout coupon id `STRIPE_LAUNCH_COUPON` alongside the `EMAIL_HASH_SALT` salt — unset ⇒ checkout
  falls back to `allow_promotion_codes=true`, zero regression; a var the container reads via `env::var`
  but neither declared on `Env` here nor forwarded by the DO would silently never reach the container.

# How it works
1. The exported handler is `baseHandler.fetch`, which resolves a request-id, handles CORS preflight,
   and matches the route before doing anything else (`worker/src/index.ts:1750-1774`).
2. `matchRoute` is an ordered first-match table mapping each URL to a `RouteKind` + tenant, with
   specificity ordering (signup/customer/onboarding before the generic `/v1/*` arm)
   (`worker/src/index.ts:653-965`).
3. Health routes short-circuit with no auth and no DO forward (`worker/src/index.ts:1776-1793`).
   The Artifact 1 `/v1/public/*` arm (the erasure-attestation verifier, `routeKind="public_attestation"`)
   is matched BEFORE the generic `/v1/*` PAT bucket and forwarded to the `_anonymous` DO → container as a
   pure pass-through with NO PAT gate and NO internal-auth (an erasure proof is publicly verifiable;
   client-forged `x-corelink-*` trust headers are still stripped) — matchRoute arm
   (`worker/src/index.ts:947-948`), forward arm (`worker/src/index.ts:2347-2375`).
3b. Three EXACT-path fabric/ingest carve-outs are matched BEFORE the generic `/v1/*` PAT arm and are pure
   pass-throughs to the `_system` DO → container (the container is the SOLE auth authority; the Worker
   applies NO edge PAT gate and forwards the caller's `x-corelink-internal-auth` unchanged):
   `/internal/v1/auth/introspect` (`worker/src/index.ts:847`) and the sibling
   `/internal/v1/auth/resolve-tenant` (`worker/src/index.ts:859`) — both share `routeKind="fabric_introspect"`
   and the same `FABRIC_INTROSPECT_AUTH_KEY` (+ optional `_HUGR`) gate; resolve-tenant lets a fabric consumer
   (githugr) map `clerk_org_id(=sub)→tenant_id` for isolation verification + per-tenant reads. The runner
   billing usage-push (`/internal/v1/billing/usage`, `routeKind="billing_ingest"`, `worker/src/index.ts:871`)
   is the same shape but gated by its own DEDICATED `BILLING_INGEST_AUTH_KEY`.
4. `extractAuth` fails CLOSED (503) when `PAT_SIGNING_KEY` is absent or decodes to < 32 bytes — the
   signing key is the sole possession gate for the native plane (`worker/src/index.ts:1079-1105`).
5. PAT validation is HMAC-SHA256 fast-reject (with rotation siblings) BEFORE any D1 round-trip, then a
   D1 lookup by `token_id` and an application-side expiry check (`worker/src/index.ts:1133-1291`). The
   expiry check honors the `expires_ms === 0` "never expires" sentinel
   (`row.expires_ms !== 0 && row.expires_ms <= now`), matching the container's `adapter_pat` SQL
   (`expires_ms = 0 OR expires_ms > now`) — so a no-TTL PAT is no longer a split-brain edge-reject that
   would pass in the container but die at the Worker (looking like a forged token). The `pat` D1 read is
   now served through the per-isolate PAT-verify cache (`verifyPatRowCached`,
   `worker/src/lib/pat_verify_cache.ts`), which owns the miss-path try/catch; a TRANSIENT D1 fault
   (network partition / DB unavailable) surfaces as the cache's `error` kind, which `extractAuth` maps to
   the distinct reason `d1_lookup_error` (`worker/src/index.ts:1271-1275`). The caller maps BOTH the config fault
   `signing_key_not_configured` (absent/short `PAT_SIGNING_KEY`) AND `d1_lookup_error` to a retryable
   `503` — a D1 infra hiccup is treated as "auth service unavailable", NOT a bad credential, so a
   transient outage can't masquerade as a 401 (which would trigger spurious CI failures, PAT rotation,
   and on-call chasing the wrong thing). Genuine bad/unknown PATs (`pat_not_found` / `pat_expired` /
   `invalid_*`) still fall through to `401` (`worker/src/index.ts:2629-2666`). NOTE: the
   `extractAuth` error-branch comment at `:1260-1262` MATCHES that behaviour — it states the caller maps
   `d1_lookup_error` to a 503 (transient, retryable; still fail-closed); the pre-cache inline `catch`
   previously lied ("for now we 401 to fail-closed"), a stale posture left over from before the H1
   caller-mapping fix landed.
6. `stripClientTrustHeaders` deletes every client-suppliable trust header on every forward, then the
   Worker re-sets its own verified values (`worker/src/index.ts:601-605`).
6b. The Worker is also the PRODUCER of the `first_cli_authed` onboarding signal. Immediately after the
   PAT resolves a real tenant and the path-spoof guard clears — and BEFORE quota/residency — a
   `GET /v1/users/me` (the CLI's first authenticated call) fires a fire-and-forget analytics event
   (`worker/src/index.ts:2715-2721`). It lives here rather than in the container because the container's
   single `D1_DATABASE_ID` points at the control plane while `analytics_events` lives only in
   `corelink-analytics-prod`, and its charter forbids `tokio::spawn` in `src/`. The write is dispatched
   over the `ANALYTICS_SVC` service binding (`worker/src/index.ts:246`) to the analytics Worker's
   `POST /v1/event` ingest — never the public hostname, which Cloudflare edge-rejects Worker→Worker with
   error 1014. Dedup is structural, not a round-trip: the event id is the deterministic
   `first_cli_authed:<tenant_id>` (`worker/src/lib/onboarding_events.ts:109-111`), so `analytics_events`'
   `PRIMARY KEY (id)` + ingest's `INSERT OR IGNORE` act as a once-per-tenant lock.
7. Per-tier quota (storage SUM + monthly request-count) runs after auth and before the DO forward —
   request-count fail-CLOSED; storage verb-aware (reads fail-open for availability, byte-adding writes
   fail-closed) (`worker/src/index.ts:2724-2885`).
7b. Data-residency (`primary_region`, for region fan-out) is resolved AFTER quota and BEFORE the DO
   forward through the three-tier cache `resolveTenantResidency` — **L1** per-isolate (5 s) → **L2** Workers
   KV (`tres:<tenant>`, 60 s) → **L3** D1 (`SELECT primary_region`) — replacing the former inline
   per-request D1-PRIMARY read so a far-from-D1 (e.g. SAM) caller no longer pays a synchronous round-trip
   for this near-immutable value (`worker/src/index.ts:2935-2939`,
   `worker/src/lib/tenant_residency_cache.ts:231-298`). It is FAIL-CLOSED: an unresolved region (a D1 fault
   with no cached fallback ⇒ the `RESIDENCY_UNRESOLVED` sentinel) returns `503 RESIDENCY_UNAVAILABLE`
   rather than IAD-leaking an EU tenant to US storage (`worker/src/index.ts:2940-2957`,
   `worker/src/lib/tenant_residency_cache.ts:284-291`); a `null` region (no tenant row / no pin) preserves
   the existing `primaryRegion === undefined` IAD-local fall-through (`worker/src/index.ts:2961`). A stale
   cached region can only MIS-ROUTE, never silently leak, because the container residency backstop
   (`residency.rs`) 409s any real cross-region mismatch.
8. The request is routed to the per-tenant DO via `idFromName(resolvedTenantId)`
   (`worker/src/index.ts:3073-3075`) and dispatched with `stub.fetch` (`worker/src/index.ts:3157`). A
   multi-region tenant may first take the LOCAL (this-region) `_system` container branch above this
   forward (`worker/src/index.ts:1923-1925`); a non-resident tenant falls through to the per-tenant DO
   derivation here.
9. The forwarded request is augmented: strip-then-set the trusted tenant-id, scope, token-prefix, and
   client-ip headers (`worker/src/index.ts:3078-3109`).
10. The whole handler is wrapped by `Sentry.withSentry`, inert until `SENTRY_DSN` is set and with
    `sendDefaultPii=false` (`worker/src/index.ts:3234-3242`); its `beforeSend`/`beforeSendTransaction`
    run `scrubSentryEvent` (`worker/src/sentry-scrub.ts`) over EVERY event before it leaves the Worker —
    not just sensitive header KEYS but message/exception bodies, breadcrumbs, and `extra`/`contexts`
    VALUES (CoreLink PATs, bearer/basic auth, Stripe `sk_`/`pk_`/`whsec_` keys, emails are
    `[REDACTED]`), closing the WP4 PII/secret-leak gap (`worker/src/index.ts:3243-3248`).

# Invariants
- Tenant isolation is structural: the DO id is derived solely from the PAT-resolved tenant, never the
  URL path segment (`worker/src/index.ts:3073-3075`).
- A client can never smuggle a server-trust header: the strip list is applied on every forward path
  before the Worker sets its own values (`worker/src/index.ts:525-594`). The team-RBAC role header
  `x-corelink-role` (migration 0074) is a member of that strip list, so a client copy is always deleted;
  the Worker is its SOLE setter and stamps the D1-resolved role only on the customer-plane forward
  (`worker/src/index.ts:593`, set at `worker/src/index.ts:2541`) — this is what lets the container gate
  OWNER-only account deletion, and owner/admin-only team-management (invite/remove), without trusting a
  client-supplied role.
- The signing-key gate is mandatory — a missing/short `PAT_SIGNING_KEY` is a 503, never a silent skip
  (`worker/src/index.ts:1079-1105`).
- The Worker forwards the D1-resolved `scope` as `x-corelink-scope` and is its sole setter
  (`worker/src/index.ts:3109`); for a find-only PAT (`pat.find_only = 1`) the forwarded value is narrowed
  to `find-missing` rather than the stored base `read-only` (ADR-0071, `worker/src/index.ts:1331`).
- On the customer (Clerk-session) forward the Worker sets `x-corelink-scope` from the D1-resolved team
  role, a THREE-way branch (not the old "viewer vs everyone-else"): `viewer` → `read-only`, `member` →
  `read-write`, `owner`/`admin` → `read-write billing` — so the `billing` capability (billing portal /
  cancel subscription / financial PII) is now OWNER/ADMIN-only and a plain `member` no longer clears the
  container's F-018 billing/PII gate; the Worker is the sole setter (`worker/src/index.ts:2528-2535`).
- A 404 from the DO is timing-padded to defeat cross-tenant enumeration
  (`worker/src/index.ts:3171-3179`).
- Data-residency is resolved FAIL-CLOSED before the DO forward: an unresolvable region (a D1 fault with
  no cached fallback) is a `503`, never an IAD-local fall-through that could place an EU tenant's bytes in
  US storage; only a resolved `null` (no row / no pin) falls through to IAD-local
  (`worker/src/index.ts:2940-2957`, `worker/src/lib/tenant_residency_cache.ts:284-291`).

# Gotchas
- Argon2id is NOT run in the Worker (cpu_ms budget) — possession on the native CAS/AC/Bazel/Turbo plane
  rests solely on the HMAC gate; adapter routes (cargo/brew/npm/pip/oci) re-verify in the container.
- OCI, the Stripe webhook, and the fabric-introspect / resolve-tenant / billing-ingest arms are pure
  pass-throughs: the Worker applies no edge PAT gate and the container is the sole auth authority for
  them. `/internal/v1/auth/introspect` (`worker/src/index.ts:847`) and `/internal/v1/auth/resolve-tenant`
  (`worker/src/index.ts:859`) share `routeKind="fabric_introspect"` + the `FABRIC_INTROSPECT_AUTH_KEY` gate;
  `/internal/v1/billing/usage` (`worker/src/index.ts:871`) uses its own `BILLING_INGEST_AUTH_KEY`.

# Citations
1. `worker/src/index.ts:1-20` — the architecture header documenting the Worker → DO → container topology.
2. `worker/src/index.ts:348-378` — the `AuthResult` carrying the D1-resolved scope (the Worker is its sole authority).
3. `worker/src/index.ts:525-594` — the `CLIENT_TRUST_HEADERS` strip list.
4. `worker/src/index.ts:601-605` — `stripClientTrustHeaders` (delete-then-set discipline).
5. `worker/src/index.ts:653-965` — the `matchRoute` ordered route table.
5b. `worker/src/index.ts:847` / `worker/src/index.ts:859` / `worker/src/index.ts:871` — the three exact-path pure-pass-through carve-outs: `/internal/v1/auth/introspect` + `/internal/v1/auth/resolve-tenant` (both `fabric_introspect`, `FABRIC_INTROSPECT_AUTH_KEY`) and `/internal/v1/billing/usage` (`billing_ingest`, `BILLING_INGEST_AUTH_KEY`).
6. `worker/src/index.ts:1079-1105` — `extractAuth` fail-CLOSED on absent/short `PAT_SIGNING_KEY`.
7. `worker/src/index.ts:1133-1291` — HMAC fast-reject + cached D1 lookup + expiry check (incl. the `expires_ms === 0` never-expires sentinel guard at `:1277`, and the cached PAT-row lookup `verifyPatRowCached` whose `error` kind becomes `d1_lookup_error` at `:1259-1263`).
7b. `worker/src/index.ts:2629-2666` — caller reason→status mapping: BOTH `signing_key_not_configured` (config fault) AND `d1_lookup_error` (transient D1 fault) → retryable `503`; every other reason (`pat_not_found` / `pat_expired` / `invalid_*`) → `401`.
8. `worker/src/index.ts:1750-1774` — the `baseHandler.fetch` entry, request-id, CORS, route match.
9. `worker/src/index.ts:1776-1793` — health short-circuit (no auth, no DO).
9b. `worker/src/index.ts:2715-2721` — the `first_cli_authed` emit site (`GET /v1/users/me`, after the PAT-resolved tenant clears the path-spoof guard), handed to `ctx.waitUntil`. The producer module is `worker/src/lib/onboarding_events.ts:121-198` (`emitFirstCliAuthed` returns `void` and swallows every error, so the emit can never add latency to — or fail — the response); every skip path is silent by design EXCEPT the half-configured one — `ANALYTICS_SVC` bound while `ANALYTICS_INGEST_KEY` is unset emits one `console.warn` instead of going dark (`worker/src/lib/onboarding_events.ts:143-148`); the deterministic id is built at `worker/src/lib/onboarding_events.ts:109-111`; the `ANALYTICS_SVC` service binding + `ANALYTICS_INGEST_KEY` are declared on `Env` at `worker/src/index.ts:246`.
10. `worker/src/index.ts:2724-2885` — per-tier quota enforcement after auth, before forward.
10a. `worker/src/index.ts:2850-2854` — `resolveTenantTierCached` three-tier tier resolution (L1 isolate → L2 KV `ttier:` 60 s → L3 D1 `getTierForTenant`) for the storage-quota header + request-count cap, replacing the former inline per-request tier D1-PRIMARY read (latency WP slice 2). FAIL-OPEN like the suspend gate: an unconfirmed (`d1Error`) result is returned unchanged and NEVER cached (a transient D1 fault can't pin a tenant to 'free' — F21 preserved), so a stale worker tier is bounded (`≤ 60 s`) and never authoritative (the DO quota FSM re-derives the hard caps). The cache module is `worker/src/lib/tenant_tier_cache.ts:182`.
10b. `worker/src/index.ts:2935-2939` — `resolveTenantResidency` three-tier residency resolution (L1 isolate → L2 KV `tres:` 60 s → L3 D1) replacing the former inline `SELECT primary_region` per-request D1-PRIMARY read; FAIL-CLOSED `503 RESIDENCY_UNAVAILABLE` on the `RESIDENCY_UNRESOLVED` sentinel (`worker/src/index.ts:2940-2957`), `null` region ⇒ IAD-local (`worker/src/index.ts:2961`). The cache module is `worker/src/lib/tenant_residency_cache.ts:231-298`, whose catch serves a cached region on a transient D1 fault but returns `RESIDENCY_UNRESOLVED` (never caches the error) when none is held (`worker/src/lib/tenant_residency_cache.ts:284-291`).
11. `worker/src/index.ts:3073-3075` — `idFromName(resolvedTenantId)` per-tenant DO routing.
12. `worker/src/index.ts:3078-3109` — the augmented forward (strip-then-set trust headers).
13. `worker/src/index.ts:3109` — forwarding the D1-resolved scope as `x-corelink-scope` (narrowed to `find-missing` for a find-only PAT, ADR-0071 `worker/src/index.ts:1331`).
14. `worker/src/index.ts:3157` — `stub.fetch` dispatch to the DO.
15. `worker/src/index.ts:3171-3179` — 404 timing-pad.
16. `worker/src/index.ts:3234-3242` — the `Sentry.withSentry` wrapper (inert until `SENTRY_DSN`; `sendDefaultPii=false`).
17. `worker/src/index.ts:3243-3248` — `beforeSend`/`beforeSendTransaction` → `scrubSentryEvent` (`worker/src/sentry-scrub.ts:101-188`): full-event PII/secret scrub (default-DENY sensitive keys + free-text secret/PII shapes), not just header keys.
