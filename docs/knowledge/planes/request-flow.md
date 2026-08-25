---
type: "Plane"
title: "Worker → DO → Container request flow"
description: "The end-to-end path a cache request takes: edge auth + routing in the Worker, lifecycle + proxy in the per-tenant Durable Object, handler execution in the Rust container."
source_files:
  - "worker/src/index.ts"
  - "worker/src/durable_object.ts"
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/origin_timing.rs"
  - "crates/corelink-container/src/storage/d1_audit_sink.rs"
  - "crates/corelink-container/src/storage/r2_s3.rs"
source_blobs:
  - "worker/src/index.ts@74f79e1cdc7219b1761b56ce3fab1fadb433691b"
  - "worker/src/durable_object.ts@bf36fd6a2f94c92c8b5d47872dedb93cfc95d137"
  - "crates/corelink-container/src/routes.rs@8603539ebcedf67dcb10e9265efdb688aae9b97e"
  - "crates/corelink-container/src/origin_timing.rs@70ce6eddc7bd07e8036ba93f73249a2d9618d4f6"
  - "crates/corelink-container/src/storage/d1_audit_sink.rs@4273f2100e5fbaf87bb4dc819a84db1d402160b4"
  - "crates/corelink-container/src/storage/r2_s3.rs@d5304f1845e9e9ae5972a3a207dcc146ecd927bb"
checkpoint_sha: "3a17e78abb15c71802f22dfac3c8cba71233f21b"
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
   tenant (`worker/src/index.ts:1919-1987`).
2. The Worker authenticates the Bearer PAT — HMAC fast-reject then a D1 `token_id` lookup + expiry —
   resolving the trusted tenant (`worker/src/index.ts:1302-1460`). EXCEPTION (Artifact 1): the unauth
   `/v1/public/*` arm (the erasure-attestation verifier) is matched BEFORE the generic `/v1/*` PAT bucket
   and forwarded as `_anonymous` with NO PAT and NO internal-auth — an erasure proof is publicly
   verifiable, so this route skips the auth hop entirely (matchRoute arm `worker/src/index.ts:1116-1117`,
   forward arm `worker/src/index.ts:2772-2800`). The edge expiry check honors the
   `expires_ms === 0` "never expires" sentinel (`row.expires_ms !== 0 && row.expires_ms <= now`),
   matching the container's `adapter_pat` SQL (`expires_ms = 0 OR expires_ms > now`) — so a no-TTL PAT
   is no longer a split-brain edge-reject that worked in the container but died at the Worker. If that
   D1 lookup itself faults (network partition / DB unavailable) the auth hop fails CLOSED but is mapped
   to a retryable 503, not a 401 — the request never reaches the DO, and a transient infra fault is
   never surfaced to the client as "bad credentials" (H1).
3. The Worker derives the per-tenant DO with `idFromName(resolvedTenantId)`, making isolation structural
   (`worker/src/index.ts:3844-3846`). A non-local-region tenant may first be routed to the LOCAL
   (this-region) `_system` container branch above this forward; the fall-through then lands on the
   per-tenant DO derivation here (`worker/src/index.ts:2217-2219`).
4. It strips any client-supplied trust headers (delete-then-set discipline), sets its own verified
   tenant-id/scope/token-prefix, and (absent an edge-serve short-circuit, next point) dispatches via
   `stub.fetch` (`worker/src/index.ts:3849-3880`, `worker/src/index.ts:3991`).
4b. **F3.3 F2 SERVE short-circuits the DO→container hop entirely on a `brew`/`pip` `_public` cache HIT.**
   With `env.EDGE_PUBLIC_READ === "serve"` and a GET on those two route kinds, the Worker reads the blob
   itself from the native `CONFIG_DB` map + `CAS_BUCKET` R2 (`readPublicHit`) BEFORE ever building the
   `stub.fetch` call — a hit sets `doResponse = edgeServed` and the container is never dispatched, which is
   the F3.3 removal of the `origin` phase (~585 ms) from this path; a miss, revocation, re-hash mismatch, or
   thrown fault yields `null`/falls into the `catch` and the request falls through unchanged to the
   container path below, so this can only make a HIT faster, never change correctness
   (`worker/src/index.ts:3925-3986`). A served HIT now sets `Content-Type: <PUBLIC_BLOB_CONTENT_TYPE>` and
   `Accept-Ranges: bytes` (faithful to what the container's own binary read returns, previously dropped on
   this path) and honors a client `Range` header — `parseByteRange` yields a `206 Partial Content` slice
   with `Content-Range`, a `416` with `Content-Range: bytes */<total>` on an unsatisfiable range, or the
   full `200` body otherwise (`worker/src/index.ts:3950-3974`). This edge HIT deliberately does **not**
   pass through the container's per-op $-ceiling gate (ADR-0068) — request-count and storage quota
   (`runQuotaBatch`, already run above this block) still apply, but the spend cap does not, an intentional
   exemption for the cheapest, most-shared traffic class (`worker/src/index.ts:3942-3949`). The Worker
   deliberately does NOT stamp `stOrigin*` on an edge-served response, so `Server-Timing` simply omits the
   `origin` phase — that absence is itself the wire-level proof the container was bypassed
   (`worker/src/index.ts:3932-3933`).
4c. **The `_public` edge-serve cache of 4b is invalidated out-of-band by `/_internal/public/revoke` (B1b).**
   That internal route forwards the revoke to the `_system` DO → container — the authoritative D1 blocklist
   + `cache_map` delete + R2 erase (`worker/src/index.ts:2417`) — and, only on the container's `ok` response
   with `METADATA_KV` bound (`worker/src/index.ts:2433`), reads the RESOLVED `content_hash` from the
   container's buffered-and-rebuilt RESPONSE body (`worker/src/index.ts:2443`, F-1: authoritative for BOTH
   revoke spaces, so a revoke-by-`upstream_digest` — whose REQUEST carries no `content_hash` — collapses the
   edge window too) and best-effort-writes a content-hash-keyed edge blocklist KV via
   `ctx.waitUntil(writePublicBlocklistKv(...))` (`worker/src/index.ts:2459`), so a revoked `_public` hash
   stops edge-serving within KV propagation (~seconds) instead of waiting out the ~60 s map-cache TTL. The
   KV write never fails the revoke the container already applied — a KV fault or a non-`{content_hash}`
   response body just falls back to the map-cache window.
4d. **The `wdb` window also carries an optional P3 EDGE_DO_METER hop — a SEPARATE DO pair from the
   per-tenant `CoreLinkServer` of steps 5-6 — now separately clocked.** When `EDGE_DO_METER === "serve"`
   for this region and the request is genuinely counted (`meter = !isFanout && requestQuotaEnabled`,
   `worker/src/index.ts:3248`), the Worker AWAITS `serveViaDO(...)` (the request-meter shard/coordinator
   pair) inside a `try/finally` so the elapsed time (`stQDoMs`) is captured even on the fail-open `catch`
   path (`worker/src/index.ts:3301-3347`). That duration is reported as the `qdo` Server-Timing phase,
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
   (`crates/corelink-container/src/origin_timing.rs:461-470`, wired last so it wraps every inner layer at
   `crates/corelink-container/src/routes.rs:1134-1136`); the Worker forwards those four and derives the
   one term only it can see, `ohop = origin − Σ(container phases)` — the dispatch, the DO's prologue and
   the wire (`worker/src/index.ts:4505`). Recording is a task-local ledger, so an instrumented region
   reached outside a request (a test, a background task) simply records nothing
   (`crates/corelink-container/src/origin_timing.rs:374-380`).
9b. **W2 split `oother` into three named regions on the PAT-verification path**, `oargon` /
   `opermit` / `ortier` — the container's Argon2id verify (memo check + coalesced flight, and the
   row-not-found dummy burn, under the SAME `oargon` name so the two arms stay indistinguishable),
   the `ARGON2_PERMIT_WAIT`-bounded semaphore acquires (`opermit`), and `ensure_tier_applied`'s D1
   tier-label read (`ortier`) (`crates/corelink-container/src/origin_timing.rs:152-166`). Because the
   Argon2id flight SPAWNS its lead future onto a task with no ambient task-local ledger, `oargon`/
   `opermit` are recorded via a ledger HANDLE captured on the originating task before the spawn
   (`current_ledger` + `PhaseScope::with_handle`), not the ambient-task-local path `timed`/
   `PhaseScope::enter` use everywhere else (`crates/corelink-container/src/origin_timing.rs:395-397`).
   All three are gated OFF by default behind `CORELINK_ORIGIN_TIMING_DETAIL=on`
   (`crates/corelink-container/src/origin_timing.rs:345-348`): `opermit`'s mere PRESENCE reveals
   whether the Argon2id flight ran at all, i.e. whether a `secret_match_memo` hit was warm for that
   exact credential, and the dummy burn exists precisely so a missing/expired/revoked `token_id` is
   timing-indistinguishable from a valid one — partitioning that residue by name would erode the
   padding it provides. Off is the production default, and the header is byte-identical to what
   shipped before the split; the ledger still records the three phases unconditionally, so arming the
   gate needs no rebuild (`crates/corelink-container/src/origin_timing.rs:236-244`, `crates/corelink-container/src/origin_timing.rs:345-348`).
9c. **A fourth phase, `oaudit`, joined the same gated group**: the blocking D1-over-HTTP write inside
   `D1AuditOutboxSink::write_blocking` — the ONE choke point every native-plane CAS/AC `AuditSink::emit`
   call routes through before/after a read or mutation — is timed into `Phase::Audit` via a
   `PhaseScope` opened at the top of that method
   (`crates/corelink-container/src/storage/d1_audit_sink.rs:165-173`). Separately, and UNGATED, the same
   native CAS/AC handlers' R2/S3 object GET/PUT/DELETE/LIST calls (`R2CasHandler`/`R2AcHandler`, made
   through the sync `block_in_place` bridge) are now wrapped into the EXISTING `ostore` phase rather than
   falling into `oother`
   (`crates/corelink-container/src/storage/r2_s3.rs:1158-1162`,
   `crates/corelink-container/src/storage/r2_s3.rs:2215-2219`). Both additions follow the same
   `PhaseScope::enter` pattern as `oargon`/`opermit`/`ortier` and change no ordering or error handling —
   `PhaseScope`'s `Drop` records on every exit path, so a downstream error still gets timed.

# Invariants
- The DO is always selected from the PAT-resolved tenant, never the URL tenant — isolation is
  established at this hop (`worker/src/index.ts:3844-3846`).
- The request that crosses Worker→DO carries only Worker-established trust headers; client values are
  stripped first (`worker/src/index.ts:3849-3880`).
- The DO→container hop always targets port 50051 via the `getTcpPort` fetcher
  (`worker/src/durable_object.ts:345-358`).
- The container re-verifies possession at the shared handler chokepoint rather than trusting the hop
  blindly (`crates/corelink-container/src/routes.rs:550-659`).
- The DO will not proxy until the container is confirmed running (or it returns 503/500)
  (`worker/src/durable_object.ts:573-602`).
- The `origin` split always reconciles: the container's residue phase is computed against its OWN
  whole-request clock, so its parts sum exactly to the time it held the request
  (`crates/corelink-container/src/origin_timing.rs:308`), and the Worker publishes no split it cannot
  make add up (`worker/src/index.ts:4502-4503`). That reconciliation holds with `oargon`/`opermit`/
  `ortier`/`oaudit` present OR absent — the gate just moves their time between the named phases and
  `oother`, never off the ledger (`crates/corelink-container/src/origin_timing.rs:275-309`).
- The DO→container hop is not mandatory on every request: a `brew`/`pip` `_public` edge-serve HIT sets
  `doResponse` directly from the Worker-local read and skips `stub.fetch` altogether, but only ever as a
  strictly-faster substitute for an outcome the container would also have served — any miss/fault falls
  through to the unchanged container path, so this hop is optional for speed, never for correctness
  (`worker/src/index.ts:3925-3986`). An edge-served HIT is exempt from the container's $-ceiling gate but
  not from request-count/storage quota, which the Worker has already enforced upstream of this block
  (`worker/src/index.ts:3942-3949`).

# Gotchas
- OCI, the Stripe webhook, and fabric-introspect take dedicated pass-through arms in the Worker that
  skip the PAT gate; they still traverse the same DO→container hop but the container is the sole auth
  authority for them.
- The DO never sets internal-auth on the data-plane forward, so the container's internal-auth-gated
  admin routes are Worker-unreachable by design (operator-only posture).

# Citations
1. `worker/src/index.ts:1-20` — the `Internet → Worker → DO → container` topology header.
2. `worker/src/index.ts:1302-1460` — edge PAT auth (HMAC fast-reject + D1 lookup + expiry).
3. `worker/src/index.ts:1919-1987` — the Worker `fetch` entry + `matchRoute`.
4. `worker/src/index.ts:3844-3846` — `idFromName(resolvedTenantId)` DO derivation (structural isolation).
5. `worker/src/index.ts:3849-3880` — strip-then-set trust headers on the forward.
6. `worker/src/index.ts:3849-3880` — the augmented forward (strip-then-set trust headers); `worker/src/index.ts:3991` — the `stub.fetch` dispatch to the DO (reached only when 6b did not already set `doResponse`).
6b. `worker/src/index.ts:3925-3986` — F3.3 F2 SERVE: the `EDGE_PUBLIC_READ === "serve"` short-circuit on a `brew`/`pip` `_public` HIT — `readPublicHit` at `worker/src/index.ts:3941`, the $-ceiling exemption rationale at `worker/src/index.ts:3942-3949`, the `Content-Type`/`Accept-Ranges` headers at `worker/src/index.ts:3955-3957`, and the `parseByteRange`-driven `200`/`206`/`416` branch at `worker/src/index.ts:3959-3974`; the deliberate omission of `stOrigin*` (so `Server-Timing` proves the bypass by omitting `origin`) at `worker/src/index.ts:3932-3933`.
6c. `worker/src/index.ts:2417` — the `/_internal/public/revoke` invalidation seam (B1b): the revoke forwards to the `_system` DO → container (authoritative D1 blocklist + `cache_map` delete + R2 erase) and, on an `ok` response with `METADATA_KV` bound (`worker/src/index.ts:2433`), reads the RESOLVED `content_hash` from the container's buffered RESPONSE body (`worker/src/index.ts:2443`, F-1: authoritative for both the raw `content_hash` and the `upstream_digest` revoke spaces) and best-effort-writes the content-hash-keyed edge blocklist KV via `ctx.waitUntil(writePublicBlocklistKv(...))` (`worker/src/index.ts:2459`), collapsing the `_public` edge-serve revocation window from the ~60 s map-cache TTL to KV propagation without ever failing the revoke on a KV fault.
6d. `worker/src/index.ts:3248` — `meter = !isFanout && requestQuotaEnabled`, the gate that decides whether the P3 `qdo` hop runs at all; `worker/src/index.ts:3301-3347` — the awaited `serveViaDO(...)` call timed in a `try/finally` (`stQDoMs`, captured even on the fail-open `catch`); `worker/src/index.ts:4152-4180` — the `qdo`/`qother` emission, gated on `SERVER_TIMING_WDB_DETAIL === "on"`.
7. `worker/src/durable_object.ts:345-358` — the DO→container proxy via `getTcpPort(50051)`.
8. `worker/src/durable_object.ts:541-642` — the DO `fetch`: tenant bind, ensure-running, proxy.
9. `worker/src/durable_object.ts:573-602` — the ensure-running gate before proxying (503/500 otherwise).
10. `crates/corelink-container/src/routes.rs:448-450` — the container's composed router receiving the request.
11. `crates/corelink-container/src/routes.rs:550-659` — the shared CAS/AC handlers (accounting + tombstone + PAT gate) executing the op.
12. `crates/corelink-container/src/origin_timing.rs:461-470` — the container's outermost data-plane layer: scope a task-local phase ledger over the request, clock the whole of it, and stamp the phases on the response's `Server-Timing`. Wired last (so it wraps every inner layer) at `crates/corelink-container/src/routes.rs:1134-1136`; the residue that makes the parts sum to the whole is `crates/corelink-container/src/origin_timing.rs:308`; `timed` is the pass-through recorder at `crates/corelink-container/src/origin_timing.rs:374-380`.
13. `crates/corelink-container/src/origin_timing.rs:152-166` — the `Phase::Argon` / `Phase::Permit` / `Phase::Tier` variants W2 split out of `oother`: the PAT Argon2id region (found arm AND row-not-found dummy burn, same name), the `ARGON2_PERMIT_WAIT` semaphore acquires, and `ensure_tier_applied`'s D1 read.
14. `crates/corelink-container/src/origin_timing.rs:395-397` — `current_ledger`: captures a handle to the ambient ledger on the ORIGINATING task, for a region (the Argon2id `FlightGroup`'s spawned lead future) that runs on a different task and cannot see the task-local `timed`/`PhaseScope::enter` rely on.
15. `crates/corelink-container/src/origin_timing.rs:345-348` — `detail_phases_enabled`: reads `CORELINK_ORIGIN_TIMING_DETAIL`, off by default and load-bearing — `opermit` presence is a warm-memo oracle, and the dummy burn's padding is timing that a named split would erode. W3 (this concept) added a fourth gated phase, `oaudit`, to the same flag.
16. `crates/corelink-container/src/storage/d1_audit_sink.rs:165-173` — `D1AuditOutboxSink::write_blocking`: the ONE blocking-D1 choke point every native CAS/AC `AuditSink::emit`/`append` call routes through, timed into `Phase::Audit` (`oaudit`) via `PhaseScope::enter`.
17. `crates/corelink-container/src/storage/r2_s3.rs:1158-1162` — `R2CasHandler::read`'s R2 GET, timed into the EXISTING `Phase::Store` (`ostore`) — the first native-plane R2 call this phase absorbs (see also `crates/corelink-container/src/storage/r2_s3.rs:2215-2219` for the AC counterpart, `R2AcHandler::lookup`).
