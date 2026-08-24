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
  - "worker/src/index.ts@7a3789b04e1c4e65e5e850b2efa7eb907d875e27"
  - "worker/src/durable_object.ts@bf36fd6a2f94c92c8b5d47872dedb93cfc95d137"
  - "crates/corelink-container/src/routes.rs@8603539ebcedf67dcb10e9265efdb688aae9b97e"
  - "crates/corelink-container/src/origin_timing.rs@c22f7ca2a309a60c8da1b599cdbeda68301ee6c8"
checkpoint_sha: "f642a7dfe0ee8532d17d14067e12f2972d9c4b16"
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
   tenant (`worker/src/index.ts:1919-1971`).
2. The Worker authenticates the Bearer PAT — HMAC fast-reject then a D1 `token_id` lookup + expiry —
   resolving the trusted tenant (`worker/src/index.ts:1302-1460`). EXCEPTION (Artifact 1): the unauth
   `/v1/public/*` arm (the erasure-attestation verifier) is matched BEFORE the generic `/v1/*` PAT bucket
   and forwarded as `_anonymous` with NO PAT and NO internal-auth — an erasure proof is publicly
   verifiable, so this route skips the auth hop entirely (matchRoute arm `worker/src/index.ts:1116-1117`,
   forward arm `worker/src/index.ts:2756-2784`). The edge expiry check honors the
   `expires_ms === 0` "never expires" sentinel (`row.expires_ms !== 0 && row.expires_ms <= now`),
   matching the container's `adapter_pat` SQL (`expires_ms = 0 OR expires_ms > now`) — so a no-TTL PAT
   is no longer a split-brain edge-reject that worked in the container but died at the Worker. If that
   D1 lookup itself faults (network partition / DB unavailable) the auth hop fails CLOSED but is mapped
   to a retryable 503, not a 401 — the request never reaches the DO, and a transient infra fault is
   never surfaced to the client as "bad credentials" (H1).
3. The Worker derives the per-tenant DO with `idFromName(resolvedTenantId)`, making isolation structural
   (`worker/src/index.ts:3752-3754`). A non-local-region tenant may first be routed to the LOCAL
   (this-region) `_system` container branch above this forward; the fall-through then lands on the
   per-tenant DO derivation here (`worker/src/index.ts:2201-2203`).
4. It strips any client-supplied trust headers (delete-then-set discipline), sets its own verified
   tenant-id/scope/token-prefix, and (absent an edge-serve short-circuit, next point) dispatches via
   `stub.fetch` (`worker/src/index.ts:3757-3788`, `worker/src/index.ts:3899`).
4b. **F3.3 F2 SERVE short-circuits the DO→container hop entirely on a `brew`/`pip` `_public` cache HIT.**
   With `env.EDGE_PUBLIC_READ === "serve"` and a GET on those two route kinds, the Worker reads the blob
   itself from the native `CONFIG_DB` map + `CAS_BUCKET` R2 (`readPublicHit`) BEFORE ever building the
   `stub.fetch` call — a hit sets `doResponse = edgeServed` and the container is never dispatched, which is
   the F3.3 removal of the `origin` phase (~585 ms) from this path; a miss, revocation, re-hash mismatch, or
   thrown fault yields `null`/falls into the `catch` and the request falls through unchanged to the
   container path below, so this can only make a HIT faster, never change correctness
   (`worker/src/index.ts:3833-3894`). A served HIT now sets `Content-Type: <PUBLIC_BLOB_CONTENT_TYPE>` and
   `Accept-Ranges: bytes` (faithful to what the container's own binary read returns, previously dropped on
   this path) and honors a client `Range` header — `parseByteRange` yields a `206 Partial Content` slice
   with `Content-Range`, a `416` with `Content-Range: bytes */<total>` on an unsatisfiable range, or the
   full `200` body otherwise (`worker/src/index.ts:3858-3882`). This edge HIT deliberately does **not**
   pass through the container's per-op $-ceiling gate (ADR-0068) — request-count and storage quota
   (`runQuotaBatch`, already run above this block) still apply, but the spend cap does not, an intentional
   exemption for the cheapest, most-shared traffic class (`worker/src/index.ts:3850-3857`). The Worker
   deliberately does NOT stamp `stOrigin*` on an edge-served response, so `Server-Timing` simply omits the
   `origin` phase — that absence is itself the wire-level proof the container was bypassed
   (`worker/src/index.ts:3840-3841`).
4c. **The `_public` edge-serve cache of 4b is invalidated out-of-band by `/_internal/public/revoke` (B1b).**
   That internal route forwards the revoke to the `_system` DO → container — the authoritative D1 blocklist
   + `cache_map` delete + R2 erase (`worker/src/index.ts:2401`) — and, only on the container's `ok` response
   with `METADATA_KV` bound (`worker/src/index.ts:2417`), reads the RESOLVED `content_hash` from the
   container's buffered-and-rebuilt RESPONSE body (`worker/src/index.ts:2427`, F-1: authoritative for BOTH
   revoke spaces, so a revoke-by-`upstream_digest` — whose REQUEST carries no `content_hash` — collapses the
   edge window too) and best-effort-writes a content-hash-keyed edge blocklist KV via
   `ctx.waitUntil(writePublicBlocklistKv(...))` (`worker/src/index.ts:2443`), so a revoked `_public` hash
   stops edge-serving within KV propagation (~seconds) instead of waiting out the ~60 s map-cache TTL. The
   KV write never fails the revoke the container already applied — a KV fault or a non-`{content_hash}`
   response body just falls back to the map-cache window.
4d. **The `wdb` window also carries an optional P3 EDGE_DO_METER hop — a SEPARATE DO pair from the
   per-tenant `CoreLinkServer` of steps 5-6 — now separately clocked.** When `EDGE_DO_METER === "serve"`
   for this region and the request is genuinely counted (`meter = !isFanout && requestQuotaEnabled`,
   `worker/src/index.ts:3249`), the Worker AWAITS `serveViaDO(...)` (the request-meter shard/coordinator
   pair) inside a `try/finally` so the elapsed time (`stQDoMs`) is captured even on the fail-open `catch`
   path (`worker/src/index.ts:3290-3311`). That duration is reported as the `qdo` Server-Timing phase,
   and `wdb`'s leftover as `qother`, but ONLY when the operator flag `SERVER_TIMING_WDB_DETAIL === "on"`
   (unset by default) — see [the Worker edge plane](/planes/worker-edge.md) for the full `wdb` breakdown
   and why the gate exists (an unflagged `qdo` would leak whether a client-supplied
   `x-corelink-fanout-from` header matched `CORELINK_INTERNAL_AUTH_KEY`).
5. The DO's `fetch` binds the forwarded tenant-id, ensures the container is running, then proxies the
   request (`worker/src/durable_object.ts:541-642`).
6. The proxy rewrites the request onto `http://localhost:50051` through the `getTcpPort` fetcher — the
   DO→container hop (`worker/src/durable_object.ts:345-358`).
7. The container's composed router (built by `build_with_factory`) receives the request and routes it to
   the matching handler (`crates/corelink-container/src/routes.rs:448-450`).
8. The shared CAS/AC handler objects — wrapped once with byte-accounting, the erasure tombstone gate,
   and the native PAT possession backstop — execute the actual cache operation
   (`crates/corelink-container/src/routes.rs:550-659`).
9. The hop is MEASURED end to end, and the measurement is split across the same boundary the request
   crosses. The container's outermost data-plane layer clocks its whole share of the request and stamps
   `opat` / `oquota` / `ostore` / `oother` onto the response's own `Server-Timing`
   (`crates/corelink-container/src/origin_timing.rs:428-436`, wired last so it wraps every inner layer at
   `crates/corelink-container/src/routes.rs:1134-1136`); the Worker forwards those four and derives the
   one term only it can see, `ohop = origin − Σ(container phases)` — the dispatch, the DO's prologue and
   the wire (`worker/src/index.ts:4260`). Recording is a task-local ledger, so an instrumented region
   reached outside a request (a test, a background task) simply records nothing
   (`crates/corelink-container/src/origin_timing.rs:341-346`).
9b. **W2 split `oother` into three named regions on the PAT-verification path**, `oargon` /
   `opermit` / `ortier` — the container's Argon2id verify (memo check + coalesced flight, and the
   row-not-found dummy burn, under the SAME `oargon` name so the two arms stay indistinguishable),
   the `ARGON2_PERMIT_WAIT`-bounded semaphore acquires (`opermit`), and `ensure_tier_applied`'s D1
   tier-label read (`ortier`) (`crates/corelink-container/src/origin_timing.rs:140-154`). Because the
   Argon2id flight SPAWNS its lead future onto a task with no ambient task-local ledger, `oargon`/
   `opermit` are recorded via a ledger HANDLE captured on the originating task before the spawn
   (`current_ledger` + `PhaseScope::with_handle`), not the ambient-task-local path `timed`/
   `PhaseScope::enter` use everywhere else (`crates/corelink-container/src/origin_timing.rs:361-364`).
   All three are gated OFF by default behind `CORELINK_ORIGIN_TIMING_DETAIL=on`
   (`crates/corelink-container/src/origin_timing.rs:312-315`): `opermit`'s mere PRESENCE reveals
   whether the Argon2id flight ran at all, i.e. whether a `secret_match_memo` hit was warm for that
   exact credential, and the dummy burn exists precisely so a missing/expired/revoked `token_id` is
   timing-indistinguishable from a valid one — partitioning that residue by name would erode the
   padding it provides. Off is the production default, and the header is byte-identical to what
   shipped before the split; the ledger still records the three phases unconditionally, so arming the
   gate needs no rebuild (`crates/corelink-container/src/origin_timing.rs:216`, `crates/corelink-container/src/origin_timing.rs:312-315`).

# Invariants
- The DO is always selected from the PAT-resolved tenant, never the URL tenant — isolation is
  established at this hop (`worker/src/index.ts:3752-3754`).
- The request that crosses Worker→DO carries only Worker-established trust headers; client values are
  stripped first (`worker/src/index.ts:3757-3788`).
- The DO→container hop always targets port 50051 via the `getTcpPort` fetcher
  (`worker/src/durable_object.ts:345-358`).
- The container re-verifies possession at the shared handler chokepoint rather than trusting the hop
  blindly (`crates/corelink-container/src/routes.rs:550-659`).
- The DO will not proxy until the container is confirmed running (or it returns 503/500)
  (`worker/src/durable_object.ts:573-602`).
- The `origin` split always reconciles: the container's residue phase is computed against its OWN
  whole-request clock, so its parts sum exactly to the time it held the request
  (`crates/corelink-container/src/origin_timing.rs:282`), and the Worker publishes no split it cannot
  make add up (`worker/src/index.ts:4257-4258`). That reconciliation holds with `oargon`/`opermit`/
  `ortier` present OR absent — the gate just moves their time between the named phases and `oother`,
  never off the ledger (`crates/corelink-container/src/origin_timing.rs:255-284`).
- The DO→container hop is not mandatory on every request: a `brew`/`pip` `_public` edge-serve HIT sets
  `doResponse` directly from the Worker-local read and skips `stub.fetch` altogether, but only ever as a
  strictly-faster substitute for an outcome the container would also have served — any miss/fault falls
  through to the unchanged container path, so this hop is optional for speed, never for correctness
  (`worker/src/index.ts:3833-3894`). An edge-served HIT is exempt from the container's $-ceiling gate but
  not from request-count/storage quota, which the Worker has already enforced upstream of this block
  (`worker/src/index.ts:3850-3857`).

# Gotchas
- OCI, the Stripe webhook, and fabric-introspect take dedicated pass-through arms in the Worker that
  skip the PAT gate; they still traverse the same DO→container hop but the container is the sole auth
  authority for them.
- The DO never sets internal-auth on the data-plane forward, so the container's internal-auth-gated
  admin routes are Worker-unreachable by design (operator-only posture).

# Citations
1. `worker/src/index.ts:1-20` — the `Internet → Worker → DO → container` topology header.
2. `worker/src/index.ts:1302-1460` — edge PAT auth (HMAC fast-reject + D1 lookup + expiry).
3. `worker/src/index.ts:1919-1971` — the Worker `fetch` entry + `matchRoute`.
4. `worker/src/index.ts:3752-3754` — `idFromName(resolvedTenantId)` DO derivation (structural isolation).
5. `worker/src/index.ts:3757-3788` — strip-then-set trust headers on the forward.
6. `worker/src/index.ts:3757-3788` — the augmented forward (strip-then-set trust headers); `worker/src/index.ts:3899` — the `stub.fetch` dispatch to the DO (reached only when 6b did not already set `doResponse`).
6b. `worker/src/index.ts:3833-3894` — F3.3 F2 SERVE: the `EDGE_PUBLIC_READ === "serve"` short-circuit on a `brew`/`pip` `_public` HIT — `readPublicHit` at `worker/src/index.ts:3849`, the $-ceiling exemption rationale at `worker/src/index.ts:3850-3857`, the `Content-Type`/`Accept-Ranges` headers at `worker/src/index.ts:3863-3865`, and the `parseByteRange`-driven `200`/`206`/`416` branch at `worker/src/index.ts:3867-3882`; the deliberate omission of `stOrigin*` (so `Server-Timing` proves the bypass by omitting `origin`) at `worker/src/index.ts:3840-3841`.
6c. `worker/src/index.ts:2401` — the `/_internal/public/revoke` invalidation seam (B1b): the revoke forwards to the `_system` DO → container (authoritative D1 blocklist + `cache_map` delete + R2 erase) and, on an `ok` response with `METADATA_KV` bound (`worker/src/index.ts:2417`), reads the RESOLVED `content_hash` from the container's buffered RESPONSE body (`worker/src/index.ts:2427`, F-1: authoritative for both the raw `content_hash` and the `upstream_digest` revoke spaces) and best-effort-writes the content-hash-keyed edge blocklist KV via `ctx.waitUntil(writePublicBlocklistKv(...))` (`worker/src/index.ts:2443`), collapsing the `_public` edge-serve revocation window from the ~60 s map-cache TTL to KV propagation without ever failing the revoke on a KV fault.
6d. `worker/src/index.ts:3249` — `meter = !isFanout && requestQuotaEnabled`, the gate that decides whether the P3 `qdo` hop runs at all; `worker/src/index.ts:3290-3311` — the awaited `serveViaDO(...)` call timed in a `try/finally` (`stQDoMs`, captured even on the fail-open `catch`); `worker/src/index.ts:4014-4042` — the `qdo`/`qother` emission, gated on `SERVER_TIMING_WDB_DETAIL === "on"`.
7. `worker/src/durable_object.ts:345-358` — the DO→container proxy via `getTcpPort(50051)`.
8. `worker/src/durable_object.ts:541-642` — the DO `fetch`: tenant bind, ensure-running, proxy.
9. `worker/src/durable_object.ts:573-602` — the ensure-running gate before proxying (503/500 otherwise).
10. `crates/corelink-container/src/routes.rs:448-450` — the container's composed router receiving the request.
11. `crates/corelink-container/src/routes.rs:550-659` — the shared CAS/AC handlers (accounting + tombstone + PAT gate) executing the op.
12. `crates/corelink-container/src/origin_timing.rs:428-436` — the container's outermost data-plane layer: scope a task-local phase ledger over the request, clock the whole of it, and stamp the phases on the response's `Server-Timing`. Wired last (so it wraps every inner layer) at `crates/corelink-container/src/routes.rs:1134-1136`; the residue that makes the parts sum to the whole is `crates/corelink-container/src/origin_timing.rs:282`; `timed` is the pass-through recorder at `crates/corelink-container/src/origin_timing.rs:341-346`.
13. `crates/corelink-container/src/origin_timing.rs:140-154` — the `Phase::Argon` / `Phase::Permit` / `Phase::Tier` variants W2 split out of `oother`: the PAT Argon2id region (found arm AND row-not-found dummy burn, same name), the `ARGON2_PERMIT_WAIT` semaphore acquires, and `ensure_tier_applied`'s D1 read.
14. `crates/corelink-container/src/origin_timing.rs:361-364` — `current_ledger`: captures a handle to the ambient ledger on the ORIGINATING task, for a region (the Argon2id `FlightGroup`'s spawned lead future) that runs on a different task and cannot see the task-local `timed`/`PhaseScope::enter` rely on.
15. `crates/corelink-container/src/origin_timing.rs:312-315` — `detail_phases_enabled`: reads `CORELINK_ORIGIN_TIMING_DETAIL`, off by default and load-bearing — `opermit` presence is a warm-memo oracle, and the dummy burn's padding is timing that a named split would erode.
