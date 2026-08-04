---
type: "Plane"
title: "Worker → DO → Container request flow"
description: "The end-to-end path a cache request takes: edge auth + routing in the Worker, lifecycle + proxy in the per-tenant Durable Object, handler execution in the Rust container."
source_files:
  - "worker/src/index.ts"
  - "worker/src/durable_object.ts"
  - "crates/corelink-container/src/routes.rs"
source_blobs:
  - "worker/src/index.ts@dc5e60d15637947b7e2b5d7737ab8f293cadf8e8"
  - "worker/src/durable_object.ts@9b407f2b402cc395576be41779afe9387d5a6bc9"
  - "crates/corelink-container/src/routes.rs@37c9e0fc4514d4276e195c8cd8c576a78668fc32"
checkpoint_sha: "e34c59ea521e8d4b86bcd211bc0795b4f855259b"
provenance: "AUTHORED"
tags: ["planes", "request-flow", "topology", "end-to-end"]
timestamp: "2026-06-26T00:00:00Z"
---

# Worker → DO → Container request flow

This concept traces a single cache request across all three planes, end to end. It is the connective
tissue between the [Cloudflare Worker edge plane](/planes/worker-edge.md), the
[Durable Object lifecycle](/planes/durable-object.md), and the [Rust container compute plane](/planes/container.md):
the Worker authenticates and routes, derives the per-tenant DO by `idFromName`, and forwards a request
whose trust headers it alone established; the DO ensures the tenant's container is live and proxies the
request onto port 50051; the container's composed axum router executes the handler and re-verifies
possession. Understanding this hop chain is what explains where each guarantee is enforced — tenant
isolation at the Worker's `idFromName`, container liveness at the DO, and the actual cache/erasure/quota
semantics in the container.

# Role
- The canonical topology: the Worker's own header documents `Internet → Worker → DO → container`
  (`worker/src/index.ts:1-20`).
- The single contract that the per-plane concepts plug into — each hop is the input to the next.

# How it works
1. The request enters `baseHandler.fetch`: request-id, CORS, then `matchRoute` selects a `RouteKind` +
   tenant (`worker/src/index.ts:1752-1798`).
2. The Worker authenticates the Bearer PAT — HMAC fast-reject then a D1 `token_id` lookup + expiry —
   resolving the trusted tenant (`worker/src/index.ts:1135-1293`). EXCEPTION (Artifact 1): the unauth
   `/v1/public/*` arm (the erasure-attestation verifier) is matched BEFORE the generic `/v1/*` PAT bucket
   and forwarded as `_anonymous` with NO PAT and NO internal-auth — an erasure proof is publicly
   verifiable, so this route skips the auth hop entirely (matchRoute arm `worker/src/index.ts:949-950`,
   forward arm `worker/src/index.ts:2381-2409`). The edge expiry check honors the
   `expires_ms === 0` "never expires" sentinel (`row.expires_ms !== 0 && row.expires_ms <= now`),
   matching the container's `adapter_pat` SQL (`expires_ms = 0 OR expires_ms > now`) — so a no-TTL PAT
   is no longer a split-brain edge-reject that worked in the container but died at the Worker. If that
   D1 lookup itself faults (network partition / DB unavailable) the auth hop fails CLOSED but is mapped
   to a retryable 503, not a 401 — the request never reaches the DO, and a transient infra fault is
   never surfaced to the client as "bad credentials" (H1).
3. The Worker derives the per-tenant DO with `idFromName(resolvedTenantId)`, making isolation structural
   (`worker/src/index.ts:3135-3137`). A non-local-region tenant may first be routed to the LOCAL
   (this-region) `_system` container branch above this forward; the fall-through then lands on the
   per-tenant DO derivation here (`worker/src/index.ts:1968-1970`).
4. It strips any client-supplied trust headers (delete-then-set discipline), sets its own verified
   tenant-id/scope/token-prefix, and dispatches via `stub.fetch` (`worker/src/index.ts:3140-3219`).
5. The DO's `fetch` binds the forwarded tenant-id, ensures the container is running, then proxies the
   request (`worker/src/durable_object.ts:333-432`).
6. The proxy rewrites the request onto `http://localhost:50051` through the `getTcpPort` fetcher — the
   DO→container hop (`worker/src/durable_object.ts:282-295`).
7. The container's composed router (built by `build_with_factory`) receives the request and routes it to
   the matching handler (`crates/corelink-container/src/routes.rs:411-413`).
8. The shared CAS/AC handler objects — wrapped once with byte-accounting, the erasure tombstone gate,
   and the native PAT possession backstop — execute the actual cache operation
   (`crates/corelink-container/src/routes.rs:474-583`).

# Invariants
- The DO is always selected from the PAT-resolved tenant, never the URL tenant — isolation is
  established at this hop (`worker/src/index.ts:3135-3137`).
- The request that crosses Worker→DO carries only Worker-established trust headers; client values are
  stripped first (`worker/src/index.ts:3140-3171`).
- The DO→container hop always targets port 50051 via the `getTcpPort` fetcher
  (`worker/src/durable_object.ts:282-295`).
- The container re-verifies possession at the shared handler chokepoint rather than trusting the hop
  blindly (`crates/corelink-container/src/routes.rs:474-583`).
- The DO will not proxy until the container is confirmed running (or it returns 503/500)
  (`worker/src/durable_object.ts:363-392`).

# Gotchas
- OCI, the Stripe webhook, and fabric-introspect take dedicated pass-through arms in the Worker that
  skip the PAT gate; they still traverse the same DO→container hop but the container is the sole auth
  authority for them.
- The DO never sets internal-auth on the data-plane forward, so the container's internal-auth-gated
  admin routes are Worker-unreachable by design (operator-only posture).

# Citations
1. `worker/src/index.ts:1-20` — the `Internet → Worker → DO → container` topology header.
2. `worker/src/index.ts:1135-1293` — edge PAT auth (HMAC fast-reject + D1 lookup + expiry).
3. `worker/src/index.ts:1752-1798` — the Worker `fetch` entry + `matchRoute`.
4. `worker/src/index.ts:3135-3137` — `idFromName(resolvedTenantId)` DO derivation (structural isolation).
5. `worker/src/index.ts:3140-3171` — strip-then-set trust headers on the forward.
6. `worker/src/index.ts:3140-3219` — the augmented forward + `stub.fetch` dispatch to the DO.
7. `worker/src/durable_object.ts:282-295` — the DO→container proxy via `getTcpPort(50051)`.
8. `worker/src/durable_object.ts:333-432` — the DO `fetch`: tenant bind, ensure-running, proxy.
9. `worker/src/durable_object.ts:363-392` — the ensure-running gate before proxying (503/500 otherwise).
10. `crates/corelink-container/src/routes.rs:411-413` — the container's composed router receiving the request.
11. `crates/corelink-container/src/routes.rs:474-583` — the shared CAS/AC handlers (accounting + tombstone + PAT gate) executing the op.
