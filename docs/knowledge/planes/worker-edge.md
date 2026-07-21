---
type: "Plane"
title: "Cloudflare Worker edge plane"
description: "The HTTPS entry point: route table, PAT auth, server-trust header hygiene, per-tier quota, and forwarding to the per-tenant Durable Object."
source_files:
  - "worker/src/index.ts"
  - "worker/src/sentry-scrub.ts"
  - "worker/src/lib/tenant_residency_cache.ts"
checkpoint_sha: "766ee7ac85575b479af484b9e8189400cc35f917"
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
  forwards it as a server-trust header (`worker/src/index.ts:336-366`).
- The declarer of the container's env contract: every operator secret the container reads is first a
  field on the Worker `Env` interface here (`worker/src/index.ts:213`), then materialized onto the
  container by the per-tenant DO's `container.start({ env })` forward — owned by
  [the Durable Object lifecycle](/planes/durable-object.md) concept. That DO forward now also carries the
  launch-checkout coupon id `STRIPE_LAUNCH_COUPON` alongside the `EMAIL_HASH_SALT` salt — unset ⇒ checkout
  falls back to `allow_promotion_codes=true`, zero regression; a var the container reads via `env::var`
  but neither declared on `Env` here nor forwarded by the DO would silently never reach the container.

# How it works
1. The exported handler is `baseHandler.fetch`, which resolves a request-id, handles CORS preflight,
   and matches the route before doing anything else (`worker/src/index.ts:1737-1761`).
2. `matchRoute` is an ordered first-match table mapping each URL to a `RouteKind` + tenant, with
   specificity ordering (signup/customer/onboarding before the generic `/v1/*` arm)
   (`worker/src/index.ts:641-952`).
3. Health routes short-circuit with no auth and no DO forward (`worker/src/index.ts:1763-1780`).
   The Artifact 1 `/v1/public/*` arm (the erasure-attestation verifier, `routeKind="public_attestation"`)
   is matched BEFORE the generic `/v1/*` PAT bucket and forwarded to the `_anonymous` DO → container as a
   pure pass-through with NO PAT gate and NO internal-auth (an erasure proof is publicly verifiable;
   client-forged `x-corelink-*` trust headers are still stripped) — matchRoute arm
   (`worker/src/index.ts:934-935`), forward arm (`worker/src/index.ts:2334-2362`).
3b. Three EXACT-path fabric/ingest carve-outs are matched BEFORE the generic `/v1/*` PAT arm and are pure
   pass-throughs to the `_system` DO → container (the container is the SOLE auth authority; the Worker
   applies NO edge PAT gate and forwards the caller's `x-corelink-internal-auth` unchanged):
   `/internal/v1/auth/introspect` (`worker/src/index.ts:834`) and the sibling
   `/internal/v1/auth/resolve-tenant` (`worker/src/index.ts:846`) — both share `routeKind="fabric_introspect"`
   and the same `FABRIC_INTROSPECT_AUTH_KEY` (+ optional `_HUGR`) gate; resolve-tenant lets a fabric consumer
   (githugr) map `clerk_org_id(=sub)→tenant_id` for isolation verification + per-tenant reads. The runner
   billing usage-push (`/internal/v1/billing/usage`, `routeKind="billing_ingest"`, `worker/src/index.ts:858`)
   is the same shape but gated by its own DEDICATED `BILLING_INGEST_AUTH_KEY`.
4. `extractAuth` fails CLOSED (503) when `PAT_SIGNING_KEY` is absent or decodes to < 32 bytes — the
   signing key is the sole possession gate for the native plane (`worker/src/index.ts:1066-1092`).
5. PAT validation is HMAC-SHA256 fast-reject (with rotation siblings) BEFORE any D1 round-trip, then a
   D1 lookup by `token_id` and an application-side expiry check (`worker/src/index.ts:1120-1278`). The
   expiry check honors the `expires_ms === 0` "never expires" sentinel
   (`row.expires_ms !== 0 && row.expires_ms <= now`), matching the container's `adapter_pat` SQL
   (`expires_ms = 0 OR expires_ms > now`) — so a no-TTL PAT is no longer a split-brain edge-reject that
   would pass in the container but die at the Worker (looking like a forged token). The `pat` D1 read is
   now served through the per-isolate PAT-verify cache (`verifyPatRowCached`,
   `worker/src/lib/pat_verify_cache.ts`), which owns the miss-path try/catch; a TRANSIENT D1 fault
   (network partition / DB unavailable) surfaces as the cache's `error` kind, which `extractAuth` maps to
   the distinct reason `d1_lookup_error` (`worker/src/index.ts:1258-1262`). The caller maps BOTH the config fault
   `signing_key_not_configured` (absent/short `PAT_SIGNING_KEY`) AND `d1_lookup_error` to a retryable
   `503` — a D1 infra hiccup is treated as "auth service unavailable", NOT a bad credential, so a
   transient outage can't masquerade as a 401 (which would trigger spurious CI failures, PAT rotation,
   and on-call chasing the wrong thing). Genuine bad/unknown PATs (`pat_not_found` / `pat_expired` /
   `invalid_*`) still fall through to `401` (`worker/src/index.ts:2616-2653`). NOTE: the
   `extractAuth` error-branch comment at `:1248-1250` MATCHES that behaviour — it states the caller maps
   `d1_lookup_error` to a 503 (transient, retryable; still fail-closed); the pre-cache inline `catch`
   previously lied ("for now we 401 to fail-closed"), a stale posture left over from before the H1
   caller-mapping fix landed.
6. `stripClientTrustHeaders` deletes every client-suppliable trust header on every forward, then the
   Worker re-sets its own verified values (`worker/src/index.ts:589-593`).
7. Per-tier quota (storage SUM + monthly request-count) runs after auth and before the DO forward —
   request-count fail-CLOSED; storage verb-aware (reads fail-open for availability, byte-adding writes
   fail-closed) (`worker/src/index.ts:2686-2838`).
7b. Data-residency (`primary_region`, for region fan-out) is resolved AFTER quota and BEFORE the DO
   forward through the three-tier cache `resolveTenantResidency` — **L1** per-isolate (5 s) → **L2** Workers
   KV (`tres:<tenant>`, 60 s) → **L3** D1 (`SELECT primary_region`) — replacing the former inline
   per-request D1-PRIMARY read so a far-from-D1 (e.g. SAM) caller no longer pays a synchronous round-trip
   for this near-immutable value (`worker/src/index.ts:2888-2892`,
   `worker/src/lib/tenant_residency_cache.ts:231-298`). It is FAIL-CLOSED: an unresolved region (a D1 fault
   with no cached fallback ⇒ the `RESIDENCY_UNRESOLVED` sentinel) returns `503 RESIDENCY_UNAVAILABLE`
   rather than IAD-leaking an EU tenant to US storage (`worker/src/index.ts:2893-2910`,
   `worker/src/lib/tenant_residency_cache.ts:284-291`); a `null` region (no tenant row / no pin) preserves
   the existing `primaryRegion === undefined` IAD-local fall-through (`worker/src/index.ts:2914`). A stale
   cached region can only MIS-ROUTE, never silently leak, because the container residency backstop
   (`residency.rs`) 409s any real cross-region mismatch.
8. The request is routed to the per-tenant DO via `idFromName(resolvedTenantId)`
   (`worker/src/index.ts:3026-3028`) and dispatched with `stub.fetch` (`worker/src/index.ts:3110`). A
   multi-region tenant may first take the LOCAL (this-region) `_system` container branch above this
   forward (`worker/src/index.ts:1910-1912`); a non-resident tenant falls through to the per-tenant DO
   derivation here.
9. The forwarded request is augmented: strip-then-set the trusted tenant-id, scope, token-prefix, and
   client-ip headers (`worker/src/index.ts:3031-3062`).
10. The whole handler is wrapped by `Sentry.withSentry`, inert until `SENTRY_DSN` is set and with
    `sendDefaultPii=false` (`worker/src/index.ts:3187-3195`); its `beforeSend`/`beforeSendTransaction`
    run `scrubSentryEvent` (`worker/src/sentry-scrub.ts`) over EVERY event before it leaves the Worker —
    not just sensitive header KEYS but message/exception bodies, breadcrumbs, and `extra`/`contexts`
    VALUES (CoreLink PATs, bearer/basic auth, Stripe `sk_`/`pk_`/`whsec_` keys, emails are
    `[REDACTED]`), closing the WP4 PII/secret-leak gap (`worker/src/index.ts:3196-3201`).

# Invariants
- Tenant isolation is structural: the DO id is derived solely from the PAT-resolved tenant, never the
  URL path segment (`worker/src/index.ts:3026-3028`).
- A client can never smuggle a server-trust header: the strip list is applied on every forward path
  before the Worker sets its own values (`worker/src/index.ts:513-582`). The team-RBAC role header
  `x-corelink-role` (migration 0074) is a member of that strip list, so a client copy is always deleted;
  the Worker is its SOLE setter and stamps the D1-resolved role only on the customer-plane forward
  (`worker/src/index.ts:581`, set at `worker/src/index.ts:2528`) — this is what lets the container gate
  OWNER-only account deletion, and owner/admin-only team-management (invite/remove), without trusting a
  client-supplied role.
- The signing-key gate is mandatory — a missing/short `PAT_SIGNING_KEY` is a 503, never a silent skip
  (`worker/src/index.ts:1066-1092`).
- The Worker forwards the D1-resolved `scope` as `x-corelink-scope` and is its sole setter
  (`worker/src/index.ts:3062`); for a find-only PAT (`pat.find_only = 1`) the forwarded value is narrowed
  to `find-missing` rather than the stored base `read-only` (ADR-0071, `worker/src/index.ts:1318`).
- On the customer (Clerk-session) forward the Worker sets `x-corelink-scope` from the D1-resolved team
  role, a THREE-way branch (not the old "viewer vs everyone-else"): `viewer` → `read-only`, `member` →
  `read-write`, `owner`/`admin` → `read-write billing` — so the `billing` capability (billing portal /
  cancel subscription / financial PII) is now OWNER/ADMIN-only and a plain `member` no longer clears the
  container's F-018 billing/PII gate; the Worker is the sole setter (`worker/src/index.ts:2515-2522`).
- A 404 from the DO is timing-padded to defeat cross-tenant enumeration
  (`worker/src/index.ts:3124-3132`).
- Data-residency is resolved FAIL-CLOSED before the DO forward: an unresolvable region (a D1 fault with
  no cached fallback) is a `503`, never an IAD-local fall-through that could place an EU tenant's bytes in
  US storage; only a resolved `null` (no row / no pin) falls through to IAD-local
  (`worker/src/index.ts:2893-2910`, `worker/src/lib/tenant_residency_cache.ts:284-291`).

# Gotchas
- Argon2id is NOT run in the Worker (cpu_ms budget) — possession on the native CAS/AC/Bazel/Turbo plane
  rests solely on the HMAC gate; adapter routes (cargo/brew/npm/pip/oci) re-verify in the container.
- OCI, the Stripe webhook, and the fabric-introspect / resolve-tenant / billing-ingest arms are pure
  pass-throughs: the Worker applies no edge PAT gate and the container is the sole auth authority for
  them. `/internal/v1/auth/introspect` (`worker/src/index.ts:834`) and `/internal/v1/auth/resolve-tenant`
  (`worker/src/index.ts:846`) share `routeKind="fabric_introspect"` + the `FABRIC_INTROSPECT_AUTH_KEY` gate;
  `/internal/v1/billing/usage` (`worker/src/index.ts:858`) uses its own `BILLING_INGEST_AUTH_KEY`.

# Citations
1. `worker/src/index.ts:1-20` — the architecture header documenting the Worker → DO → container topology.
2. `worker/src/index.ts:336-366` — the `AuthResult` carrying the D1-resolved scope (the Worker is its sole authority).
3. `worker/src/index.ts:513-582` — the `CLIENT_TRUST_HEADERS` strip list.
4. `worker/src/index.ts:589-593` — `stripClientTrustHeaders` (delete-then-set discipline).
5. `worker/src/index.ts:641-952` — the `matchRoute` ordered route table.
5b. `worker/src/index.ts:834` / `worker/src/index.ts:846` / `worker/src/index.ts:858` — the three exact-path pure-pass-through carve-outs: `/internal/v1/auth/introspect` + `/internal/v1/auth/resolve-tenant` (both `fabric_introspect`, `FABRIC_INTROSPECT_AUTH_KEY`) and `/internal/v1/billing/usage` (`billing_ingest`, `BILLING_INGEST_AUTH_KEY`).
6. `worker/src/index.ts:1066-1092` — `extractAuth` fail-CLOSED on absent/short `PAT_SIGNING_KEY`.
7. `worker/src/index.ts:1120-1278` — HMAC fast-reject + cached D1 lookup + expiry check (incl. the `expires_ms === 0` never-expires sentinel guard at `:1265`, and the cached PAT-row lookup `verifyPatRowCached` whose `error` kind becomes `d1_lookup_error` at `:1247-1251`).
7b. `worker/src/index.ts:2616-2653` — caller reason→status mapping: BOTH `signing_key_not_configured` (config fault) AND `d1_lookup_error` (transient D1 fault) → retryable `503`; every other reason (`pat_not_found` / `pat_expired` / `invalid_*`) → `401`.
8. `worker/src/index.ts:1737-1761` — the `baseHandler.fetch` entry, request-id, CORS, route match.
9. `worker/src/index.ts:1763-1780` — health short-circuit (no auth, no DO).
10. `worker/src/index.ts:2686-2838` — per-tier quota enforcement after auth, before forward.
10b. `worker/src/index.ts:2888-2892` — `resolveTenantResidency` three-tier residency resolution (L1 isolate → L2 KV `tres:` 60 s → L3 D1) replacing the former inline `SELECT primary_region` per-request D1-PRIMARY read; FAIL-CLOSED `503 RESIDENCY_UNAVAILABLE` on the `RESIDENCY_UNRESOLVED` sentinel (`worker/src/index.ts:2893-2910`), `null` region ⇒ IAD-local (`worker/src/index.ts:2914`). The cache module is `worker/src/lib/tenant_residency_cache.ts:231-298`, whose catch serves a cached region on a transient D1 fault but returns `RESIDENCY_UNRESOLVED` (never caches the error) when none is held (`worker/src/lib/tenant_residency_cache.ts:284-291`).
11. `worker/src/index.ts:3026-3028` — `idFromName(resolvedTenantId)` per-tenant DO routing.
12. `worker/src/index.ts:3031-3062` — the augmented forward (strip-then-set trust headers).
13. `worker/src/index.ts:3062` — forwarding the D1-resolved scope as `x-corelink-scope` (narrowed to `find-missing` for a find-only PAT, ADR-0071 `worker/src/index.ts:1318`).
14. `worker/src/index.ts:3110` — `stub.fetch` dispatch to the DO.
15. `worker/src/index.ts:3124-3132` — 404 timing-pad.
16. `worker/src/index.ts:3187-3195` — the `Sentry.withSentry` wrapper (inert until `SENTRY_DSN`; `sendDefaultPii=false`).
17. `worker/src/index.ts:3196-3201` — `beforeSend`/`beforeSendTransaction` → `scrubSentryEvent` (`worker/src/sentry-scrub.ts:101-188`): full-event PII/secret scrub (default-DENY sensitive keys + free-text secret/PII shapes), not just header keys.
