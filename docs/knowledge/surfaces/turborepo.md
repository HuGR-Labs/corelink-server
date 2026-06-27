---
type: "CacheSurface"
title: "Turborepo v8 remote-cache surface"
description: "The Vercel Turborepo /v8/artifacts remote-cache protocol wired onto CoreLink, with teamId-as-sub-namespace isolation and per-route body caps."
source_files:
  - "crates/corelink-container/src/routes/turbo_v8.rs"
checkpoint_sha: "41d84e271568cb47df664806fa3dc9798c134249"
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
   matchit prefers the literals over the capture (`crates/corelink-container/src/routes/turbo_v8.rs:794-823`).
2. `handle_get` serves an artifact read and requires a cache-read scope (`cas:rw` or `cas:r`) before
   any storage (`crates/corelink-container/src/routes/turbo_v8.rs:832`; `crates/corelink-container/src/routes/turbo_v8.rs:842-843`).
3. The `events` route is accept-and-drop telemetry carrying its own innermost 64 KiB body limit so it
   does NOT inherit the 100 MiB artifact cap (`crates/corelink-container/src/routes/turbo_v8.rs:808-812`; `crates/corelink-container/src/routes/turbo_v8.rs:126`).
4. The artifact routes get a per-route 100 MiB `DefaultBodyLimit` overriding the 10 MiB global default
   while still bounding the body (`crates/corelink-container/src/routes/turbo_v8.rs:815-822`; `crates/corelink-container/src/routes/turbo_v8.rs:103`).
5. The isolation tenant is the PAT-resolved authenticated tenant; `teamId` is demoted to a sub-namespace
   within it (`crates/corelink-container/src/routes/turbo_v8.rs:848`).

# Invariants
- `teamId` is required on GET/PUT but is NOT a security boundary; the authenticated tenant is the sole isolation key (`crates/corelink-container/src/routes/turbo_v8.rs:848`).
- An artifact GET requires read capability; an insufficient scope is rejected 403 before storage (`crates/corelink-container/src/routes/turbo_v8.rs:842-843`).
- The telemetry `events` route is capped at `EVENTS_BODY_LIMIT_BYTES` (64 KiB), not the artifact cap (`crates/corelink-container/src/routes/turbo_v8.rs:126`; `crates/corelink-container/src/routes/turbo_v8.rs:808-812`).
- Artifact bodies are bounded at `TURBO_BODY_LIMIT_BYTES` (100 MiB) so a PAT cannot OOM the shared container — the bound is applied by the `DefaultBodyLimit::max` layer (`crates/corelink-container/src/routes/turbo_v8.rs:822`).

# Gotchas
- axum honours the INNERMOST `DefaultBodyLimit`, so the order of the layers matters: the `events`
  route's own 64 KiB limit must be layered directly on its handler, otherwise the outer 100 MiB
  artifact limit would widen telemetry back to 100 MiB and reopen the OOM vector.
- The R2-backed swap is DONE: the executed `build_handlers` selects the backing at runtime —
  `if StorageEnv::from_env().is_some()` builds the durable `R2KvStore` and returns the state wired to it
  (`crates/corelink-container/src/routes/turbo_v8.rs:722-745`), and if creds ARE present but the store
  refuses to build it mounts the fail-CLOSED `UnavailableTurboHandler` rather than silently falling back
  (`crates/corelink-container/src/routes/turbo_v8.rs:749-754`). So artifacts persist across container
  restarts; the in-RAM `InMemoryKvStore` is only the no-credentials dev/CI fallback (the route handlers
  and audit surface are identical for both backings; the `:705-708` /// only narrates this).

# Citations
1. `crates/corelink-container/src/routes/turbo_v8.rs:794-823` — the router with static-before-wildcard ordering + body-limit layers.
2. `crates/corelink-container/src/routes/turbo_v8.rs:832` — `handle_get` artifact read handler.
3. `crates/corelink-container/src/routes/turbo_v8.rs:842-843` — the read-scope gate (fail-closed).
4. `crates/corelink-container/src/routes/turbo_v8.rs:808-812` — `events` route's own innermost body limit.
5. `crates/corelink-container/src/routes/turbo_v8.rs:126` — `EVENTS_BODY_LIMIT_BYTES` (64 KiB).
6. `crates/corelink-container/src/routes/turbo_v8.rs:815-822` — per-route 100 MiB artifact limit override.
7. `crates/corelink-container/src/routes/turbo_v8.rs:103` — `TURBO_BODY_LIMIT_BYTES` (100 MiB).
8. `crates/corelink-container/src/routes/turbo_v8.rs:848` — `caller_tenant = auth.0`: the PAT-resolved authenticated tenant is the isolation key; `teamId` is a sub-namespace label only.
9. `crates/corelink-container/src/routes/turbo_v8.rs:722-745` — the EXECUTED runtime backing selection (`StorageEnv::from_env().is_some()` → durable `R2KvStore`), with the fail-CLOSED `UnavailableTurboHandler` on a creds-present build failure at `:749-754`; the `:705-708` /// only documents it.
