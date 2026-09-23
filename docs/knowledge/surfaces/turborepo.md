---
type: "CacheSurface"
title: "Turborepo v8 remote-cache surface"
description: "The Vercel Turborepo /v8/artifacts remote-cache protocol wired onto CoreLink, with teamId-as-sub-namespace isolation and per-route body caps."
source_files:
  - "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs"
  - "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01_part2.rs"
  - "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs"
  - "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02_part2.rs"
  - "crates/corelink-turbo-bridge/src/adapter.rs"
  - "crates/corelink-turbo-bridge/src/error.rs"
  - "crates/corelink-container/src/routes/turbo_v8.rs"
source_blobs:
  - "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02_part2.rs@0a6ecdbe44d6381e2cc498e67c899c2c9235bb30"
  - "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01_part2.rs@9c83f47b220e2f16bb92c7882eabedd0962e60d0"
  - "crates/corelink-container/src/routes/turbo_v8.rs@7c0614eacdde15e70ba1127637ec526e2ad65e2e"
  - "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs@21080a6bf6d64b0549e44a0a1567ad21d8785a61"
  - "crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs@0de1e781e6d054909151e80315097595535dd03f"
  - "crates/corelink-turbo-bridge/src/adapter.rs@bee7a6c1be4d4ee7a1585627e1452574b8fbd907"
  - "crates/corelink-turbo-bridge/src/error.rs@8bed7e6406ee9e40dffb6a937c930367f1ac732d"
checkpoint_sha: "648ecdccd229bdb5154b86843053c28b9cce9d36"
provenance: "AUTHORED"
tags: ["surfaces", "turborepo", "vercel", "cache"]
timestamp: "2026-06-26T00:00:00Z"

---
# Turborepo v8 remote-cache surface

Turborepo (by Vercel) can use any HTTP server implementing the Vercel Remote Cache `/v8/artifacts`
API. This surface makes CoreLink that server, so a team sets `TURBO_API` + `TURBO_TOKEN=<CoreLink PAT>`
and replaces Vercel's paid remote cache with their own CoreLink-backed store. It lives in the container
plane and is the artifact-cache analogue of the other surfaces, but with two protocol-specific twists:
a `teamId` label that is a logical sub-namespace (NOT a security boundary), and large build artifacts
that justify a per-route body cap well above the global default. The authenticated tenant remains the
only isolation key.

# Role
It serves `GET`/`PUT /v8/artifacts/:hash` (artifact read/write), `POST /v8/artifacts/events`
(accept-and-drop telemetry) and `POST /v8/artifacts/status`. Storage is keyed `tenant = auth.0`,
`key = "<teamId>/<hash>"`, so multiple Turborepo teams under one tenant stay partitioned while
cross-tenant access is impossible. When a client enables artifact signing, the `x-artifact-tag`
signature rides alongside the artifact and is handed back on read.

# How it works
1. The router registers the static `events`/`status` routes BEFORE the `:hash` artifact routes so
   matchit prefers the literals over the capture (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:166-195`).
2. `handle_get` serves an artifact read and requires a cache-read scope (`cas:rw` or `cas:r`) before
   any storage (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:209`; `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:233`).
3. The `events` route is accept-and-drop telemetry carrying its own innermost 64 KiB body limit so it
   does NOT inherit the 100 MiB artifact cap (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:180-185`; `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs:74`).
4. The artifact routes get a per-route 100 MiB `DefaultBodyLimit` overriding the 10 MiB global default
   while still bounding the body (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:187-195`; `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs:37`).
5. The isolation tenant is the PAT-resolved authenticated tenant; `teamId` is demoted to a sub-namespace
   within it (`crates/corelink-container/src/routes/turbo_v8.rs:28-37`).
6. Turbo GET is concurrency-bounded mirroring PUT: `handle_get` declares two `FromRequestParts` guards
   AHEAD of buffering — a per-tenant `GetConcurrencyGuard` (cap `TURBO_GET_CONCURRENCY_LIMIT` = 4, 429 on
   over-cap) and a process-wide `GlobalGetBudgetGuard` (cap derived from the deployed basic capacity, 503 on
   saturation) — so an over-cap read is rejected BEFORE the up-to-100 MiB artifact is read into the heap,
   bounding per-tenant and aggregate read-path memory on a pool SEPARATE from writes
   (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:222-229`; `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01_part2.rs:88-190`; `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01_part2.rs:190-286`).
7. Each artifact read and write records a fire-and-forget usage-metering event into the in-process
   display aggregator [`crate::usage_meter`] — a `ReadHit` / `ReadMiss` on GET and a `Write` on PUT —
   off the hot path (no await/I/O), DISPLAY telemetry only, never gating the response
   (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:282-283`; `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02_part2.rs:9-160`).
8. The Turborepo artifact signature is carried end to end. A client with
   `TURBO_REMOTE_CACHE_SIGNATURE_KEY` set sends `x-artifact-tag` on PUT; the route reads that header
   into the request (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02_part2.rs:9-160`), the bridge validates
   it (`crates/corelink-turbo-bridge/src/error.rs:93`) and stores it as a sidecar object AFTER the
   artifact write (`crates/corelink-turbo-bridge/src/adapter.rs:320`), and GET reads the sidecar back
   (`crates/corelink-turbo-bridge/src/adapter.rs:400`) and echoes it as a response header
   (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:308`). CoreLink never holds the signing key, so
   it neither computes nor checks the tag — its role is store-and-echo of an opaque value
   (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs:29`).

# Invariants
- `teamId` is required on GET/PUT but is NOT a security boundary; the authenticated tenant is the sole isolation key (`crates/corelink-container/src/routes/turbo_v8.rs:28-37`).
- An artifact GET requires read capability; an insufficient scope is rejected 403 before storage (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:241`).
- The telemetry `events` route is capped at `EVENTS_BODY_LIMIT_BYTES` (64 KiB), not the artifact cap (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs:74`; `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02_part2.rs:153`).
- Artifact bodies are bounded at `TURBO_BODY_LIMIT_BYTES` (100 MiB) so a PAT cannot OOM the shared container (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs:37`).
- The GET read path is concurrency-bounded like PUT: per-tenant cap 4 plus a global cap derived from the deployed basic container, both reserved before the artifact is buffered, so neither one tenant nor an aggregate read burst can OOM the container (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs:62`; `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs:117`).
- PUT is create-only (`put_if_absent`): a PUT to a key that already holds an artifact is refused `409 Conflict`, never overwritten (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02_part2.rs:347-374`). Turborepo keys are opaque/client-chosen, not content-addressed, so an overwrite could replace the bytes behind a tenant's own existing key — within-tenant cache poisoning the content envelope cannot detect. The presence probe runs under the per-`(tenant, team, hash)` write lock so the probe→refuse-or-write is serialized per stored object; a probe backend error fails OPEN (a transient storage error never blocks a legitimate first insert). Proven safe against the real `turbo` client, which never re-PUTs an existing key in normal operation and tolerates the 409 as a non-fatal warning (`docs/design/2026-08-24-turborepo-create-only-evidence.md`).
- An `x-artifact-tag` supplied on PUT is stored with the artifact and returned verbatim on the matching GET; CoreLink does not hold the customer's signing key and therefore never computes or validates the value (`crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs:29`; `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:316`).
- ABSENCE of a tag is valid on both verbs and is the normal case (most clients do not enable signatures): an untagged PUT succeeds and its GET omits the header rather than inventing one (`crates/corelink-turbo-bridge/src/adapter.rs:400`). Fail-closed applies only to a tag that is PRESENT and malformed — empty, over `MAX_ARTIFACT_TAG_LEN`, or non-printable-ASCII — which is rejected `400` before any storage or audit, never silently dropped (`crates/corelink-turbo-bridge/src/error.rs:93`; `crates/corelink-turbo-bridge/src/adapter.rs:237`; `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02_part2.rs:359-374`).
- The tag sidecar keyspace is disjoint from the artifact keyspace BY CONSTRUCTION: sidecars live under a reserved `$tag/` first segment, and `team_id` is charset-restricted to `[A-Za-z0-9_-]`, so no client-chosen hash — opaque and only length-bounded — can name a sidecar key (`crates/corelink-turbo-bridge/src/adapter.rs:170`; `crates/corelink-turbo-bridge/src/adapter.rs:174`).

# Gotchas
- axum honours the INNERMOST `DefaultBodyLimit`, so the order of the layers matters: the `events`
  route's own 64 KiB limit must be layered directly on its handler, otherwise the outer 100 MiB
  artifact limit would widen telemetry back to 100 MiB and reopen the OOM vector.
- The backing store is selected at `build_handlers` time: when storage credentials are configured
  (`StorageEnv::from_env()`), it is the **durable** per-tenant R2-backed `R2KvStore` ("durable storage"),
  so artifacts persist across container restarts (this closed the former `TODO(v2)`); only the no-creds
  dev/CI path falls back to the in-RAM `InMemoryKvStore`. If creds ARE present but `R2KvStore` refuses to
  build, the handler does NOT silently fall back — it mounts a fail-CLOSED handler that 503s every verb so
  durability is never silently lost. The route handlers and audit surface are identical across both backings.
- The `x-artifact-tag` sidecar is written AFTER the artifact, never before, so a sidecar can only exist
  for an artifact that actually landed and a create-only-refused (409) re-PUT returns before the sidecar
  write and cannot replace a stored tag. On GET the three sidecar outcomes are deliberately distinct: a
  hit echoes, a `NotFound` means "stored unsigned" and is NOT an error, and any other backend error
  propagates rather than serving the artifact tagless — serving a silently unsigned hit to a client that
  asked to verify is the failure mode this surface exists to avoid.

# Citations
1. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:166-195` — the router with static-before-wildcard ordering + body-limit layers.
2. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:217` — `handle_get` artifact read handler.
3. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:241` — the read-scope gate (fail-closed).
4. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:180-185` — `events` route's own innermost body limit.
5. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs:74` — `EVENTS_BODY_LIMIT_BYTES` (64 KiB).
6. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:187-195` — per-route 100 MiB artifact limit override.
7. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs:37` — `TURBO_BODY_LIMIT_BYTES` (100 MiB).
8. `crates/corelink-container/src/routes/turbo_v8.rs:28-37` — `teamId` sub-namespace vs authenticated-tenant isolation.
9. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01_part2.rs:88-190` — `GetConcurrencyGuard`: per-tenant GET concurrency cap (429 before buffering), the read twin of `PutConcurrencyGuard`.
10. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01_part2.rs:190-286` — `GlobalGetBudgetGuard`: process-wide GET budget (503 on saturation), separate pool from the PUT budget.
11. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs:62` — `TURBO_GET_CONCURRENCY_LIMIT` (4); `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs:117` — `GLOBAL_TURBO_GET_PERMITS` (derived from `container_capacity`).
12. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:81-159` — `build_handlers` store selection: durable `R2KvStore` when `StorageEnv::from_env()` is present (persists across restarts), in-RAM `InMemoryKvStore` only in the no-creds dev/CI fallback, fail-CLOSED handler when creds are present but R2 refuses to build.
13. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:282-283` (GET read HIT), `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02_part2.rs:9-160` (PUT write) — fire-and-forget usage-metering `record` calls (DISPLAY telemetry, off the hot path).
14. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02_part2.rs:347-374` — `map_err(AlreadyExists)` → `409 Conflict`: the create-only (`put_if_absent`) refusal of an overwrite. The presence probe itself lives in `corelink-turbo-bridge`'s adapter, under the route's per-`(tenant, team, hash)` write lock. Client evidence: `docs/design/2026-08-24-turborepo-create-only-evidence.md`.
15. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_01.rs:29` — `ARTIFACT_TAG_HEADER`: the Turborepo artifact-signature header CoreLink stores and echoes but never validates (the signing key is the customer's).
16. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02_part2.rs:9-160` — PUT reads `x-artifact-tag` off the request into `TurboPutRequest`; `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02.rs:316` — GET writes the stored tag back onto the response, omitting the header entirely when none was stored.
17. `crates/corelink-container/src/routes/turbo_v8/b126_m2_impl_02_part2.rs:359-374` — `map_err(ArtifactTagInvalid)` → `400`: a PRESENT but malformed tag fails the request instead of being silently dropped.
18. `crates/corelink-turbo-bridge/src/error.rs:93` — `validate_artifact_tag`: non-empty, `<= MAX_ARTIFACT_TAG_LEN`, printable-ASCII only (so a tag can never inject or mangle the echoed response header).
19. `crates/corelink-turbo-bridge/src/adapter.rs:237` — the tag is validated with the other input guards, BEFORE any audit emit or storage access.
20. `crates/corelink-turbo-bridge/src/adapter.rs:320` — the sidecar write, ordered AFTER the artifact write so it exists only for an artifact that landed and a 409-refused re-PUT cannot reach it.
21. `crates/corelink-turbo-bridge/src/adapter.rs:400` — the GET sidecar read: `NotFound` ⇒ `None` (stored unsigned, not an error), any other backend error propagates rather than serving a tagless hit.
22. `crates/corelink-turbo-bridge/src/adapter.rs:170` — `TAG_KEY_NAMESPACE` (`$tag`), the reserved first key segment; `crates/corelink-turbo-bridge/src/adapter.rs:174` — `tag_storage_key`. `team_id`'s `[A-Za-z0-9_-]` charset is what makes the two keyspaces disjoint by construction.
