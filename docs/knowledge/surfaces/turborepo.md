---
type: "CacheSurface"
title: "Turborepo v8 remote-cache surface"
description: "The Vercel Turborepo /v8/artifacts remote-cache protocol wired onto CoreLink, with teamId-as-sub-namespace isolation and per-route body caps."
source_files:
  - "crates/corelink-container/src/routes/turbo_v8.rs"
checkpoint_sha: "11947e0423eb06b58ae2e63600804d23bf18a48a"
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
cross-tenant access is impossible.

# How it works
1. The router registers the static `events`/`status` routes BEFORE the `:hash` artifact routes so
   matchit prefers the literals over the capture (`crates/corelink-container/src/routes/turbo_v8.rs:1036-1065`).
2. `handle_get` serves an artifact read and requires a cache-read scope (`cas:rw` or `cas:r`) before
   any storage (`crates/corelink-container/src/routes/turbo_v8.rs:1078`; `crates/corelink-container/src/routes/turbo_v8.rs:1102-1103`).
3. The `events` route is accept-and-drop telemetry carrying its own innermost 64 KiB body limit so it
   does NOT inherit the 100 MiB artifact cap (`crates/corelink-container/src/routes/turbo_v8.rs:1050-1053`; `crates/corelink-container/src/routes/turbo_v8.rs:140`).
4. The artifact routes get a per-route 100 MiB `DefaultBodyLimit` overriding the 10 MiB global default
   while still bounding the body (`crates/corelink-container/src/routes/turbo_v8.rs:1056-1064`; `crates/corelink-container/src/routes/turbo_v8.rs:103`).
5. The isolation tenant is the PAT-resolved authenticated tenant; `teamId` is demoted to a sub-namespace
   within it (`crates/corelink-container/src/routes/turbo_v8.rs:28-37`).
6. Turbo GET is concurrency-bounded mirroring PUT: `handle_get` declares two `FromRequestParts` guards
   AHEAD of buffering — a per-tenant `GetConcurrencyGuard` (cap `TURBO_GET_CONCURRENCY_LIMIT` = 4, 429 on
   over-cap) and a process-wide `GlobalGetBudgetGuard` (cap `GLOBAL_TURBO_GET_PERMITS` = 16, 503 on
   saturation) — so an over-cap read is rejected BEFORE the up-to-100 MiB artifact is read into the heap,
   bounding per-tenant and aggregate read-path memory on a pool SEPARATE from writes
   (`crates/corelink-container/src/routes/turbo_v8.rs:1091-1098`; `crates/corelink-container/src/routes/turbo_v8.rs:654-724`; `crates/corelink-container/src/routes/turbo_v8.rs:744-780`).
7. Each artifact read and write records a fire-and-forget usage-metering event into the in-process
   display aggregator [`crate::usage_meter`] — a `ReadHit` / `ReadMiss` on GET and a `Write` on PUT —
   off the hot path (no await/I/O), DISPLAY telemetry only, never gating the response
   (`crates/corelink-container/src/routes/turbo_v8.rs:1151-1152`; `crates/corelink-container/src/routes/turbo_v8.rs:1310-1311`).

# Invariants
- `teamId` is required on GET/PUT but is NOT a security boundary; the authenticated tenant is the sole isolation key (`crates/corelink-container/src/routes/turbo_v8.rs:28-37`).
- An artifact GET requires read capability; an insufficient scope is rejected 403 before storage (`crates/corelink-container/src/routes/turbo_v8.rs:1102-1103`).
- The telemetry `events` route is capped at `EVENTS_BODY_LIMIT_BYTES` (64 KiB), not the artifact cap (`crates/corelink-container/src/routes/turbo_v8.rs:140`; `crates/corelink-container/src/routes/turbo_v8.rs:1050-1053`).
- Artifact bodies are bounded at `TURBO_BODY_LIMIT_BYTES` (100 MiB) so a PAT cannot OOM the shared container (`crates/corelink-container/src/routes/turbo_v8.rs:103`).
- The GET read path is concurrency-bounded like PUT: per-tenant cap 4 + global cap 16, both reserved before the artifact is buffered, so neither one tenant nor an aggregate read burst can OOM the container (`crates/corelink-container/src/routes/turbo_v8.rs:128`; `crates/corelink-container/src/routes/turbo_v8.rs:183`).

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

# Citations
1. `crates/corelink-container/src/routes/turbo_v8.rs:1036-1065` — the router with static-before-wildcard ordering + body-limit layers.
2. `crates/corelink-container/src/routes/turbo_v8.rs:1078` — `handle_get` artifact read handler.
3. `crates/corelink-container/src/routes/turbo_v8.rs:1102-1103` — the read-scope gate (fail-closed).
4. `crates/corelink-container/src/routes/turbo_v8.rs:1050-1053` — `events` route's own innermost body limit.
5. `crates/corelink-container/src/routes/turbo_v8.rs:140` — `EVENTS_BODY_LIMIT_BYTES` (64 KiB).
6. `crates/corelink-container/src/routes/turbo_v8.rs:1056-1064` — per-route 100 MiB artifact limit override.
7. `crates/corelink-container/src/routes/turbo_v8.rs:103` — `TURBO_BODY_LIMIT_BYTES` (100 MiB).
8. `crates/corelink-container/src/routes/turbo_v8.rs:28-37` — `teamId` sub-namespace vs authenticated-tenant isolation.
9. `crates/corelink-container/src/routes/turbo_v8.rs:654-724` — `GetConcurrencyGuard`: per-tenant GET concurrency cap (429 before buffering), the read twin of `PutConcurrencyGuard`.
10. `crates/corelink-container/src/routes/turbo_v8.rs:744-780` — `GlobalGetBudgetGuard`: process-wide GET budget (503 on saturation), separate pool from the PUT budget.
11. `crates/corelink-container/src/routes/turbo_v8.rs:128` — `TURBO_GET_CONCURRENCY_LIMIT` (4); `crates/corelink-container/src/routes/turbo_v8.rs:183` — `GLOBAL_TURBO_GET_PERMITS` (16).
12. `crates/corelink-container/src/routes/turbo_v8.rs:951-1029` — `build_handlers` store selection: durable `R2KvStore` when `StorageEnv::from_env()` is present (persists across restarts), in-RAM `InMemoryKvStore` only in the no-creds dev/CI fallback, fail-CLOSED handler when creds are present but R2 refuses to build.
13. `crates/corelink-container/src/routes/turbo_v8.rs:1151-1152` (GET read HIT), `crates/corelink-container/src/routes/turbo_v8.rs:1310-1311` (PUT write) — fire-and-forget usage-metering `record` calls (DISPLAY telemetry, off the hot path).
