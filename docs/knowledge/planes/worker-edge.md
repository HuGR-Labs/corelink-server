---
type: "Plane"
title: "Cloudflare Worker edge plane"
description: "The HTTPS entry point: route table, PAT auth, server-trust header hygiene, per-tier quota, and forwarding to the per-tenant Durable Object."
source_files:
  - "worker/src/index.ts"
  - "worker/src/sentry-scrub.ts"
checkpoint_sha: "2103eddb4c5a79d3ee4c553d3da9f89c54aebcfa"
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
  forwards it as a server-trust header (`worker/src/index.ts:292-307`).

# How it works
1. The exported handler is `baseHandler.fetch`, which resolves a request-id, handles CORS preflight,
   and matches the route before doing anything else (`worker/src/index.ts:1494-1507`).
2. `matchRoute` is an ordered first-match table mapping each URL to a `RouteKind` + tenant, with
   specificity ordering (signup/customer/onboarding before the generic `/v1/*` arm)
   (`worker/src/index.ts:532-820`).
3. Health routes short-circuit with no auth and no DO forward (`worker/src/index.ts:1512-1525`).
   The Artifact 1 `/v1/public/*` arm (the erasure-attestation verifier, `routeKind="public_attestation"`)
   is matched BEFORE the generic `/v1/*` PAT bucket and forwarded to the `_anonymous` DO → container as a
   pure pass-through with NO PAT gate and NO internal-auth (an erasure proof is publicly verifiable;
   client-forged `x-corelink-*` trust headers are still stripped) — matchRoute arm
   (`worker/src/index.ts:794-804`), forward arm (`worker/src/index.ts:1958-1982`).
3b. Three EXACT-path fabric/ingest carve-outs are matched BEFORE the generic `/v1/*` PAT arm and are pure
   pass-throughs to the `_system` DO → container (the container is the SOLE auth authority; the Worker
   applies NO edge PAT gate and forwards the caller's `x-corelink-internal-auth` unchanged):
   `/internal/v1/auth/introspect` (`worker/src/index.ts:703`) and the sibling
   `/internal/v1/auth/resolve-tenant` (`worker/src/index.ts:715`) — both share `routeKind="fabric_introspect"`
   and the same `FABRIC_INTROSPECT_AUTH_KEY` (+ optional `_HUGR`) gate; resolve-tenant lets a fabric consumer
   (githugr) map `clerk_org_id(=sub)→tenant_id` for isolation verification + per-tenant reads. The runner
   billing usage-push (`/internal/v1/billing/usage`, `routeKind="billing_ingest"`, `worker/src/index.ts:727`)
   is the same shape but gated by its own DEDICATED `BILLING_INGEST_AUTH_KEY`.
4. `extractAuth` fails CLOSED (503) when `PAT_SIGNING_KEY` is absent or decodes to < 32 bytes — the
   signing key is the sole possession gate for the native plane (`worker/src/index.ts:882-909`).
5. PAT validation is HMAC-SHA256 fast-reject (with rotation siblings) BEFORE any D1 round-trip, then a
   D1 lookup by `token_id` and an application-side expiry check (`worker/src/index.ts:947-1067`). The
   expiry check honors the `expires_ms === 0` "never expires" sentinel
   (`row.expires_ms !== 0 && row.expires_ms <= now`), matching the container's `adapter_pat` SQL
   (`expires_ms = 0 OR expires_ms > now`) — so a no-TTL PAT is no longer a split-brain edge-reject that
   would pass in the container but die at the Worker (looking like a forged token). The D1 lookup is
   wrapped in try/catch: a TRANSIENT D1 fault (network partition / DB unavailable) returns the distinct
   reason `d1_lookup_error` (`worker/src/index.ts:1043-1053`). The caller maps BOTH the config fault
   `signing_key_not_configured` (absent/short `PAT_SIGNING_KEY`) AND `d1_lookup_error` to a retryable
   `503` — a D1 infra hiccup is treated as "auth service unavailable", NOT a bad credential, so a
   transient outage can't masquerade as a 401 (which would trigger spurious CI failures, PAT rotation,
   and on-call chasing the wrong thing). Genuine bad/unknown PATs (`pat_not_found` / `pat_expired` /
   `invalid_*`) still fall through to `401` (`worker/src/index.ts:2167-2197`). NOTE: the inline
   `extractAuth` comment at `:1052` ("for now we 401 to fail-closed") is STALE — it describes a
   superseded posture and was never updated when the H1 caller-mapping fix landed; the live caller
   mapping returns 503.
6. `stripClientTrustHeaders` deletes every client-suppliable trust header on every forward, then the
   Worker re-sets its own verified values (`worker/src/index.ts:480-484`).
7. Per-tier quota (storage SUM + monthly request-count) runs after auth and before the DO forward —
   request-count fail-CLOSED; storage verb-aware (reads fail-open for availability, byte-adding writes
   fail-closed) (`worker/src/index.ts:2230-2251`).
8. The request is routed to the per-tenant DO via `idFromName(resolvedTenantId)`
   (`worker/src/index.ts:2544-2547`) and dispatched with `stub.fetch` (`worker/src/index.ts:2612`).
9. The forwarded request is augmented: strip-then-set the trusted tenant-id, scope, token-prefix, and
   client-ip headers (`worker/src/index.ts:2549-2607`).
10. The whole handler is wrapped by `Sentry.withSentry`, inert until `SENTRY_DSN` is set and with
    `sendDefaultPii=false` (`worker/src/index.ts:2665-2684`); its `beforeSend`/`beforeSendTransaction`
    run `scrubSentryEvent` (`worker/src/sentry-scrub.ts`) over EVERY event before it leaves the Worker —
    not just sensitive header KEYS but message/exception bodies, breadcrumbs, and `extra`/`contexts`
    VALUES (CoreLink PATs, bearer/basic auth, Stripe `sk_`/`pk_`/`whsec_` keys, emails are
    `[REDACTED]`), closing the WP4 PII/secret-leak gap (`worker/src/index.ts:2674-2679`).

# Invariants
- Tenant isolation is structural: the DO id is derived solely from the PAT-resolved tenant, never the
  URL path segment (`worker/src/index.ts:2544-2547`).
- A client can never smuggle a server-trust header: the strip list is applied on every forward path
  before the Worker sets its own values (`worker/src/index.ts:434-473`).
- The signing-key gate is mandatory — a missing/short `PAT_SIGNING_KEY` is a 503, never a silent skip
  (`worker/src/index.ts:882-909`).
- The Worker forwards the D1-resolved `scope` as `x-corelink-scope` and is its sole setter
  (`worker/src/index.ts:2578-2581`).
- A 404 from the DO is timing-padded to defeat cross-tenant enumeration
  (`worker/src/index.ts:2625-2632`).

# Gotchas
- Argon2id is NOT run in the Worker (cpu_ms budget) — possession on the native CAS/AC/Bazel/Turbo plane
  rests solely on the HMAC gate; adapter routes (cargo/brew/npm/pip/oci) re-verify in the container.
- OCI, the Stripe webhook, and the fabric-introspect / resolve-tenant / billing-ingest arms are pure
  pass-throughs: the Worker applies no edge PAT gate and the container is the sole auth authority for
  them. `/internal/v1/auth/introspect` (`worker/src/index.ts:703`) and `/internal/v1/auth/resolve-tenant`
  (`worker/src/index.ts:715`) share `routeKind="fabric_introspect"` + the `FABRIC_INTROSPECT_AUTH_KEY` gate;
  `/internal/v1/billing/usage` (`worker/src/index.ts:727`) uses its own `BILLING_INGEST_AUTH_KEY`.

# Citations
1. `worker/src/index.ts:1-20` — the architecture header documenting the Worker → DO → container topology.
2. `worker/src/index.ts:292-307` — the `AuthResult` carrying the D1-resolved scope (the Worker is its sole authority).
3. `worker/src/index.ts:434-473` — the `CLIENT_TRUST_HEADERS` strip list.
4. `worker/src/index.ts:480-484` — `stripClientTrustHeaders` (delete-then-set discipline).
5. `worker/src/index.ts:532-820` — the `matchRoute` ordered route table.
5b. `worker/src/index.ts:703` / `worker/src/index.ts:715` / `worker/src/index.ts:727` — the three exact-path pure-pass-through carve-outs: `/internal/v1/auth/introspect` + `/internal/v1/auth/resolve-tenant` (both `fabric_introspect`, `FABRIC_INTROSPECT_AUTH_KEY`) and `/internal/v1/billing/usage` (`billing_ingest`, `BILLING_INGEST_AUTH_KEY`).
6. `worker/src/index.ts:882-909` — `extractAuth` fail-CLOSED on absent/short `PAT_SIGNING_KEY`.
7. `worker/src/index.ts:947-1067` — HMAC fast-reject + D1 lookup + expiry check (incl. the `expires_ms === 0` never-expires sentinel guard at `:1067`, and the try/catch that turns a transient D1 fault into `d1_lookup_error` at `:1031-1041`).
7b. `worker/src/index.ts:2167-2197` — caller reason→status mapping: BOTH `signing_key_not_configured` (config fault) AND `d1_lookup_error` (transient D1 fault) → retryable `503`; every other reason (`pat_not_found` / `pat_expired` / `invalid_*`) → `401`.
8. `worker/src/index.ts:1494-1507` — the `baseHandler.fetch` entry, request-id, CORS, route match.
9. `worker/src/index.ts:1512-1525` — health short-circuit (no auth, no DO).
10. `worker/src/index.ts:2230-2251` — per-tier quota enforcement after auth, before forward.
11. `worker/src/index.ts:2544-2547` — `idFromName(resolvedTenantId)` per-tenant DO routing.
12. `worker/src/index.ts:2549-2607` — the augmented forward (strip-then-set trust headers).
13. `worker/src/index.ts:2578-2581` — forwarding the D1-resolved scope as `x-corelink-scope`.
14. `worker/src/index.ts:2612` — `stub.fetch` dispatch to the DO.
15. `worker/src/index.ts:2625-2632` — 404 timing-pad.
16. `worker/src/index.ts:2665-2684` — the `Sentry.withSentry` wrapper (inert until `SENTRY_DSN`; `sendDefaultPii=false`).
17. `worker/src/index.ts:2674-2679` — `beforeSend`/`beforeSendTransaction` → `scrubSentryEvent` (`worker/src/sentry-scrub.ts:101-188`): full-event PII/secret scrub (default-DENY sensitive keys + free-text secret/PII shapes), not just header keys.
