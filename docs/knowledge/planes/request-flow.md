---
type: "Plane"
title: "Worker → DO → Container request flow"
description: "The end-to-end path a cache request takes: edge auth + routing in the Worker, lifecycle + proxy in the per-tenant Durable Object, handler execution in the Rust container."
source_files:
  - "worker/src/index.ts"
  - "worker/src/lib/edge_find_missing.ts"
  - "crates/corelink-container/src/routes/audit_cas_attempted.rs"
  - "worker/src/durable_object.ts"
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/origin_timing.rs"
  - "crates/corelink-container/src/storage/d1_audit_sink.rs"
  - "crates/corelink-container/src/storage/r2_s3.rs"
source_blobs:
  - "worker/src/index.ts@d4f2605e0e46840b641a56e444007304d7f4f2ec"
  - "worker/src/durable_object.ts@bf36fd6a2f94c92c8b5d47872dedb93cfc95d137"
  - "crates/corelink-container/src/routes.rs@90c8f13a942bca512dec0047e7f0bf0b2356c229"
  - "crates/corelink-container/src/origin_timing.rs@1c61b0059b68f2ee697d3b69a111246faea4fa7c"
  - "crates/corelink-container/src/storage/d1_audit_sink.rs@e7469a400d53d76a210a6e7c27bf5c4b5aa87c37"
  - "crates/corelink-container/src/storage/r2_s3.rs@6882c633b816e969236c16a64f568bcdd5b4f8d0"
checkpoint_sha: "07bb61ee80779947e6bff8cdc01f895414088fc9"
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
   tenant (`worker/src/index.ts:1963-2031`).
2. The Worker authenticates the Bearer PAT — HMAC fast-reject then a D1 `token_id` lookup + expiry —
   resolving the trusted tenant (`worker/src/index.ts:1346-1504`). EXCEPTION (Artifact 1): the unauth
   `/v1/public/*` arm (the erasure-attestation verifier) is matched BEFORE the generic `/v1/*` PAT bucket
   and forwarded as `_anonymous` with NO PAT and NO internal-auth — an erasure proof is publicly
   verifiable, so this route skips the auth hop entirely (matchRoute arm `worker/src/index.ts:1160-1161`,
   forward arm `worker/src/index.ts:2833-2861`). The edge expiry check honors the
   `expires_ms === 0` "never expires" sentinel (`row.expires_ms !== 0 && row.expires_ms <= now`),
   matching the container's `adapter_pat` SQL (`expires_ms = 0 OR expires_ms > now`) — so a no-TTL PAT
   is no longer a split-brain edge-reject that worked in the container but died at the Worker. If that
   D1 lookup itself faults (network partition / DB unavailable) the auth hop fails CLOSED but is mapped
   to a retryable 503, not a 401 — the request never reaches the DO, and a transient infra fault is
   never surfaced to the client as "bad credentials" (H1).
3. The Worker derives the per-tenant DO with `idFromName(resolvedTenantId)`, making isolation structural
   (`worker/src/index.ts:3986-3988`). A non-local-region tenant may first be routed to the LOCAL
   (this-region) `_system` container branch above this forward; the fall-through then lands on the
   per-tenant DO derivation here (`worker/src/index.ts:2278-2280`).
4. It strips any client-supplied trust headers (delete-then-set discipline), sets its own verified
   tenant-id/scope/token-prefix, and (absent an edge-serve short-circuit, next point) dispatches via
   `stub.fetch` (`worker/src/index.ts:4008-4039`, `worker/src/index.ts:4212`).
4b. **F3.3 F2 SERVE short-circuits the DO→container hop entirely on a `brew`/`pip` `_public` cache HIT.**
   With `env.EDGE_PUBLIC_READ === "serve"` and a GET on those two route kinds, the Worker reads the blob
   itself from the native `CONFIG_DB` map + `CAS_BUCKET` R2 (`readPublicHit`) BEFORE ever building the
   `stub.fetch` call — a hit sets `doResponse = edgeServed` and the container is never dispatched, which is
   the F3.3 removal of the `origin` phase (~585 ms) from this path; a miss, revocation, re-hash mismatch, or
   thrown fault yields `null`/falls into the `catch` and the request falls through unchanged to the
   container path below, so this can only make a HIT faster, never change correctness
   (`worker/src/index.ts:4084-4207`). A served HIT now sets `Content-Type: <PUBLIC_BLOB_CONTENT_TYPE>` and
   `Accept-Ranges: bytes` (faithful to what the container's own binary read returns, previously dropped on
   this path) and honors a client `Range` header — `parseByteRange` yields a `206 Partial Content` slice
   with `Content-Range`, a `416` with `Content-Range: bytes */<total>` on an unsatisfiable range, or the
   full `200` body otherwise (`worker/src/index.ts:4109-4133`). This edge HIT deliberately does **not**
   pass through the container's per-op $-ceiling gate (ADR-0068) — request-count and storage quota
   (`runQuotaBatch`, already run above this block) still apply, but the spend cap does not, an intentional
   exemption for the cheapest, most-shared traffic class (`worker/src/index.ts:4101-4108`). The Worker
   deliberately does NOT stamp `stOrigin*` on an edge-served response, so `Server-Timing` simply omits the
   `origin` phase — that absence is itself the wire-level proof the container was bypassed
   (`worker/src/index.ts:4091-4092`).
4c. **The `_public` edge-serve cache of 4b is invalidated out-of-band by `/_internal/public/revoke` (B1b).**
   That internal route forwards the revoke to the `_system` DO → container — the authoritative D1 blocklist
   + `cache_map` delete + R2 erase (`worker/src/index.ts:2478`) — and, only on the container's `ok` response
   with `METADATA_KV` bound (`worker/src/index.ts:2494`), reads the RESOLVED `content_hash` from the
   container's buffered-and-rebuilt RESPONSE body (`worker/src/index.ts:2504`, F-1: authoritative for BOTH
   revoke spaces, so a revoke-by-`upstream_digest` — whose REQUEST carries no `content_hash` — collapses the
   edge window too) and best-effort-writes a content-hash-keyed edge blocklist KV via
   `ctx.waitUntil(writePublicBlocklistKv(...))` (`worker/src/index.ts:2520`), so a revoked `_public` hash
   stops edge-serving within KV propagation (~seconds) instead of waiting out the ~60 s map-cache TTL. The
   KV write never fails the revoke the container already applied — a KV fault or a non-`{content_hash}`
   response body just falls back to the map-cache window.
4d. **The `wdb` window also carries an optional P3 EDGE_DO_METER hop — a SEPARATE DO pair from the
   per-tenant `CoreLinkServer` of steps 5-6 — now separately clocked.** When `EDGE_DO_METER === "serve"`
   for this region and the request is genuinely counted (`meter = !isFanout && requestQuotaEnabled`,
   `worker/src/index.ts:3390`), the Worker AWAITS `serveViaDO(...)` (the request-meter shard/coordinator
   pair) inside a `try/finally` so the elapsed time (`stQDoMs`) is captured even on the fail-open `catch`
   path (`worker/src/index.ts:3443-3489`). That duration is reported as the `qdo` Server-Timing phase,
   and `wdb`'s leftover as `qother`, but ONLY when the operator flag `SERVER_TIMING_WDB_DETAIL === "on"`
   (unset by default) — see [the Worker edge plane](/planes/worker-edge.md) for the full `wdb` breakdown
   and why the gate exists (an unflagged `qdo` would leak whether a client-supplied
   `x-corelink-fanout-from` header matched `CORELINK_INTERNAL_AUTH_KEY`).
5. The DO's `fetch` binds the forwarded tenant-id, ensures the container is running, then proxies the
   request (`worker/src/durable_object.ts:541-642`).
6. The proxy rewrites the request onto `http://localhost:50051` through the `getTcpPort` fetcher — the
   DO→container hop (`worker/src/durable_object.ts:345-358`).
7. The container's composed router (built by `build_with_factory`) receives the request and routes it to
   the matching handler (`crates/corelink-container/src/routes.rs:462-464`).
8. The shared CAS/AC handler objects — wrapped once with byte-accounting, the erasure tombstone gate,
   and the native PAT possession backstop — execute the actual cache operation
   (`crates/corelink-container/src/routes.rs:564-673`).
9. The hop is MEASURED end to end, and the measurement is split across the same boundary the request
   crosses. The container's outermost data-plane layer clocks its whole share of the request and stamps
   `opat` / `oquota` / `ostore` / `oother` onto the response's own `Server-Timing`
   (`crates/corelink-container/src/origin_timing.rs:533-542`, wired last so it wraps every inner layer at
   `crates/corelink-container/src/routes.rs:1148-1150`); the Worker forwards those four and derives the
   one term only it can see, `ohop = origin − Σ(container phases)` — the dispatch, the DO's prologue and
   the wire (`worker/src/index.ts:4753`). Recording is a task-local ledger, so an instrumented region
   reached outside a request (a test, a background task) simply records nothing
   (`crates/corelink-container/src/origin_timing.rs:446-452`).
9b. **W2 split `oother` into three named regions on the PAT-verification path**, `oargon` /
   `opermit` / `ortier` — the container's Argon2id verify (memo check + coalesced flight, and the
   row-not-found dummy burn, under the SAME `oargon` name so the two arms stay indistinguishable),
   the `ARGON2_PERMIT_WAIT`-bounded semaphore acquires (`opermit`), and `ensure_tier_applied`'s D1
   tier-label read (`ortier`) (`crates/corelink-container/src/origin_timing.rs:218-232`). Because the
   Argon2id flight SPAWNS its lead future onto a task with no ambient task-local ledger, `oargon`/
   `opermit` are recorded via a ledger HANDLE captured on the originating task before the spawn
   (`current_ledger` + `PhaseScope::with_handle`), not the ambient-task-local path `timed`/
   `PhaseScope::enter` use everywhere else (`crates/corelink-container/src/origin_timing.rs:467-469`).
   Of these, only `oargon` and `opermit` are gated OFF by default behind
   `CORELINK_ORIGIN_TIMING_DETAIL=on` (`crates/corelink-container/src/origin_timing.rs:360`,
   `crates/corelink-container/src/origin_timing.rs:417-420`): `opermit`'s mere PRESENCE reveals
   whether the Argon2id flight ran at all, i.e. whether a `secret_match_memo` hit was warm for that
   exact credential, and the dummy burn exists precisely so a missing/expired/revoked `token_id` is
   timing-indistinguishable from a valid one — partitioning that residue by name would erode the
   padding it provides. Off is the production default; the ledger still records the gated phases
   unconditionally, so arming the gate needs no rebuild
   (`crates/corelink-container/src/origin_timing.rs:302-310`,
   `crates/corelink-container/src/origin_timing.rs:417-420`).
9c. **`ortier` and `oaudit` are NOT gated — they publish unconditionally.** Both sat behind the same
   flag until B-109, which split it: neither carries the credential oracle the flag exists to withhold,
   and gating them meant the only way to enumerate a 139 ms `oother` in production was to arm that
   oracle for the length of the diagnostic window. The gate now names exactly the two credential-path
   phases (`crates/corelink-container/src/origin_timing.rs:360`). `oaudit` is the blocking D1-over-HTTP write inside
   `D1AuditOutboxSink::write_blocking` — the choke point every SYNC native-plane CAS/AC `AuditSink::emit`
   call routes through before/after a read or mutation — is timed into `Phase::Audit` via a
   `PhaseScope` opened at the top of that method
   (`crates/corelink-container/src/storage/d1_audit_sink.rs:408-416`). Separately, and UNGATED, the same
   native CAS/AC handlers' R2/S3 object GET/PUT/DELETE/LIST calls (`R2CasHandler`/`R2AcHandler`, made
   through the sync `block_in_place` bridge) are now wrapped into the EXISTING `ostore` phase rather than
   falling into `oother`
   (`crates/corelink-container/src/storage/r2_s3.rs:1333-1337`,
   `crates/corelink-container/src/storage/r2_s3.rs:2787-2791`). Both additions follow the same
   `PhaseScope::enter` pattern as `oargon`/`opermit`/`ortier` and change no ordering or error handling —
   `PhaseScope`'s `Drop` records on every exit path, so a downstream error still gets timed.
9d. **The native `list()` path takes this one step further: `oaudit` and `ostore` now genuinely
   OVERLAP in wall-clock time**, not just in name. `R2CasHandler::list` / `R2AcHandler::list` run the
   mandatory `ListAttempted` audit write CONCURRENTLY with the R2 `ListObjectsV2` enumeration —
   `tokio::join!`ed under one `block_in_place`/`block_on` — when the handler was built with the async
   audit seam wired (production; a handler without it, e.g. every test handler, keeps the fully serial
   path) (`crates/corelink-container/src/storage/r2_s3.rs:2294-2408`). Naively timing both sides with
   their own `PhaseScope` would double-count that overlapping window and break the `Σ(phases) ≤ total`
   partition the reconciliation below depends on, so the join is timed ONCE, under `Phase::Store`
   only — `append_async` (the audit half) never opens a `PhaseScope` of its own
   (`crates/corelink-container/src/storage/d1_audit_sink.rs:324-334`). `oaudit` therefore does not
   report this specific write; it is unaffected everywhere else (every write/update/delete mutation
   path stays fully serial). Fail-CLOSED is unweakened: the audit result is checked, and can short-circuit
   to `AuditFailed`, BEFORE the store result is ever inspected — no rows are served on a failed audit
   write, concurrency only changes whether the R2 call was already dispatched, never whether its result
   can reach the caller (`crates/corelink-container/src/storage/r2_s3.rs:2410-2413`).
9f. **Under `EDGE_FIND_MISSING="on"` the request may never reach 9e at all.** The Worker probes R2
   through its own in-colo binding and answers there (`worker/src/index.ts:4144-4205`), which is the
   point: the container's ceiling is ~13 digests/second and is a property of the 0.25-vCPU instance
   and its public-S3 path to R2, not of our fan-out. What makes this legal rather than a hole in the
   audit trail is that the edge does NOT own the rows. It awaits ONE call to
   `POST /_internal/audit/cas-attempted`, which emits them through the SAME `D1AuditOutboxSink` 9e
   uses (`crates/corelink-container/src/routes/audit_cas_attempted.rs:245-257`), and a `204` is the
   ONLY status that authorizes serving (`worker/src/index.ts:4184`). Anything else — route unmounted,
   outbox down, timeout, transport error — discards a probe result the Worker is already holding and
   falls through to 9e (`worker/src/lib/edge_find_missing.ts:415`), which then attempts the audit
   itself and fails closed in its own taxonomy. Every other doubt defers the same way: BYOK or
   unknown tenant, non-UUID tenant, over the 256-digest edge cap, unparseable body, probe error
   (`worker/src/lib/edge_find_missing.ts:371-417`). An empty batch IS answered without auditing,
   because 9e writes no row for one either.

9e. **The Bazel `findMissingBlobs` path applies 9d's rule twice over.** That endpoint probes up to
   `FIND_MISSING_BLOB_CAP` (4096) digests in ONE request, and was strictly serial in BOTH halves — one
   blocking D1 audit write plus one R2 `HeadObject` per digest, measured in prod at ~268 ms per digest
   (a 40-digest call decomposed to `ostore` 2420 ms + `oother` 4881 ms). `R2CasHandler::exists_batch`
   now joins ONE batched `ReadAttempted` write with up to `MAX_CONCURRENT_EXISTS_PROBES` in-flight
   HEADs (`crates/corelink-container/src/storage/r2_s3.rs:1715-1825`, bound at `crates/corelink-container/src/storage/r2_s3.rs:661`). Attribution follows 9d exactly, at TWO levels: one
   `Phase::Store` scope covers the whole joined window and `Phase::Audit` is never entered for it
   (`append_batch_async`, like `append_async`, opens no scope — `crates/corelink-container/src/storage/d1_audit_sink.rs:367-387`), AND the per-probe
   helper opens no scope either (`crates/corelink-container/src/storage/r2_s3.rs:1874-1892`) — N overlapping probes each entering `Phase::Store`
   would bill the same window N times over and could make `ostore` alone exceed `total_ms`.
   Fail-CLOSED is likewise unchanged: the audit result is evaluated FIRST and can short-circuit to
   `AuditFailed` before any probe result is read (`crates/corelink-container/src/storage/r2_s3.rs:1804-1806`). The rule to carry into any future
   concurrent seam: the joined window gets exactly ONE `PhaseScope`, opened by whoever owns the join —
   never one per concurrent branch. A handler without the async audit seam (every test handler)
   advertises no batch capability at all and keeps the unchanged per-digest `exists()` loop
   (`crates/corelink-container/src/storage/r2_s3.rs:1704-1709`).

# Invariants
- The DO is always selected from the PAT-resolved tenant, never the URL tenant — isolation is
  established at this hop (`worker/src/index.ts:3986-3988`).
- The request that crosses Worker→DO carries only Worker-established trust headers; client values are
  stripped first (`worker/src/index.ts:4008-4039`).
- The DO→container hop always targets port 50051 via the `getTcpPort` fetcher
  (`worker/src/durable_object.ts:345-358`).
- The container re-verifies possession at the shared handler chokepoint rather than trusting the hop
  blindly (`crates/corelink-container/src/routes.rs:564-673`).
- The DO will not proxy until the container is confirmed running (or it returns 503/500)
  (`worker/src/durable_object.ts:573-602`).
- The `origin` split always reconciles: the container's residue phase is computed against its OWN
  whole-request clock, so its parts sum exactly to the time it held the request
  (`crates/corelink-container/src/origin_timing.rs:372`), and the Worker publishes no split it cannot
  make add up (`worker/src/index.ts:4753-4754`). That reconciliation holds with `oargon`/`opermit`/
  `ortier`/`oaudit` present OR absent — the gate just moves their time between the named phases and
  `oother`, never off the ledger (`crates/corelink-container/src/origin_timing.rs:341-375`). The ONE
  exception is the native `list()` concurrent seam (citation 18): there, `oaudit` and `ostore` overlap
  in wall time by construction, so the joined window is charged to `Phase::Store` exactly once rather
  than split — the four-way sum still holds, by not double-counting, not by the residue's `max(0)`
  guard papering over an overcount (citation 19).
- The DO→container hop is not mandatory on every request: a `brew`/`pip` `_public` edge-serve HIT sets
  `doResponse` directly from the Worker-local read and skips `stub.fetch` altogether, but only ever as a
  strictly-faster substitute for an outcome the container would also have served — any miss/fault falls
  through to the unchanged container path, so this hop is optional for speed, never for correctness
  (`worker/src/index.ts:4084-4207`). An edge-served HIT is exempt from the container's $-ceiling gate but
  not from request-count/storage quota, which the Worker has already enforced upstream of this block
  (`worker/src/index.ts:4101-4108`).

# Gotchas
- OCI, the Stripe webhook, and fabric-introspect take dedicated pass-through arms in the Worker that
  skip the PAT gate; they still traverse the same DO→container hop but the container is the sole auth
  authority for them.
- The DO never sets internal-auth on the data-plane forward, so the container's internal-auth-gated
  admin routes are Worker-unreachable by design (operator-only posture).

# Citations
1. `worker/src/index.ts:1-20` — the `Internet → Worker → DO → container` topology header.
2. `worker/src/index.ts:1346-1504` — edge PAT auth (HMAC fast-reject + D1 lookup + expiry).
3. `worker/src/index.ts:1963-2031` — the Worker `fetch` entry + `matchRoute`.
4. `worker/src/index.ts:3986-3988` — `idFromName(resolvedTenantId)` DO derivation (structural isolation).
5. `worker/src/index.ts:4008-4039` — strip-then-set trust headers on the forward.
6. `worker/src/index.ts:4008-4039` — the augmented forward (strip-then-set trust headers); `worker/src/index.ts:4212` — the `stub.fetch` dispatch to the DO (reached only when 6b did not already set `doResponse`).
6b. `worker/src/index.ts:4084-4207` — F3.3 F2 SERVE: the `EDGE_PUBLIC_READ === "serve"` short-circuit on a `brew`/`pip` `_public` HIT — `readPublicHit` at `worker/src/index.ts:4100`, the $-ceiling exemption rationale at `worker/src/index.ts:4101-4108`, the `Content-Type`/`Accept-Ranges` headers at `worker/src/index.ts:4114-4116`, and the `parseByteRange`-driven `200`/`206`/`416` branch at `worker/src/index.ts:4118-4133`; the deliberate omission of `stOrigin*` (so `Server-Timing` proves the bypass by omitting `origin`) at `worker/src/index.ts:4091-4092`.
6c. `worker/src/index.ts:2478` — the `/_internal/public/revoke` invalidation seam (B1b): the revoke forwards to the `_system` DO → container (authoritative D1 blocklist + `cache_map` delete + R2 erase) and, on an `ok` response with `METADATA_KV` bound (`worker/src/index.ts:2494`), reads the RESOLVED `content_hash` from the container's buffered RESPONSE body (`worker/src/index.ts:2504`, F-1: authoritative for both the raw `content_hash` and the `upstream_digest` revoke spaces) and best-effort-writes the content-hash-keyed edge blocklist KV via `ctx.waitUntil(writePublicBlocklistKv(...))` (`worker/src/index.ts:2520`), collapsing the `_public` edge-serve revocation window from the ~60 s map-cache TTL to KV propagation without ever failing the revoke on a KV fault.
6d. `worker/src/index.ts:3390` — `meter = !isFanout && requestQuotaEnabled`, the gate that decides whether the P3 `qdo` hop runs at all; `worker/src/index.ts:3443-3489` — the awaited `serveViaDO(...)` call timed in a `try/finally` (`stQDoMs`, captured even on the fail-open `catch`); `worker/src/index.ts:4400-4428` — the `qdo`/`qother` emission, gated on `SERVER_TIMING_WDB_DETAIL === "on"`.
7. `worker/src/durable_object.ts:345-358` — the DO→container proxy via `getTcpPort(50051)`.
8. `worker/src/durable_object.ts:541-642` — the DO `fetch`: tenant bind, ensure-running, proxy.
9. `worker/src/durable_object.ts:573-602` — the ensure-running gate before proxying (503/500 otherwise).
10. `crates/corelink-container/src/routes.rs:462-464` — the container's composed router receiving the request.
11. `crates/corelink-container/src/routes.rs:564-673` — the shared CAS/AC handlers (accounting + tombstone + PAT gate) executing the op.
12. `crates/corelink-container/src/origin_timing.rs:533-542` — the container's outermost data-plane layer: scope a task-local phase ledger over the request, clock the whole of it, and stamp the phases on the response's `Server-Timing`. Wired last (so it wraps every inner layer) at `crates/corelink-container/src/routes.rs:1148-1150`; the residue that makes the parts sum to the whole is `crates/corelink-container/src/origin_timing.rs:372`; `timed` is the pass-through recorder at `crates/corelink-container/src/origin_timing.rs:446-452`.
13. `crates/corelink-container/src/origin_timing.rs:218-232` — the `Phase::Argon` / `Phase::Permit` / `Phase::Tier` variants W2 split out of `oother`: the PAT Argon2id region (found arm AND row-not-found dummy burn, same name), the `ARGON2_PERMIT_WAIT` semaphore acquires, and `ensure_tier_applied`'s D1 read.
14. `crates/corelink-container/src/origin_timing.rs:467-469` — `current_ledger`: captures a handle to the ambient ledger on the ORIGINATING task, for a region (the Argon2id `FlightGroup`'s spawned lead future) that runs on a different task and cannot see the task-local `timed`/`PhaseScope::enter` rely on.
15. `crates/corelink-container/src/origin_timing.rs:417-420` — `detail_phases_enabled`: reads `CORELINK_ORIGIN_TIMING_DETAIL`, off by default and load-bearing — `opermit` presence is a warm-memo oracle, and the dummy burn's padding is timing that a named split would erode. B-109 narrowed the gate to exactly that credential-path pair; the arm that skips them is `crates/corelink-container/src/origin_timing.rs:360`, and `ortier`/`oaudit` fall through it and publish always.
16. `crates/corelink-container/src/storage/d1_audit_sink.rs:408-416` — `D1AuditOutboxSink::write_blocking`: the choke point every SYNC native CAS/AC `AuditSink::emit`/`append` call routes through, timed into `Phase::Audit` (`oaudit`) via `PhaseScope::enter`.
17. `crates/corelink-container/src/storage/r2_s3.rs:1333-1337` — `R2CasHandler::read`'s R2 GET, timed into the EXISTING `Phase::Store` (`ostore`) — the first native-plane R2 call this phase absorbs (see also `crates/corelink-container/src/storage/r2_s3.rs:2787-2791` for the AC counterpart, `R2AcHandler::lookup`).
18. `crates/corelink-container/src/storage/r2_s3.rs:2294-2408` — `R2CasHandler::list`'s concurrent seam (W4, this reconcile): when built with the async audit seam wired, the mandatory `ListAttempted` audit write and the R2 `ListObjectsV2` call run under one `tokio::join!` instead of two serial round trips; without that seam (every test handler) the original fully serial code path runs unchanged. `R2AcHandler::list` mirrors it exactly.
19. `crates/corelink-container/src/storage/d1_audit_sink.rs:324-334` — `append_async`: the audit half of the join in 18, deliberately WITHOUT its own `PhaseScope` — the caller (18) attributes the whole overlapping window to `Phase::Store` exactly once, so `Σ(phases) ≤ total` still holds when `oaudit` and `ostore` would otherwise have double-counted the same wall-clock window.
20. `crates/corelink-container/src/storage/r2_s3.rs:2410-2413` — the fail-CLOSED check in 18: the audit result is inspected, and can short-circuit to `AuditFailed`, BEFORE the store result — concurrency changes when the R2 call was dispatched, never whether a failed audit can still let its result reach the caller.
- `worker/src/lib/edge_find_missing.ts:371-417` — the edge-serve decision: probe in-colo, await the container's audit emit, and return null (fall through) for every doubt including an audit that did not commit.
21. `crates/corelink-container/src/storage/r2_s3.rs:1715-1825` — `R2CasHandler::exists_batch_inner`: the Bazel `findMissingBlobs` seam. Same join shape as 18 — one batched audit write plus the concurrent R2 HEADs under one `block_in_place`/`block_on`, one `Phase::Store` scope over the whole window, audit result evaluated first (`crates/corelink-container/src/storage/r2_s3.rs:1804-1806`).
22. `crates/corelink-container/src/storage/r2_s3.rs:1874-1892` — `probe_existence_unaudited`, the storage half of ONE probe: no `PhaseScope` of its own (N concurrent probes each entering `Phase::Store` would bill the same wall-clock window N times over). Shared with the single-digest `exists()` seam, so both drive one body.
23. `crates/corelink-container/src/storage/r2_s3.rs:661` — `MAX_CONCURRENT_EXISTS_PROBES`: the bound on 21's in-flight HEADs, kept small because the container is a 0.25-vCPU `basic` instance and R2 request limits are shared across tenants.
24. `crates/corelink-container/src/storage/d1_audit_sink.rs:367-387` — `append_batch_async`: the audit half of 21 — N rows in one JSON1 statement and, like 19, deliberately without its own `PhaseScope`.
