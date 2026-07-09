---
type: "CacheSurface"
title: "Bazel REAPI v2 surface"
description: "The five REAPI v2 REST cache endpoints — the CoreLink REAPI ByteStream scheme (not stock --remote_cache=http) — that let a REAPI client hit the same R2 blobs as native CAS/AC."
source_files:
  - "crates/corelink-container/src/routes/bazel_v2.rs"
checkpoint_sha: "86e439a821d3c407cc94f4b30ba2b7a7c563d958"
provenance: "AUTHORED"
tags: ["surfaces", "bazel", "reapi", "cache"]
timestamp: "2026-06-26T00:00:00Z"
---

# Bazel REAPI v2 surface

This surface speaks the Bazel Remote Execution API (REAPI) v2 REST cache protocol. It requires the
CoreLink REAPI ByteStream scheme (`/bazel/v2/:instance/blobs/:hash/:size`) and a compatible
REAPI/ByteStream client — it is NOT stock Bazel's plain HTTP cache: a vanilla
`--remote_cache=http(s)://…` sends `/cas/<hash>` and `/ac/<hash>` (no `:instance`, no `:size`), which
does not match these routes and 404s. A stock-Bazel-HTTP `/cas|/ac` alias is a tracked enhancement
(DEFERRED), not a current capability. It is a protocol adapter, not a new store: every blob it serves is backed by the same
R2 blobs the [native CAS](/surfaces/native-cas.md) and [Action Cache](/surfaces/action-cache.md)
surfaces use, so a result cached by one front-door is visible to the others. It lives in the container
plane and translates REAPI paths/digests into `BazelAdapter`/`FindMissingHandler` calls, mapping
bridge errors to canonical HTTP status codes.

# Role
It mounts the five REAPI v2 cache endpoints (CAS read/write, AC read/write, and `findMissingBlobs`)
under `/bazel/v2/:instance/...`, where `:instance` is the tenant boundary. The route layer extracts
path params, builds the REAPI digest, reads Worker-injected caller metadata, and dispatches.

# How it works
1. The router registers exactly five REAPI routes under `/bazel/v2/:instance/...` (`crates/corelink-container/src/routes/bazel_v2.rs:290-316`).
2. CAS read is `GET /bazel/v2/:instance/blobs/:hash/:size` → `handle_cas_read` (`crates/corelink-container/src/routes/bazel_v2.rs:293-296`).
3. AC read/write share `GET`/`PUT /bazel/v2/:instance/blobs/ac/:hash/:size`, differentiated by method (`crates/corelink-container/src/routes/bazel_v2.rs:302-305`).
4. CAS write is `PUT /bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size` (`crates/corelink-container/src/routes/bazel_v2.rs:307-310`).
5. Batch find-missing is `POST /bazel/v2/:instance/findMissingBlobs` (`crates/corelink-container/src/routes/bazel_v2.rs:312-315`).
6. Caller metadata is read from Worker-injected headers via `header_str`, defaulting when absent (`crates/corelink-container/src/routes/bazel_v2.rs:322-329`).
7. The authenticated tenant is extracted fail-CLOSED by `caller_tenant`, which routes its missing/empty/reserved-sentinel rejection through the SHARED `crate::auth_tenant::is_reserved_sentinel` — so this REAPI surface (which has no `AuthTenant` extractor) rejects the full reserved set including `_oci` and `_public` (`PUBLIC_NAMESPACE`), and `BazelPutGuard` reserves a write slot only for a non-sentinel tenant via the same check (`crates/corelink-container/src/routes/bazel_v2.rs:346-356`; `crates/corelink-container/src/routes/bazel_v2.rs:179`).
8. Each CAS/AC read and write records a fire-and-forget usage-metering event into the in-process display aggregator [`crate::usage_meter`] — a `ReadHit` / `ReadMiss` on reads and a `Write` on writes — off the hot path (no await/I/O), DISPLAY telemetry only, never gating the response (`crates/corelink-container/src/routes/bazel_v2.rs:527-528`; `crates/corelink-container/src/routes/bazel_v2.rs:624-625`; `crates/corelink-container/src/routes/bazel_v2.rs:757-758`).

# Invariants
- The `:instance` path segment MUST equal the Worker-injected `x-corelink-tenant-id`; a mismatch is 403 with an audit row — the executed enforcer maps the bridge's `BazelBridgeError::CrossTenantDenied` to `StatusCode::FORBIDDEN` (`crates/corelink-container/src/routes/bazel_v2.rs:465`). The documented rule lives in the module doc (`crates/corelink-container/src/routes/bazel_v2.rs:31-34`).
- All routes use the matchit `:name` capture form, never `{name}`, per the DEBT-029 rule (`crates/corelink-container/src/routes/bazel_v2.rs:47-50`).
- Per-tenant concurrent writes are bounded by `BAZEL_WRITE_CONCURRENCY_LIMIT` (`crates/corelink-container/src/routes/bazel_v2.rs:126`).
- The bytes served are the same R2 blobs as the native CAS/AC endpoints (a shared store, not a copy) (`crates/corelink-container/src/routes/bazel_v2.rs:5-7`).
- The surface requires the CoreLink REAPI ByteStream scheme (`/bazel/v2/:instance/blobs/:hash/:size`); stock `--remote_cache=http` Bazel hits `/cas`,`/ac` and 404s — a stock-client `/cas|/ac` alias is DEFERRED, not built (`crates/corelink-container/src/routes/bazel_v2.rs:9-17`).
- A missing/empty/reserved-sentinel `x-corelink-tenant-id` is rejected `401` through the shared `is_reserved_sentinel`, so a `_oci`/`_public` masquerade can never reach the cross-tenant dedup namespace via this surface (`crates/corelink-container/src/routes/bazel_v2.rs:346-356`).

# Gotchas
- `/blobs/ac/:hash/:size` resolves AHEAD of `/blobs/:hash/:size` because axum/matchit ranks the literal
  `ac` segment above the `:hash` wildcard — AC and CAS read share a path prefix and only differ by that
  literal, so route ordering, not method, disambiguates them.
- Tenant isolation is enforced inside the adapter (which emits the audit row), not duplicated in the
  route layer, so the 403 path is the bridge's, not a route-local check.

# Citations
1. `crates/corelink-container/src/routes/bazel_v2.rs:290-316` — the REAPI v2 router (five endpoints).
2. `crates/corelink-container/src/routes/bazel_v2.rs:293-296` — CAS read route.
3. `crates/corelink-container/src/routes/bazel_v2.rs:302-305` — AC read/write route (method-differentiated).
4. `crates/corelink-container/src/routes/bazel_v2.rs:307-310` — CAS write (uploads/:uuid) route.
5. `crates/corelink-container/src/routes/bazel_v2.rs:312-315` — `findMissingBlobs` batch route.
6. `crates/corelink-container/src/routes/bazel_v2.rs:322-329` — `header_str` Worker-metadata reader.
7. `crates/corelink-container/src/routes/bazel_v2.rs:465` — `CrossTenantDenied → 403`: the executed enforcer of the `:instance` == `x-corelink-tenant-id` isolation rule (the documented rule is in the module doc at `:31-34`).
8. `crates/corelink-container/src/routes/bazel_v2.rs:47-50` — matchit `:name` capture-form rule (DEBT-029).
9. `crates/corelink-container/src/routes/bazel_v2.rs:126` — `BAZEL_WRITE_CONCURRENCY_LIMIT`.
10. `crates/corelink-container/src/routes/bazel_v2.rs:5-7` — same R2 blobs as native CAS/AC.
11. `crates/corelink-container/src/routes/bazel_v2.rs:346-356` — `caller_tenant` fail-CLOSED via the shared `is_reserved_sentinel` (rejects `_oci`/`_public`).
12. `crates/corelink-container/src/routes/bazel_v2.rs:179` — `BazelPutGuard` reserves only for a non-sentinel tenant (same shared check).
13. `crates/corelink-container/src/routes/bazel_v2.rs:9-17` — client contract: requires the REAPI ByteStream scheme; stock `--remote_cache=http` (`/cas`,`/ac`) 404s, alias DEFERRED.
14. `crates/corelink-container/src/routes/bazel_v2.rs:527-528` (CAS read HIT), `crates/corelink-container/src/routes/bazel_v2.rs:624-625` (CAS write), `crates/corelink-container/src/routes/bazel_v2.rs:757-758` (AC write) — fire-and-forget usage-metering `record` calls (DISPLAY telemetry, off the hot path).
