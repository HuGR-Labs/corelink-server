---
type: "Plane"
title: "Cloudflare Worker edge plane"
description: "The HTTPS entry point: route table, PAT auth, server-trust header hygiene, per-tier quota, and forwarding to the per-tenant Durable Object."
source_files:
  - "worker/src/index.ts"
checkpoint_sha: "41d84e271568cb47df664806fa3dc9798c134249"
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
  forwards it as a server-trust header (`worker/src/index.ts:285-300`).

# How it works
1. The exported handler is `baseHandler.fetch`, which resolves a request-id, handles CORS preflight,
   and matches the route before doing anything else (`worker/src/index.ts:1457-1470`).
2. `matchRoute` is an ordered first-match table mapping each URL to a `RouteKind` + tenant, with
   specificity ordering (signup/customer/onboarding before the generic `/v1/*` arm)
   (`worker/src/index.ts:525-789`).
3. Health routes short-circuit with no auth and no DO forward (`worker/src/index.ts:1476-1489`).
4. `extractAuth` fails CLOSED (503) when `PAT_SIGNING_KEY` is absent or decodes to < 32 bytes — the
   signing key is the sole possession gate for the native plane (`worker/src/index.ts:851-878`).
5. PAT validation is HMAC-SHA256 fast-reject (with rotation siblings) BEFORE any D1 round-trip, then a
   D1 lookup by `token_id` and an application-side expiry check (`worker/src/index.ts:916-1031`).
6. `stripClientTrustHeaders` deletes every client-suppliable trust header on every forward, then the
   Worker re-sets its own verified values (`worker/src/index.ts:473-477`).
7. Per-tier quota (storage SUM + monthly request-count, fail-CLOSED by default) runs after auth and
   before the DO forward (`worker/src/index.ts:2151-2172`).
8. The request is routed to the per-tenant DO via `idFromName(resolvedTenantId)`
   (`worker/src/index.ts:2465-2468`) and dispatched with `stub.fetch` (`worker/src/index.ts:2532`).
9. The forwarded request is augmented: strip-then-set the trusted tenant-id, scope, token-prefix, and
   client-ip headers (`worker/src/index.ts:2470-2528`).
10. The whole handler is wrapped by `Sentry.withSentry`, inert until `SENTRY_DSN` is set, scrubbing
    Authorization/Cookie/internal-auth headers (`worker/src/index.ts:2599-2617`).

# Invariants
- Tenant isolation is structural: the DO id is derived solely from the PAT-resolved tenant, never the
  URL path segment (`worker/src/index.ts:2465-2468`).
- A client can never smuggle a server-trust header: the strip list is applied on every forward path
  before the Worker sets its own values (`worker/src/index.ts:427-466`).
- The signing-key gate is mandatory — a missing/short `PAT_SIGNING_KEY` is a 503, never a silent skip
  (`worker/src/index.ts:851-878`).
- The Worker forwards the D1-resolved `scope` as `x-corelink-scope` and is its sole setter
  (`worker/src/index.ts:2498-2501`).
- A 404 from the DO is timing-padded to defeat cross-tenant enumeration
  (`worker/src/index.ts:2546-2553`).

# Gotchas
- Argon2id is NOT run in the Worker (cpu_ms budget) — possession on the native CAS/AC/Bazel/Turbo plane
  rests solely on the HMAC gate; adapter routes (cargo/brew/npm/pip/oci) re-verify in the container.
- OCI, the Stripe webhook, and the fabric-introspect arms are pure pass-throughs: the Worker applies no
  edge PAT gate and the container is the sole auth authority for them.

# Citations
1. `worker/src/index.ts:1-20` — the architecture header documenting the Worker → DO → container topology.
2. `worker/src/index.ts:285-300` — the `AuthResult` carrying the D1-resolved scope (the Worker is its sole authority).
3. `worker/src/index.ts:427-466` — the `CLIENT_TRUST_HEADERS` strip list.
4. `worker/src/index.ts:473-477` — `stripClientTrustHeaders` (delete-then-set discipline).
5. `worker/src/index.ts:525-789` — the `matchRoute` ordered route table.
6. `worker/src/index.ts:851-878` — `extractAuth` fail-CLOSED on absent/short `PAT_SIGNING_KEY`.
7. `worker/src/index.ts:916-1031` — HMAC fast-reject + D1 lookup + expiry check.
8. `worker/src/index.ts:1457-1470` — the `baseHandler.fetch` entry, request-id, CORS, route match.
9. `worker/src/index.ts:1476-1489` — health short-circuit (no auth, no DO).
10. `worker/src/index.ts:2151-2172` — per-tier quota enforcement after auth, before forward.
11. `worker/src/index.ts:2465-2468` — `idFromName(resolvedTenantId)` per-tenant DO routing.
12. `worker/src/index.ts:2470-2528` — the augmented forward (strip-then-set trust headers).
13. `worker/src/index.ts:2498-2501` — forwarding the D1-resolved scope as `x-corelink-scope`.
14. `worker/src/index.ts:2532` — `stub.fetch` dispatch to the DO.
15. `worker/src/index.ts:2546-2553` — 404 timing-pad.
16. `worker/src/index.ts:2599-2617` — the Sentry wrapper (inert until `SENTRY_DSN`).
