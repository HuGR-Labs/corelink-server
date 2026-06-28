---
type: "Plane"
title: "Cloudflare Worker edge plane"
description: "The HTTPS entry point: route table, PAT auth, server-trust header hygiene, per-tier quota, and forwarding to the per-tenant Durable Object."
source_files:
  - "worker/src/index.ts"
checkpoint_sha: "2ca7714476506bd7702b44cdb53e19600432e0b6"
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
  forwards it as a server-trust header (`worker/src/index.ts:289-304`).

# How it works
1. The exported handler is `baseHandler.fetch`, which resolves a request-id, handles CORS preflight,
   and matches the route before doing anything else (`worker/src/index.ts:1478-1491`).
2. `matchRoute` is an ordered first-match table mapping each URL to a `RouteKind` + tenant, with
   specificity ordering (signup/customer/onboarding before the generic `/v1/*` arm)
   (`worker/src/index.ts:529-805`).
3. Health routes short-circuit with no auth and no DO forward (`worker/src/index.ts:1497-1510`).
   The Artifact 1 `/v1/public/*` arm (the erasure-attestation verifier, `routeKind="public_attestation"`)
   is matched BEFORE the generic `/v1/*` PAT bucket and forwarded to the `_anonymous` DO → container as a
   pure pass-through with NO PAT gate and NO internal-auth (an erasure proof is publicly verifiable;
   client-forged `x-corelink-*` trust headers are still stripped) — matchRoute arm
   (`worker/src/index.ts:779-789`), forward arm (`worker/src/index.ts:1943-1967`).
4. `extractAuth` fails CLOSED (503) when `PAT_SIGNING_KEY` is absent or decodes to < 32 bytes — the
   signing key is the sole possession gate for the native plane (`worker/src/index.ts:867-894`).
5. PAT validation is HMAC-SHA256 fast-reject (with rotation siblings) BEFORE any D1 round-trip, then a
   D1 lookup by `token_id` and an application-side expiry check (`worker/src/index.ts:932-1052`). The
   expiry check honors the `expires_ms === 0` "never expires" sentinel
   (`row.expires_ms !== 0 && row.expires_ms <= now`), matching the container's `adapter_pat` SQL
   (`expires_ms = 0 OR expires_ms > now`) — so a no-TTL PAT is no longer a split-brain edge-reject that
   would pass in the container but die at the Worker (looking like a forged token). The D1 lookup is
   wrapped in try/catch: a TRANSIENT D1 fault (network partition / DB unavailable) returns the distinct
   reason `d1_lookup_error` (`worker/src/index.ts:1028-1038`). The caller maps BOTH the config fault
   `signing_key_not_configured` (absent/short `PAT_SIGNING_KEY`) AND `d1_lookup_error` to a retryable
   `503` — a D1 infra hiccup is treated as "auth service unavailable", NOT a bad credential, so a
   transient outage can't masquerade as a 401 (which would trigger spurious CI failures, PAT rotation,
   and on-call chasing the wrong thing). Genuine bad/unknown PATs (`pat_not_found` / `pat_expired` /
   `invalid_*`) still fall through to `401` (`worker/src/index.ts:2152-2182`). NOTE: the inline
   `extractAuth` comment at `:1036` ("for now we 401 to fail-closed") is STALE — it describes a
   superseded posture and was never updated when the H1 caller-mapping fix landed; the live caller
   mapping returns 503.
6. `stripClientTrustHeaders` deletes every client-suppliable trust header on every forward, then the
   Worker re-sets its own verified values (`worker/src/index.ts:477-481`).
7. Per-tier quota (storage SUM + monthly request-count) runs after auth and before the DO forward —
   request-count fail-CLOSED; storage verb-aware (reads fail-open for availability, byte-adding writes
   fail-closed) (`worker/src/index.ts:2215-2236`).
8. The request is routed to the per-tenant DO via `idFromName(resolvedTenantId)`
   (`worker/src/index.ts:2529-2532`) and dispatched with `stub.fetch` (`worker/src/index.ts:2596`).
9. The forwarded request is augmented: strip-then-set the trusted tenant-id, scope, token-prefix, and
   client-ip headers (`worker/src/index.ts:2534-2592`).
10. The whole handler is wrapped by `Sentry.withSentry`, inert until `SENTRY_DSN` is set
    (`worker/src/index.ts:2663-2681`), scrubbing Authorization/Cookie/internal-auth headers via the
    `SENTRY_SENSITIVE_HEADER_PATTERN` scrub list (`worker/src/index.ts:2648-2660`).

# Invariants
- Tenant isolation is structural: the DO id is derived solely from the PAT-resolved tenant, never the
  URL path segment (`worker/src/index.ts:2529-2532`).
- A client can never smuggle a server-trust header: the strip list is applied on every forward path
  before the Worker sets its own values (`worker/src/index.ts:431-470`).
- The signing-key gate is mandatory — a missing/short `PAT_SIGNING_KEY` is a 503, never a silent skip
  (`worker/src/index.ts:867-894`).
- The Worker forwards the D1-resolved `scope` as `x-corelink-scope` and is its sole setter
  (`worker/src/index.ts:2562-2565`).
- A 404 from the DO is timing-padded to defeat cross-tenant enumeration
  (`worker/src/index.ts:2610-2617`).

# Gotchas
- Argon2id is NOT run in the Worker (cpu_ms budget) — possession on the native CAS/AC/Bazel/Turbo plane
  rests solely on the HMAC gate; adapter routes (cargo/brew/npm/pip/oci) re-verify in the container.
- OCI, the Stripe webhook, and the fabric-introspect arms are pure pass-throughs: the Worker applies no
  edge PAT gate and the container is the sole auth authority for them.

# Citations
1. `worker/src/index.ts:1-20` — the architecture header documenting the Worker → DO → container topology.
2. `worker/src/index.ts:289-304` — the `AuthResult` carrying the D1-resolved scope (the Worker is its sole authority).
3. `worker/src/index.ts:431-470` — the `CLIENT_TRUST_HEADERS` strip list.
4. `worker/src/index.ts:477-481` — `stripClientTrustHeaders` (delete-then-set discipline).
5. `worker/src/index.ts:529-805` — the `matchRoute` ordered route table.
6. `worker/src/index.ts:867-894` — `extractAuth` fail-CLOSED on absent/short `PAT_SIGNING_KEY`.
7. `worker/src/index.ts:932-1052` — HMAC fast-reject + D1 lookup + expiry check (incl. the `expires_ms === 0` never-expires sentinel guard at `:1051`, and the try/catch that turns a transient D1 fault into `d1_lookup_error` at `:1028-1038`).
7b. `worker/src/index.ts:2152-2182` — caller reason→status mapping: BOTH `signing_key_not_configured` (config fault) AND `d1_lookup_error` (transient D1 fault) → retryable `503`; every other reason (`pat_not_found` / `pat_expired` / `invalid_*`) → `401`.
8. `worker/src/index.ts:1478-1491` — the `baseHandler.fetch` entry, request-id, CORS, route match.
9. `worker/src/index.ts:1497-1510` — health short-circuit (no auth, no DO).
10. `worker/src/index.ts:2215-2236` — per-tier quota enforcement after auth, before forward.
11. `worker/src/index.ts:2529-2532` — `idFromName(resolvedTenantId)` per-tenant DO routing.
12. `worker/src/index.ts:2534-2592` — the augmented forward (strip-then-set trust headers).
13. `worker/src/index.ts:2562-2565` — forwarding the D1-resolved scope as `x-corelink-scope`.
14. `worker/src/index.ts:2596` — `stub.fetch` dispatch to the DO.
15. `worker/src/index.ts:2610-2617` — 404 timing-pad.
16. `worker/src/index.ts:2663-2681` — the Sentry wrapper (inert until `SENTRY_DSN`).
17. `worker/src/index.ts:2648-2660` — `SENTRY_SENSITIVE_HEADER_PATTERN` + `scrubSentryEvent` header-scrub list.
