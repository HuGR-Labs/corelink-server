---
type: "Plane"
title: "Cloudflare Worker edge plane"
description: "The HTTPS entry point: route table, PAT auth, server-trust header hygiene, per-tier quota, and forwarding to the per-tenant Durable Object."
source_files:
  - "worker/src/index.ts"
  - "worker/src/sentry-scrub.ts"
checkpoint_sha: "294982663d1b05462077c616ca00886be16159e7"
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
  forwards it as a server-trust header (`worker/src/index.ts:329-354`).
- The declarer of the container's env contract: every operator secret the container reads is first a
  field on the Worker `Env` interface here (`worker/src/index.ts:208`), then materialized onto the
  container by the per-tenant DO's `container.start({ env })` forward — owned by
  [the Durable Object lifecycle](/planes/durable-object.md) concept. That DO forward now also carries the
  launch-checkout coupon id `STRIPE_LAUNCH_COUPON` alongside the `EMAIL_HASH_SALT` salt — unset ⇒ checkout
  falls back to `allow_promotion_codes=true`, zero regression; a var the container reads via `env::var`
  but neither declared on `Env` here nor forwarded by the DO would silently never reach the container.

# How it works
1. The exported handler is `baseHandler.fetch`, which resolves a request-id, handles CORS preflight,
   and matches the route before doing anything else (`worker/src/index.ts:1641-1654`).
2. `matchRoute` is an ordered first-match table mapping each URL to a `RouteKind` + tenant, with
   specificity ordering (signup/customer/onboarding before the generic `/v1/*` arm)
   (`worker/src/index.ts:580-891`).
3. Health routes short-circuit with no auth and no DO forward (`worker/src/index.ts:1659-1672`).
   The Artifact 1 `/v1/public/*` arm (the erasure-attestation verifier, `routeKind="public_attestation"`)
   is matched BEFORE the generic `/v1/*` PAT bucket and forwarded to the `_anonymous` DO → container as a
   pure pass-through with NO PAT gate and NO internal-auth (an erasure proof is publicly verifiable;
   client-forged `x-corelink-*` trust headers are still stripped) — matchRoute arm
   (`worker/src/index.ts:895-896`), forward arm (`worker/src/index.ts:2256-2278`).
3b. Three EXACT-path fabric/ingest carve-outs are matched BEFORE the generic `/v1/*` PAT arm and are pure
   pass-throughs to the `_system` DO → container (the container is the SOLE auth authority; the Worker
   applies NO edge PAT gate and forwards the caller's `x-corelink-internal-auth` unchanged):
   `/internal/v1/auth/introspect` (`worker/src/index.ts:774`) and the sibling
   `/internal/v1/auth/resolve-tenant` (`worker/src/index.ts:786`) — both share `routeKind="fabric_introspect"`
   and the same `FABRIC_INTROSPECT_AUTH_KEY` (+ optional `_HUGR`) gate; resolve-tenant lets a fabric consumer
   (githugr) map `clerk_org_id(=sub)→tenant_id` for isolation verification + per-tenant reads. The runner
   billing usage-push (`/internal/v1/billing/usage`, `routeKind="billing_ingest"`, `worker/src/index.ts:798`)
   is the same shape but gated by its own DEDICATED `BILLING_INGEST_AUTH_KEY`.
4. `extractAuth` fails CLOSED (503) when `PAT_SIGNING_KEY` is absent or decodes to < 32 bytes — the
   signing key is the sole possession gate for the native plane (`worker/src/index.ts:1010-1049`).
5. PAT validation is HMAC-SHA256 fast-reject (with rotation siblings) BEFORE any D1 round-trip, then a
   D1 lookup by `token_id` and an application-side expiry check (`worker/src/index.ts:1079-1226`). The
   expiry check honors the `expires_ms === 0` "never expires" sentinel
   (`row.expires_ms !== 0 && row.expires_ms <= now`), matching the container's `adapter_pat` SQL
   (`expires_ms = 0 OR expires_ms > now`) — so a no-TTL PAT is no longer a split-brain edge-reject that
   would pass in the container but die at the Worker (looking like a forged token). The D1 lookup is
   wrapped in try/catch: a TRANSIENT D1 fault (network partition / DB unavailable) returns the distinct
   reason `d1_lookup_error` (`worker/src/index.ts:1202-1211`). The caller maps BOTH the config fault
   `signing_key_not_configured` (absent/short `PAT_SIGNING_KEY`) AND `d1_lookup_error` to a retryable
   `503` — a D1 infra hiccup is treated as "auth service unavailable", NOT a bad credential, so a
   transient outage can't masquerade as a 401 (which would trigger spurious CI failures, PAT rotation,
   and on-call chasing the wrong thing). Genuine bad/unknown PATs (`pat_not_found` / `pat_expired` /
   `invalid_*`) still fall through to `401` (`worker/src/index.ts:2501-2530`). NOTE: the inline
   `extractAuth` `catch` comment at `:1088-1090` now MATCHES that behaviour — it states the caller maps
   `d1_lookup_error` to a 503 (transient, retryable; still fail-closed); it previously lied ("for now we
   401 to fail-closed"), a stale posture left over from before the H1 caller-mapping fix landed.
6. `stripClientTrustHeaders` deletes every client-suppliable trust header on every forward, then the
   Worker re-sets its own verified values (`worker/src/index.ts:515-519`).
7. Per-tier quota (storage SUM + monthly request-count) runs after auth and before the DO forward —
   request-count fail-CLOSED; storage verb-aware (reads fail-open for availability, byte-adding writes
   fail-closed) (`worker/src/index.ts:2535-2556`).
8. The request is routed to the per-tenant DO via `idFromName(resolvedTenantId)`
   (`worker/src/index.ts:2849-2851`) and dispatched with `stub.fetch` (`worker/src/index.ts:2922`). A
   multi-region tenant may first take the LOCAL (this-region) `_system` container branch above this
   forward (`worker/src/index.ts:1852-1854`); a non-resident tenant falls through to the per-tenant DO
   derivation here.
9. The forwarded request is augmented: strip-then-set the trusted tenant-id, scope, token-prefix, and
   client-ip headers (`worker/src/index.ts:2861-2897`).
10. The whole handler is wrapped by `Sentry.withSentry`, inert until `SENTRY_DSN` is set and with
    `sendDefaultPii=false` (`worker/src/index.ts:2981-3000`); its `beforeSend`/`beforeSendTransaction`
    run `scrubSentryEvent` (`worker/src/sentry-scrub.ts`) over EVERY event before it leaves the Worker —
    not just sensitive header KEYS but message/exception bodies, breadcrumbs, and `extra`/`contexts`
    VALUES (CoreLink PATs, bearer/basic auth, Stripe `sk_`/`pk_`/`whsec_` keys, emails are
    `[REDACTED]`), closing the WP4 PII/secret-leak gap (`worker/src/index.ts:2990-2995`).

# Invariants
- Tenant isolation is structural: the DO id is derived solely from the PAT-resolved tenant, never the
  URL path segment (`worker/src/index.ts:2849-2851`).
- A client can never smuggle a server-trust header: the strip list is applied on every forward path
  before the Worker sets its own values (`worker/src/index.ts:469-508`).
- The signing-key gate is mandatory — a missing/short `PAT_SIGNING_KEY` is a 503, never a silent skip
  (`worker/src/index.ts:1010-1049`).
- The Worker forwards the D1-resolved `scope` as `x-corelink-scope` and is its sole setter
  (`worker/src/index.ts:2891`).
- A 404 from the DO is timing-padded to defeat cross-tenant enumeration
  (`worker/src/index.ts:2936-2949`).

# Gotchas
- Argon2id is NOT run in the Worker (cpu_ms budget) — possession on the native CAS/AC/Bazel/Turbo plane
  rests solely on the HMAC gate; adapter routes (cargo/brew/npm/pip/oci) re-verify in the container.
- OCI, the Stripe webhook, and the fabric-introspect / resolve-tenant / billing-ingest arms are pure
  pass-throughs: the Worker applies no edge PAT gate and the container is the sole auth authority for
  them. `/internal/v1/auth/introspect` (`worker/src/index.ts:774`) and `/internal/v1/auth/resolve-tenant`
  (`worker/src/index.ts:786`) share `routeKind="fabric_introspect"` + the `FABRIC_INTROSPECT_AUTH_KEY` gate;
  `/internal/v1/billing/usage` (`worker/src/index.ts:798`) uses its own `BILLING_INGEST_AUTH_KEY`.

# Citations
1. `worker/src/index.ts:1-20` — the architecture header documenting the Worker → DO → container topology.
2. `worker/src/index.ts:329-354` — the `AuthResult` carrying the D1-resolved scope (the Worker is its sole authority).
3. `worker/src/index.ts:469-508` — the `CLIENT_TRUST_HEADERS` strip list.
4. `worker/src/index.ts:515-519` — `stripClientTrustHeaders` (delete-then-set discipline).
5. `worker/src/index.ts:580-891` — the `matchRoute` ordered route table.
5b. `worker/src/index.ts:774` / `worker/src/index.ts:786` / `worker/src/index.ts:798` — the three exact-path pure-pass-through carve-outs: `/internal/v1/auth/introspect` + `/internal/v1/auth/resolve-tenant` (both `fabric_introspect`, `FABRIC_INTROSPECT_AUTH_KEY`) and `/internal/v1/billing/usage` (`billing_ingest`, `BILLING_INGEST_AUTH_KEY`).
6. `worker/src/index.ts:1010-1049` — `extractAuth` fail-CLOSED on absent/short `PAT_SIGNING_KEY`.
7. `worker/src/index.ts:1079-1226` — HMAC fast-reject + D1 lookup + expiry check (incl. the `expires_ms === 0` never-expires sentinel guard at `:1166`, and the try/catch that turns a transient D1 fault into `d1_lookup_error` at `:1143-1152`).
7b. `worker/src/index.ts:2501-2530` — caller reason→status mapping: BOTH `signing_key_not_configured` (config fault) AND `d1_lookup_error` (transient D1 fault) → retryable `503`; every other reason (`pat_not_found` / `pat_expired` / `invalid_*`) → `401`.
8. `worker/src/index.ts:1641-1654` — the `baseHandler.fetch` entry, request-id, CORS, route match.
9. `worker/src/index.ts:1659-1672` — health short-circuit (no auth, no DO).
10. `worker/src/index.ts:2535-2556` — per-tier quota enforcement after auth, before forward.
11. `worker/src/index.ts:2849-2851` — `idFromName(resolvedTenantId)` per-tenant DO routing.
12. `worker/src/index.ts:2861-2897` — the augmented forward (strip-then-set trust headers).
13. `worker/src/index.ts:2891` — forwarding the D1-resolved scope as `x-corelink-scope`.
14. `worker/src/index.ts:2922` — `stub.fetch` dispatch to the DO.
15. `worker/src/index.ts:2936-2949` — 404 timing-pad.
16. `worker/src/index.ts:2981-3000` — the `Sentry.withSentry` wrapper (inert until `SENTRY_DSN`; `sendDefaultPii=false`).
17. `worker/src/index.ts:2990-2995` — `beforeSend`/`beforeSendTransaction` → `scrubSentryEvent` (`worker/src/sentry-scrub.ts:101-188`): full-event PII/secret scrub (default-DENY sensitive keys + free-text secret/PII shapes), not just header keys.
