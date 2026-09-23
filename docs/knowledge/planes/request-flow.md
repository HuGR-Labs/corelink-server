---


type: "Plane"
title: "Worker → DO → Container request flow"
description: "The end-to-end path a cache request takes: edge auth + routing in the Worker, lifecycle + proxy in the per-tenant Durable Object, handler execution in the Rust container."
source_files:
  - "worker/src/lib/edge_find_missing.ts"
  - "crates/corelink-container/src/routes/audit_cas_attempted.rs"
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/routes/build.rs"
  - "crates/corelink-container/src/origin_timing.rs"
  - "crates/corelink-container/src/storage/d1_audit_sink.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/client_types.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_ops.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/ac_ops.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/ac_list.rs"
  - "worker/src/durable_object_probes.ts"
  - "worker/src/durable_object.ts"
  - "worker/src/index_auth_policy.ts"
  - "worker/src/index_auth_verify.ts"
  - "worker/src/index_auth_pat.ts"
  - "worker/src/index_env.ts"
  - "worker/src/index_edge_stage.ts"
  - "worker/src/index_finish_stage.ts"
  - "worker/src/index_observability.ts"
  - "worker/src/index_public_internal.ts"
  - "worker/src/index_quota_impl.ts"
  - "worker/src/index_routing_stage.ts"
  - "worker/src/index_special_passthrough.ts"
  - "worker/src/route_match.ts"
  - "worker/src/index_fetch.ts"
  - "worker/src/index_auth_stage.ts"
source_blobs:
  - "worker/src/lib/edge_find_missing.ts@17ca8cfe8bfbf233d031cd600ccb7802ab7f28b8"
  - "crates/corelink-container/src/routes/audit_cas_attempted.rs@a1a8353d127c9a46c28a9d417f0d7a4f2e991c45"
  - "crates/corelink-container/src/routes.rs@ddbe70297312a757a9894c71635d9610f881d3b2"
  - "crates/corelink-container/src/storage/d1_audit_sink.rs@8248f5b89b81118171c8d98dfd28eb98dcad95c5"

  - "crates/corelink-container/src/origin_timing.rs@41b25e0b9239234bdff06fbf7391abd826d3faf4"
  - "crates/corelink-container/src/routes/build.rs@43036d91a76bf2e14d6d49c2bca705037fc02081"
  - "crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs@9d0f2ddda14e30d0358283bc9d96577a94d8ad29"
  - "crates/corelink-container/src/storage/r2_s3_parts/ac_list.rs@8444cef5b6a71580ba8329371d8fca7ecbf9e77c"
  - "crates/corelink-container/src/storage/r2_s3_parts/ac_ops.rs@9cc8dd3e5d7c5eedda97f57065060593331ddfa0"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs@9255013bd5f08e8ea4ee2d66f6dffc5aa466852b"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_ops.rs@8762b858933aadb0c6577be5cef37d9826dec42a"
  - "crates/corelink-container/src/storage/r2_s3_parts/client_types.rs@acb304cc10ef4e48fd5da1b779565ca27588c156"
  - "worker/src/durable_object.ts@1ea100789b9514ea1c7caf67212fe3b162bbe3e4"
  - "worker/src/durable_object_probes.ts@10c038f0726316f0cae814aec422eabc708489a5"
  - "worker/src/index_auth_pat.ts@cfa7eeccce38b15f1bafc3658816c0c3f40f6505"
  - "worker/src/index_auth_verify.ts@2fc384e44afb315c32bc63171d5444bf73b6e59a"
  - "worker/src/index_edge_stage.ts@2c3812119bbf05bf0a29c7b573a81a2f6e27b509"
  - "worker/src/index_env.ts@4e2290b692b2acf11c7f33cfa7de617ba7d8c834"
  - "worker/src/index_fetch.ts@bb95a854b3865e62855f4310b3128dc2bb2ed070"
  - "worker/src/index_finish_stage.ts@f53818a791f4ab9d278ecb885fa53a27864b285a"
  - "worker/src/index_observability.ts@9da17700d204099cace321924583d4c059d58373"
  - "worker/src/index_public_internal.ts@645f0bbc073553492122d9b23ed90d2cb57e736b"
  - "worker/src/index_quota_impl.ts@7112a78c574de31aaa26eee34a7b8dbece41b967"
  - "worker/src/index_routing_stage.ts@5cd6c7f272a8a15586734d7a7df0fa6e93066bdd"
  - "worker/src/index_special_passthrough.ts@ad5789c4f5bae6f7ad721fe9e69ce9607057b549"
  - "worker/src/route_match.ts@0e1733c5279fc2862c8f8f4bdadcc04c75b60423"

  - "worker/src/index_auth_policy.ts@545af17a21b5b3fe2fd19e0c2516782ae6758a7d"
  - "worker/src/index_auth_stage.ts@1db504a3961f4fa6e1022bf71926ec451b4028a0"
checkpoint_sha: "91630baebe3ae7abe686cd4e06a5621ecdc4ab73"
provenance: "AUTHORED"
tags: ["planes", "request-flow", "topology", "end-to-end"]
timestamp: "2026-09-06T00:00:00Z"

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
  (`worker/src/index_env.ts:1-20`).
- The single contract that the per-plane concepts plug into — each hop is the input to the next.

# How it works
1. The request enters `baseHandler.fetch`: request-id, CORS, then `matchRoute` selects a `RouteKind` +
   tenant (`worker/src/index_fetch.ts:17-45`).
2. The Worker authenticates the Bearer PAT — HMAC fast-reject then a D1 `token_id` lookup + expiry —
   resolving the trusted tenant (`worker/src/index_auth_verify.ts:13-145`). EXCEPTION (Artifact 1): the unauth
   `/v1/public/*` arm (the erasure-attestation verifier) is matched BEFORE the generic `/v1/*` PAT bucket
   and forwarded as `_anonymous` with NO PAT and NO internal-auth — an erasure proof is publicly
   verifiable, so this route skips the auth hop entirely (matchRoute arm `worker/src/route_match.ts:391-392`,
   forward arm `worker/src/index_special_passthrough.ts:203-231`). The edge expiry check honors the
   `expires_ms === 0` "never expires" sentinel (`row.expires_ms !== 0 && row.expires_ms <= now`),
   matching the container's `adapter_pat` SQL (`expires_ms = 0 OR expires_ms > now`) — so a no-TTL PAT
   is no longer a split-brain edge-reject that worked in the container but died at the Worker. If that
   D1 lookup itself faults (network partition / DB unavailable) the auth hop fails CLOSED but is mapped
   to a retryable 503, not a 401 — the request never reaches the DO, and a transient infra fault is
   never surfaced to the client as "bad credentials" (H1).
3. The Worker derives the per-tenant DO with `idFromName(resolvedTenantId)`, making isolation structural
   (`worker/src/index_routing_stage.ts:198-200`). A non-local-region tenant may first be routed to the LOCAL
   (this-region) `_system` container branch above this forward; the fall-through then lands on the
   per-tenant DO derivation here (`worker/src/index_public_internal.ts:164-166`).
4. It strips any client-supplied trust headers (delete-then-set discipline), sets its own verified
   tenant-id/scope/token-prefix, and (absent an edge-serve short-circuit, next point) dispatches via
   `stub.fetch` (`worker/src/index_routing_stage.ts:220-251`, `worker/src/index_finish_stage.ts:38`).
4b. **F3.3 F2 SERVE short-circuits the DO→container hop entirely on a `brew`/`pip` `_public` cache HIT.**
   With `env.EDGE_PUBLIC_READ === "serve"` and a GET on those two route kinds, the Worker reads the blob
   itself from the native `CONFIG_DB` map + `CAS_BUCKET` R2 (`readPublicHit`) BEFORE ever building the
   `stub.fetch` call — a hit sets `doResponse = edgeServed` and the container is never dispatched, which is
   the F3.3 removal of the `origin` phase (~585 ms) from this path; a miss, revocation, re-hash mismatch, or
   thrown fault yields `null`/falls into the `catch` and the request falls through unchanged to the
   container path below, so this can only make a HIT faster, never change correctness
   (`worker/src/index_edge_stage.ts:22-131`). A served HIT now sets `Content-Type: <PUBLIC_BLOB_CONTENT_TYPE>` and
   `Accept-Ranges: bytes` (faithful to what the container's own binary read returns, previously dropped on
   this path) and honors a client `Range` header — `parseByteRange` yields a `206 Partial Content` slice
   with `Content-Range`, a `416` with `Content-Range: bytes */<total>` on an unsatisfiable range, or the
   full `200` body otherwise (`worker/src/index_edge_stage.ts:33-57`). This edge HIT deliberately does **not**
   pass through the container's per-op $-ceiling gate (ADR-0068) — request-count and storage quota
   (`runQuotaBatch`, already run above this block) still apply, but the spend cap does not, an intentional
   exemption for the cheapest, most-shared traffic class (`worker/src/index_edge_stage.ts:25-32`). The Worker
   deliberately does NOT stamp `stOrigin*` on an edge-served response, so `Server-Timing` simply omits the
   `origin` phase — that absence is itself the wire-level proof the container was bypassed
   (`worker/src/index_routing_stage.ts:303-304`).
4c. **The `_public` edge-serve cache of 4b is invalidated out-of-band by `/_internal/public/revoke` (B1b).**
   That internal route forwards the revoke to the `_system` DO → container — the authoritative D1 blocklist
   + `cache_map` delete + R2 erase (`worker/src/index_public_internal.ts:364`) — and, only on the container's `ok` response
   with `METADATA_KV` bound (`worker/src/index_public_internal.ts:380`), reads the RESOLVED `content_hash` from the
   container's buffered-and-rebuilt RESPONSE body (`worker/src/index_public_internal.ts:390`, F-1: authoritative for BOTH
   revoke spaces, so a revoke-by-`upstream_digest` — whose REQUEST carries no `content_hash` — collapses the
   edge window too) and best-effort-writes a content-hash-keyed edge blocklist KV via
   `ctx.waitUntil(writePublicBlocklistKv(...))` (`worker/src/index_public_internal.ts:406`), so a revoked `_public` hash
   stops edge-serving within KV propagation (~seconds) instead of waiting out the ~60 s map-cache TTL. The
   KV write never fails the revoke the container already applied — a KV fault or a non-`{content_hash}`
   response body just falls back to the map-cache window.
4d. **The `wdb` window also carries an optional P3 EDGE_DO_METER hop — a SEPARATE DO pair from the
   per-tenant `CoreLinkServer` of steps 5-6 — now separately clocked.** When `EDGE_DO_METER === "serve"`
   for this region and the request is genuinely counted (`meter = !isFanout && requestQuotaEnabled`,
   `worker/src/index_quota_impl.ts:66`), the Worker AWAITS `serveViaDO(...)` (the request-meter shard/coordinator
   pair) inside a `try/finally` so the elapsed time (`stQDoMs`) is captured even on the fail-open `catch`
   path (`worker/src/index_quota_impl.ts:92-139`). That duration is reported as the `qdo` Server-Timing phase,
   and `wdb`'s leftover as `qother`, but ONLY when the operator flag `SERVER_TIMING_WDB_DETAIL === "on"`
   (unset by default) — see [the Worker edge plane](/planes/worker-edge.md) for the full `wdb` breakdown
   and why the gate exists (an unflagged `qdo` would leak whether a client-supplied
   `x-corelink-fanout-from` header matched `CORELINK_INTERNAL_AUTH_KEY`).
5. The DO's `fetch` binds the forwarded tenant-id, ensures the container is running, then proxies the
   request (`worker/src/durable_object.ts:177-278`).
6. The proxy rewrites the request onto `http://localhost:50051` through the `getTcpPort` fetcher — the
   DO→container hop (`worker/src/durable_object_probes.ts:226-237`).
7. The container's composed router (built by `build_with_factory`) receives the request and routes it to
   the matching handler (`crates/corelink-container/src/routes.rs:462-464`).
8. The shared CAS/AC handler objects — wrapped once with byte-accounting, the erasure tombstone gate,
   and the native PAT possession backstop — execute the actual cache operation
   (`crates/corelink-container/src/routes/build.rs:75-184`).
9. The hop is MEASURED end to end, and the measurement is split across the same boundary the request
   crosses. The container's outermost data-plane layer clocks its whole share of the request and stamps
   `opat` / `oquota` / `ostore` / `oother` onto the response's own `Server-Timing`
   (`crates/corelink-container/src/origin_timing.rs:728-737`, wired last so it wraps every inner layer at
   `crates/corelink-container/src/routes/build.rs:714-716`); the Worker forwards those four and derives the
   one term only it can see, `ohop = origin − Σ(container phases)` — the dispatch, the DO's prologue and
   the wire (`worker/src/index_observability.ts:247`). Recording is a task-local ledger, so an instrumented region
   reached outside a request (a test, a background task) simply records nothing
   (`crates/corelink-container/src/origin_timing.rs:426-432`).
9b. **W2 split `oother` into three named regions on the PAT-verification path**, `oargon` /
   `opermit` / `ortier` — the container's Argon2id verify (memo check + coalesced flight, and the
   row-not-found dummy burn, under the SAME `oargon` name so the two arms stay indistinguishable),
   the `ARGON2_PERMIT_WAIT`-bounded semaphore acquires (`opermit`), and `ensure_tier_applied`'s D1
   tier-label read (`ortier`) (`crates/corelink-container/src/origin_timing.rs:195-209`). Because the
   Argon2id flight SPAWNS its lead future onto a task with no ambient task-local ledger, `oargon`/
   `opermit` are recorded via a ledger HANDLE captured on the originating task before the spawn
   (`current_ledger` + `PhaseScope::with_handle`), not the ambient-task-local path `timed`/
   `PhaseScope::enter` use everywhere else (`crates/corelink-container/src/origin_timing.rs:461-463`).
   Of these, only `oargon` and `opermit` are gated OFF by default behind
   `CORELINK_ORIGIN_TIMING_DETAIL=on` (`crates/corelink-container/src/origin_timing.rs:483`,
   `crates/corelink-container/src/origin_timing.rs:552-555`): `opermit`'s mere PRESENCE reveals
   whether the Argon2id flight ran at all, i.e. whether a `secret_match_memo` hit was warm for that
   exact credential, and the dummy burn exists precisely so a missing/expired/revoked `token_id` is
   timing-indistinguishable from a valid one — partitioning that residue by name would erode the
   padding it provides. Off is the production default; the ledger still records the gated phases
   unconditionally, so arming the gate needs no rebuild
   (`crates/corelink-container/src/origin_timing.rs:277-285`,
   `crates/corelink-container/src/origin_timing.rs:530-532`).
9c. **`ortier` and `oaudit` are NOT gated — they publish unconditionally.** Both sat behind the same
   flag until B-109, which split it: neither carries the credential oracle the flag exists to withhold,
   and gating them meant the only way to enumerate a 139 ms `oother` in production was to arm that
   oracle for the length of the diagnostic window. The gate now names exactly the two credential-path
   phases (`crates/corelink-container/src/origin_timing.rs:339`). `oaudit` is the blocking D1-over-HTTP write inside
   `D1AuditOutboxSink::write_blocking` — the choke point every SYNC native-plane CAS/AC `AuditSink::emit`
   call routes through before/after a read or mutation — is timed into `Phase::Audit` via a
   `PhaseScope` opened at the top of that method
   (`crates/corelink-container/src/storage/d1_audit_sink.rs:367-375`). Separately, and UNGATED, the same
   native CAS/AC handlers' R2/S3 object GET/PUT/DELETE/LIST calls (`R2CasHandler`/`R2AcHandler`, made
   through the sync `block_in_place` bridge) are now wrapped into the EXISTING `ostore` phase rather than
   falling into `oother`
   (`crates/corelink-container/src/storage/r2_s3_parts/cas_ops.rs:75-117`,
   `crates/corelink-container/src/storage/r2_s3_parts/ac_ops.rs:2-80`). Both additions follow the same
   `PhaseScope::enter` pattern as `oargon`/`opermit`/`ortier` and change no ordering or error handling —
   `PhaseScope`'s `Drop` records on every exit path, so a downstream error still gets timed.
9d. **Native `list()` keeps the audit-before-storage contract.** Both `R2CasHandler::list` and
   `R2AcHandler::list` emit the mandatory `ListAttempted` audit before resolving the tenant prefix
   and dispatching the R2 enumeration. If the audit fails, the handler returns `AuditFailed` and
   does not call storage (`crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs:329-418`,
   `crates/corelink-container/src/storage/r2_s3_parts/ac_list.rs:33-98`). The audit write and R2
   list each use their own timing scope, so `oaudit` and `ostore` measure sequential work rather
   than an overlapping window.
9f. **Under `EDGE_FIND_MISSING="on"` the request may never reach 9e at all.** The Worker probes R2
   through its own in-colo binding and answers there (`worker/src/index_edge_stage.ts:68-129`), which is the
   point: the container's ceiling is ~13 digests/second and is a property of the 0.25-vCPU instance
   and its public-S3 path to R2, not of our fan-out. What makes this legal rather than a hole in the
   audit trail is that the edge does NOT own the rows. It awaits ONE call to
   `POST /_internal/audit/cas-attempted`, which emits them through the SAME `D1AuditOutboxSink` 9e
   uses (`crates/corelink-container/src/routes/audit_cas_attempted.rs:245-257`), and a `204` is the
   ONLY status that authorizes serving (`worker/src/index_edge_stage.ts:108`). Anything else — route unmounted,
   outbox down, timeout, transport error — discards a probe result the Worker is already holding and
   falls through to 9e (`worker/src/lib/edge_find_missing.ts:415`), which then attempts the audit
   itself and fails closed in its own taxonomy. Every other doubt defers the same way: BYOK or
   unknown tenant, non-UUID tenant, over the 256-digest edge cap, unparseable body, probe error
   (`worker/src/lib/edge_find_missing.ts:371-417`). An empty batch IS answered without auditing,
   because 9e writes no row for one either.

9e. **The Bazel `findMissingBlobs` path batches the audit and bounds storage fan-out.** The
   endpoint probes up to `FIND_MISSING_BLOB_CAP` (4096) digests in one request. It writes the
   per-digest `ReadAttempted` rows in one batched audit operation, waits for that audit to commit,
   then runs up to `MAX_CONCURRENT_EXISTS_PROBES` R2 HEADs concurrently
   (`crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs:54-103`, bound at
   `crates/corelink-container/src/storage/r2_s3_parts/client_types.rs:40`). The audit and probe
   phases remain serial: an `AuditFailed` result returns before any storage probe is dispatched
   (`crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs:74-87`). `Phase::Audit` covers
   the batched D1 write, and one `Phase::Store` scope covers the concurrent probe window; individual
   probes open no nested store scope (`crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs:94-103`,
   `:174-237`). A handler without the async audit seam advertises no batch capability and keeps the
   unchanged per-digest `exists()` loop (`crates/corelink-container/src/storage/r2_s3_parts/cas_ops.rs:367-371`).

# Invariants
- Durable tenant-attributed audit writes resolve the row's region from an existing `tenant.primary_region`; migration 0107 rejects a missing tenant instead of allowing an unevaluable residency row, while the explicit `_public` namespace remains pinned to `wnam` (`crates/corelink-container/src/storage/d1_audit_sink.rs:144-175`).
- The DO is always selected from the PAT-resolved tenant, never the URL tenant — isolation is
  established at this hop (`worker/src/index_routing_stage.ts:198-200`).
- The request that crosses Worker→DO carries only Worker-established trust headers; client values are
  stripped first (`worker/src/index_edge_stage.ts:24`).
- The DO→container hop always targets port 50051 via the `getTcpPort` fetcher
  (`worker/src/durable_object_probes.ts:226-237`).
- The container re-verifies possession at the shared handler chokepoint rather than trusting the hop
  blindly (`crates/corelink-container/src/routes/build.rs:75-184`).
- The DO will not proxy until the container is confirmed running (or it returns 503/500)
  (`worker/src/durable_object.ts:327-356`).
- The `origin` split always reconciles: the container's residue phase is computed against its OWN
  whole-request clock, so its parts sum exactly to the time it held the request
  (`crates/corelink-container/src/origin_timing.rs:389`), and the Worker publishes no split it cannot
  make add up (`worker/src/index_observability.ts:247-248`). That reconciliation holds with `oargon`/`opermit`/
  `ortier`/`oaudit` present OR absent — the gate just moves their time between the named phases and
  `oother`, never off the ledger (`crates/corelink-container/src/origin_timing.rs:321-355`). For CAS and
  AC list operations, the mandatory `ListAttempted` audit completes before the R2 list dispatch; the
  separate audit and store scopes therefore do not overlap (`crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs:329-418`, `crates/corelink-container/src/storage/r2_s3_parts/ac_list.rs:33-98`).
- The DO→container hop is not mandatory on every request: a `brew`/`pip` `_public` edge-serve HIT sets
  `doResponse` directly from the Worker-local read and skips `stub.fetch` altogether, but only ever as a
  strictly-faster substitute for an outcome the container would also have served — any miss/fault falls
  through to the unchanged container path, so this hop is optional for speed, never for correctness
  (`worker/src/index_edge_stage.ts:22-131`). An edge-served HIT is exempt from the container's $-ceiling gate but
  not from request-count/storage quota, which the Worker has already enforced upstream of this block
  (`worker/src/index_edge_stage.ts:25-32`).

# Gotchas
- OCI, the Stripe webhook, and fabric-introspect take dedicated pass-through arms in the Worker that
  skip the PAT gate; they still traverse the same DO→container hop but the container is the sole auth
  authority for them.
- The DO never sets internal-auth on the data-plane forward, so the container's internal-auth-gated
  admin routes are Worker-unreachable by design (operator-only posture).

# Citations
3a. `worker/src/route_match.ts:65-315` — the Worker’s ordered route table and path dispatch.
3b. `worker/src/index_special_passthrough.ts:203-234` — unauthenticated public-attestation pass-through.
3c. `worker/src/index_auth_policy.ts:229-319` — client trust-header deny list and strip-then-set helper.
3d. `worker/src/index_auth_pat.ts:204-260` — HMAC-SHA256 PAT validation and key-rotation checks.
3e. `worker/src/index_auth_stage.ts:51-88` — auth failure classification and retryable 503 mapping.
3f. `worker/src/index_observability.ts:213-250` — reconciliation of container timing phases into the Worker’s `origin` split.
3g. `crates/corelink-container/src/routes/audit_cas_attempted.rs:245-257` — internal audit-probe endpoint used by the edge `findMissingBlobs` comparison.
1. `worker/src/index_env.ts:1-20` — the `Internet → Worker → DO → container` topology header.
2. `worker/src/index_auth_verify.ts:13-145` — edge PAT auth (HMAC fast-reject + D1 lookup + expiry).
3. `worker/src/index_fetch.ts:17-45` — the Worker `fetch` entry + `matchRoute`.
4. `worker/src/index_routing_stage.ts:198-200` — `idFromName(resolvedTenantId)` DO derivation (structural isolation).
5. `worker/src/index_routing_stage.ts:220-251` — strip-then-set trust headers on the forward.
6. `worker/src/index_routing_stage.ts:220-251` — the augmented forward (strip-then-set trust headers); `worker/src/index_routing_stage.ts:245` — the `stub.fetch` dispatch to the DO (reached only when 6b did not already set `doResponse`).
6b. `worker/src/index_edge_stage.ts:22-131` — F3.3 F2 SERVE: the `EDGE_PUBLIC_READ === "serve"` short-circuit on a `brew`/`pip` `_public` HIT — `readPublicHit` at `worker/src/index_edge_stage.ts:28-30`, the $-ceiling exemption rationale at `worker/src/index_edge_stage.ts:25-32`, the `Content-Type`/`Accept-Ranges` headers at `worker/src/index_edge_stage.ts:38-40`, and the `parseByteRange`-driven `200`/`206`/`416` branch at `worker/src/index_edge_stage.ts:42-57`; the deliberate omission of `stOrigin*` (so `Server-Timing` proves the bypass by omitting `origin`) at `worker/src/index_routing_stage.ts:303-304`.
6c. `worker/src/index_public_internal.ts:68` — the `/_internal/public/revoke` invalidation seam (B1b): the revoke forwards to the `_system` DO → container (authoritative D1 blocklist + `cache_map` delete + R2 erase) and, on an `ok` response with `METADATA_KV` bound (`worker/src/index_public_internal.ts:84`), reads the RESOLVED `content_hash` from the container's buffered RESPONSE body (`worker/src/index_public_internal.ts:94`, F-1: authoritative for both the raw `content_hash` and the `upstream_digest` revoke spaces) and best-effort-writes the content-hash-keyed edge blocklist KV via `ctx.waitUntil(writePublicBlocklistKv(...))` (`worker/src/index_public_internal.ts:110`), collapsing the `_public` edge-serve revocation window from the ~60 s map-cache TTL to KV propagation without ever failing the revoke on a KV fault.
6d. `worker/src/index_quota_impl.ts:66` — `meter = !isFanout && requestQuotaEnabled`, the gate that decides whether the P3 `qdo` hop runs at all; `worker/src/index_quota_impl.ts:92-139` — the awaited `serveViaDO(...)` call timed in a `try/finally` (`stQDoMs`, captured even on the fail-open `catch`); `worker/src/index_finish_stage.ts:250-278` — the `qdo`/`qother` emission, gated on `SERVER_TIMING_WDB_DETAIL === "on"`.
7. `worker/src/durable_object_probes.ts:226-237` — the DO→container proxy via `getTcpPort(50051)`.
8. `worker/src/durable_object.ts:177-278` — the DO `fetch`: tenant bind, ensure-running, proxy.
9. `worker/src/durable_object.ts:327-356` — the ensure-running gate before proxying (503/500 otherwise).
10. `crates/corelink-container/src/routes.rs:462-464` — the container's composed router receiving the request.
11. `crates/corelink-container/src/routes/build.rs:75-184` — the shared CAS/AC handlers (accounting + tombstone + PAT gate) executing the op.
12. `crates/corelink-container/src/origin_timing.rs:728-737` — the container's outermost data-plane layer: scope a task-local phase ledger over the request, clock the whole of it, and stamp the phases on the response's `Server-Timing`. Wired last (so it wraps every inner layer) at `crates/corelink-container/src/routes/build.rs:714-716`; the residue that makes the parts sum to the whole is `crates/corelink-container/src/origin_timing.rs:389`; `timed` is the pass-through recorder at `crates/corelink-container/src/origin_timing.rs:426-432`.
13. `crates/corelink-container/src/origin_timing.rs:195-209` — the `Phase::Argon` / `Phase::Permit` / `Phase::Tier` variants W2 split out of `oother`: the PAT Argon2id region (found arm AND row-not-found dummy burn, same name), the `ARGON2_PERMIT_WAIT` semaphore acquires, and `ensure_tier_applied`'s D1 read.
14. `crates/corelink-container/src/origin_timing.rs:461-463` — `current_ledger`: captures a handle to the ambient ledger on the ORIGINATING task, for a region (the Argon2id `FlightGroup`'s spawned lead future) that runs on a different task and cannot see the task-local `timed`/`PhaseScope::enter` rely on.
15. `crates/corelink-container/src/origin_timing.rs:552-555` — `detail_phases_enabled`: reads `CORELINK_ORIGIN_TIMING_DETAIL`, off by default and load-bearing — `opermit` presence is a warm-memo oracle, and the dummy burn's padding is timing that a named split would erode. B-109 narrowed the gate to exactly that credential-path pair; the arm that skips them is `crates/corelink-container/src/origin_timing.rs:339`, and `ortier`/`oaudit` fall through it and publish always.
16. `crates/corelink-container/src/storage/d1_audit_sink.rs:367-375` — `D1AuditOutboxSink::write_blocking`: the choke point every SYNC native CAS/AC `AuditSink::emit`/`append` call routes through, timed into `Phase::Audit` (`oaudit`) via `PhaseScope::enter`.
17. `crates/corelink-container/src/storage/r2_s3_parts/cas_ops.rs:75-117` — `R2CasHandler::read`'s R2 GET, timed into the EXISTING `Phase::Store` (`ostore`) — the first native-plane R2 call this phase absorbs (see also `crates/corelink-container/src/storage/r2_s3_parts/ac_ops.rs:2-80` for the AC counterpart, `R2AcHandler::lookup`).
18. `crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs:329-418` — `R2CasHandler::list`: emits the mandatory `ListAttempted` audit before the R2 enumeration and returns `AuditFailed` without dispatching storage when the audit fails.
19. `crates/corelink-container/src/storage/r2_s3_parts/ac_list.rs:33-98` — `R2AcHandler::list`: likewise commits the mandatory `ListAttempted` audit before the R2 enumeration; audit failure returns before storage dispatch.
20. `crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs:337-352` and `:415-418`, plus `crates/corelink-container/src/storage/r2_s3_parts/ac_list.rs:33-46` — both list handlers stop before storage on audit failure; CAS and AC use separate audit and store timing scopes.
- `worker/src/lib/edge_find_missing.ts:371-417` — the edge-serve decision: probe in-colo, await the container's audit emit, and return null (fall through) for every doubt including an audit that did not commit.
21. `crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs:74-103` — `R2CasHandler::exists_batch_inner`: the Bazel `findMissingBlobs` seam. It awaits the batched audit before creating or dispatching storage probes, then scopes the bounded concurrent R2 HEADs as `Phase::Store`.
22. `crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs:174-237` — `probe_existence_unaudited`, the storage half of ONE probe: no `PhaseScope` of its own (N concurrent probes each entering `Phase::Store` would bill the same wall-clock window N times over). Shared with the single-digest `exists()` seam, so both drive one body.
23. `crates/corelink-container/src/storage/r2_s3_parts/client_types.rs:40` — `MAX_CONCURRENT_EXISTS_PROBES`: the bound on 21's in-flight HEADs, kept small because the container is a 0.25-vCPU `basic` instance and R2 request limits are shared across tenants.
24. `crates/corelink-container/src/storage/d1_audit_sink.rs:324-347` — `append_batch_async` and `build_batch_statements`: the batched writer builds JSON1 statements and awaits the D1 writes; the caller owns the audit-phase scope.
