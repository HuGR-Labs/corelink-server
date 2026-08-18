---
type: "Plane"
title: "Worker → DO → Container request flow"
description: "The end-to-end path a cache request takes: edge auth + routing in the Worker, lifecycle + proxy in the per-tenant Durable Object, handler execution in the Rust container."
source_files:
  - "worker/src/index.ts"
  - "worker/src/durable_object.ts"
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/origin_timing.rs"
source_blobs:
  - "worker/src/index.ts@9a04ddf2001f6cc750fe7aac327d1a2f69fa910c"
  - "worker/src/durable_object.ts@bf36fd6a2f94c92c8b5d47872dedb93cfc95d137"
  - "crates/corelink-container/src/routes.rs@fe49db9455326e980ee761012a5990c8a5e30a42"
  - "crates/corelink-container/src/origin_timing.rs@6eac1fc37428babeeb919d51d7f234a2d41c501c"
checkpoint_sha: "018e63e6f823a1802852d8d3ecd4eba834928c71"
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
   tenant (`worker/src/index.ts:1865-1911`).
2. The Worker authenticates the Bearer PAT — HMAC fast-reject then a D1 `token_id` lookup + expiry —
   resolving the trusted tenant (`worker/src/index.ts:1248-1406`). EXCEPTION (Artifact 1): the unauth
   `/v1/public/*` arm (the erasure-attestation verifier) is matched BEFORE the generic `/v1/*` PAT bucket
   and forwarded as `_anonymous` with NO PAT and NO internal-auth — an erasure proof is publicly
   verifiable, so this route skips the auth hop entirely (matchRoute arm `worker/src/index.ts:1062-1063`,
   forward arm `worker/src/index.ts:2598-2626`). The edge expiry check honors the
   `expires_ms === 0` "never expires" sentinel (`row.expires_ms !== 0 && row.expires_ms <= now`),
   matching the container's `adapter_pat` SQL (`expires_ms = 0 OR expires_ms > now`) — so a no-TTL PAT
   is no longer a split-brain edge-reject that worked in the container but died at the Worker. If that
   D1 lookup itself faults (network partition / DB unavailable) the auth hop fails CLOSED but is mapped
   to a retryable 503, not a 401 — the request never reaches the DO, and a transient infra fault is
   never surfaced to the client as "bad credentials" (H1).
3. The Worker derives the per-tenant DO with `idFromName(resolvedTenantId)`, making isolation structural
   (`worker/src/index.ts:3434-3436`). A non-local-region tenant may first be routed to the LOCAL
   (this-region) `_system` container branch above this forward; the fall-through then lands on the
   per-tenant DO derivation here (`worker/src/index.ts:2141-2143`).
4. It strips any client-supplied trust headers (delete-then-set discipline), sets its own verified
   tenant-id/scope/token-prefix, and (absent an edge-serve short-circuit, next point) dispatches via
   `stub.fetch` (`worker/src/index.ts:3439-3470`, `worker/src/index.ts:3581`).
4b. **F3.3 F2 SERVE short-circuits the DO→container hop entirely on a `brew`/`pip` `_public` cache HIT.**
   With `env.EDGE_PUBLIC_READ === "serve"` and a GET on those two route kinds, the Worker reads the blob
   itself from the native `CONFIG_DB` map + `CAS_BUCKET` R2 (`readPublicHit`) BEFORE ever building the
   `stub.fetch` call — a hit sets `doResponse = edgeServed` and the container is never dispatched, which is
   the F3.3 removal of the `origin` phase (~585 ms) from this path; a miss, revocation, re-hash mismatch, or
   thrown fault yields `null`/falls into the `catch` and the request falls through unchanged to the
   container path below, so this can only make a HIT faster, never change correctness
   (`worker/src/index.ts:3515-3576`). A served HIT now sets `Content-Type: <PUBLIC_BLOB_CONTENT_TYPE>` and
   `Accept-Ranges: bytes` (faithful to what the container's own binary read returns, previously dropped on
   this path) and honors a client `Range` header — `parseByteRange` yields a `206 Partial Content` slice
   with `Content-Range`, a `416` with `Content-Range: bytes */<total>` on an unsatisfiable range, or the
   full `200` body otherwise (`worker/src/index.ts:3540-3564`). This edge HIT deliberately does **not**
   pass through the container's per-op $-ceiling gate (ADR-0068) — request-count and storage quota
   (`runQuotaBatch`, already run above this block) still apply, but the spend cap does not, an intentional
   exemption for the cheapest, most-shared traffic class (`worker/src/index.ts:3532-3539`). The Worker
   deliberately does NOT stamp `stOrigin*` on an edge-served response, so `Server-Timing` simply omits the
   `origin` phase — that absence is itself the wire-level proof the container was bypassed
   (`worker/src/index.ts:3522-3523`).
4c. **The `_public` edge-serve cache of 4b is invalidated out-of-band by `/_internal/public/revoke` (B1b).**
   That internal route forwards the revoke to the `_system` DO → container — the authoritative D1 blocklist
   + `cache_map` delete + R2 erase (`worker/src/index.ts:2259`) — and, only on the container's `ok` response
   with `METADATA_KV` bound, best-effort-writes a content-hash-keyed edge blocklist KV via
   `ctx.waitUntil(writePublicBlocklistKv(...))` (`worker/src/index.ts:2285`), so a revoked `_public` hash
   stops edge-serving within KV propagation (~seconds) instead of waiting out the ~60 s map-cache TTL. The
   KV write never fails the revoke the container already applied — a KV fault or a non-`{content_hash}`
   body just falls back to the map-cache window (`worker/src/index.ts:2275`).
5. The DO's `fetch` binds the forwarded tenant-id, ensures the container is running, then proxies the
   request (`worker/src/durable_object.ts:541-642`).
6. The proxy rewrites the request onto `http://localhost:50051` through the `getTcpPort` fetcher — the
   DO→container hop (`worker/src/durable_object.ts:345-358`).
7. The container's composed router (built by `build_with_factory`) receives the request and routes it to
   the matching handler (`crates/corelink-container/src/routes.rs:431-433`).
8. The shared CAS/AC handler objects — wrapped once with byte-accounting, the erasure tombstone gate,
   and the native PAT possession backstop — execute the actual cache operation
   (`crates/corelink-container/src/routes.rs:494-603`).
9. The hop is MEASURED end to end, and the measurement is split across the same boundary the request
   crosses. The container's outermost data-plane layer clocks its whole share of the request and stamps
   `opat` / `oquota` / `ostore` / `oother` onto the response's own `Server-Timing`
   (`crates/corelink-container/src/origin_timing.rs:265-273`, wired last so it wraps every inner layer at
   `crates/corelink-container/src/routes.rs:1072-1074`); the Worker forwards those four and derives the
   one term only it can see, `ohop = origin − Σ(container phases)` — the dispatch, the DO's prologue and
   the wire (`worker/src/index.ts:3871`). Recording is a task-local ledger, so an instrumented region
   reached outside a request (a test, a background task) simply records nothing
   (`crates/corelink-container/src/origin_timing.rs:247-252`).

# Invariants
- The DO is always selected from the PAT-resolved tenant, never the URL tenant — isolation is
  established at this hop (`worker/src/index.ts:3434-3436`).
- The request that crosses Worker→DO carries only Worker-established trust headers; client values are
  stripped first (`worker/src/index.ts:3439-3470`).
- The DO→container hop always targets port 50051 via the `getTcpPort` fetcher
  (`worker/src/durable_object.ts:345-358`).
- The container re-verifies possession at the shared handler chokepoint rather than trusting the hop
  blindly (`crates/corelink-container/src/routes.rs:494-603`).
- The DO will not proxy until the container is confirmed running (or it returns 503/500)
  (`worker/src/durable_object.ts:573-602`).
- The `origin` split always reconciles: the container's residue phase is computed against its OWN
  whole-request clock, so its parts sum exactly to the time it held the request
  (`crates/corelink-container/src/origin_timing.rs:225`), and the Worker publishes no split it cannot
  make add up (`worker/src/index.ts:3868-3869`).
- The DO→container hop is not mandatory on every request: a `brew`/`pip` `_public` edge-serve HIT sets
  `doResponse` directly from the Worker-local read and skips `stub.fetch` altogether, but only ever as a
  strictly-faster substitute for an outcome the container would also have served — any miss/fault falls
  through to the unchanged container path, so this hop is optional for speed, never for correctness
  (`worker/src/index.ts:3515-3576`). An edge-served HIT is exempt from the container's $-ceiling gate but
  not from request-count/storage quota, which the Worker has already enforced upstream of this block
  (`worker/src/index.ts:3532-3539`).

# Gotchas
- OCI, the Stripe webhook, and fabric-introspect take dedicated pass-through arms in the Worker that
  skip the PAT gate; they still traverse the same DO→container hop but the container is the sole auth
  authority for them.
- The DO never sets internal-auth on the data-plane forward, so the container's internal-auth-gated
  admin routes are Worker-unreachable by design (operator-only posture).

# Citations
1. `worker/src/index.ts:1-20` — the `Internet → Worker → DO → container` topology header.
2. `worker/src/index.ts:1248-1406` — edge PAT auth (HMAC fast-reject + D1 lookup + expiry).
3. `worker/src/index.ts:1865-1911` — the Worker `fetch` entry + `matchRoute`.
4. `worker/src/index.ts:3434-3436` — `idFromName(resolvedTenantId)` DO derivation (structural isolation).
5. `worker/src/index.ts:3439-3470` — strip-then-set trust headers on the forward.
6. `worker/src/index.ts:3439-3470` — the augmented forward (strip-then-set trust headers); `worker/src/index.ts:3581` — the `stub.fetch` dispatch to the DO (reached only when 6b did not already set `doResponse`).
6b. `worker/src/index.ts:3515-3576` — F3.3 F2 SERVE: the `EDGE_PUBLIC_READ === "serve"` short-circuit on a `brew`/`pip` `_public` HIT — `readPublicHit` at `worker/src/index.ts:3531`, the $-ceiling exemption rationale at `worker/src/index.ts:3532-3539`, the `Content-Type`/`Accept-Ranges` headers at `worker/src/index.ts:3545-3547`, and the `parseByteRange`-driven `200`/`206`/`416` branch at `worker/src/index.ts:3549-3564`; the deliberate omission of `stOrigin*` (so `Server-Timing` proves the bypass by omitting `origin`) at `worker/src/index.ts:3522-3523`.
6c. `worker/src/index.ts:2259` — the `/_internal/public/revoke` invalidation seam (B1b): the revoke forwards to the `_system` DO → container (authoritative D1 blocklist + `cache_map` delete + R2 erase) and, on an `ok` response with `METADATA_KV` bound (`worker/src/index.ts:2275`), best-effort-writes the content-hash-keyed edge blocklist KV via `ctx.waitUntil(writePublicBlocklistKv(...))` (`worker/src/index.ts:2285`), collapsing the `_public` edge-serve revocation window from the ~60 s map-cache TTL to KV propagation without ever failing the revoke on a KV fault.
7. `worker/src/durable_object.ts:345-358` — the DO→container proxy via `getTcpPort(50051)`.
8. `worker/src/durable_object.ts:541-642` — the DO `fetch`: tenant bind, ensure-running, proxy.
9. `worker/src/durable_object.ts:573-602` — the ensure-running gate before proxying (503/500 otherwise).
10. `crates/corelink-container/src/routes.rs:431-433` — the container's composed router receiving the request.
11. `crates/corelink-container/src/routes.rs:494-603` — the shared CAS/AC handlers (accounting + tombstone + PAT gate) executing the op.
12. `crates/corelink-container/src/origin_timing.rs:265-273` — the container's outermost data-plane layer: scope a task-local phase ledger over the request, clock the whole of it, and stamp the phases on the response's `Server-Timing`. Wired last (so it wraps every inner layer) at `crates/corelink-container/src/routes.rs:1072-1074`; the residue that makes the parts sum to the whole is `crates/corelink-container/src/origin_timing.rs:225`; `timed` is the pass-through recorder at `crates/corelink-container/src/origin_timing.rs:247-252`.
