---
type: "CacheSurface"
title: "Bazel REAPI v2 surface"
description: "The Bazel cache surface: the CoreLink REAPI ByteStream REST scheme AND the stock-Bazel HTTP cache alias (`/bazel/cache/{cas,ac}/:hash`), both onto the same R2 blobs as native CAS/AC."
source_files:
  - "crates/corelink-container/src/routes/bazel_v2.rs"
checkpoint_sha: "8af9ed65caf286d3f800e91d3f823face3aefd31"
provenance: "AUTHORED"
tags: ["surfaces", "bazel", "reapi", "cache"]
timestamp: "2026-06-26T00:00:00Z"
---

# Bazel REAPI v2 surface

This surface speaks the Bazel Remote Execution API (REAPI) v2 cache protocol and serves TWO wire
schemes onto the SAME `BazelAdapter`/handlers and the same per-tenant R2 store:

1. The CoreLink **REAPI ByteStream REST scheme** (`/bazel/v2/:instance/blobs/:hash/:size`) — for a
   REAPI/ByteStream client (or `bazel` configured against this scheme). The tenant is the `:instance`
   path segment.
2. The **stock-Bazel HTTP cache alias** (`/bazel/cache/{cas,ac}/:hash`) — what vanilla
   `bazel --remote_cache=https://host/bazel/cache` actually sends: `GET`/`PUT` on `/cas/<hash>` and
   `/ac/<hash>` with **no `:instance` segment and no `:size`**. Buck2 is NOT a client of this alias:
   it reaches a cache only over REAPI **gRPC** and has no plain-HTTP cache backend
   (`crates/corelink-container/src/routes/bazel_v2.rs:9-27`).
   Here the tenant (== REAPI `instance`) is derived from the Worker-injected `x-corelink-tenant-id`
   header, so a missing/sentinel tenant → 401 and isolation is by the per-tenant namespace (a
   cross-tenant hash is a uniform 404, never another tenant's bytes). This alias was previously
   DEFERRED (stock `--remote_cache=http` 404'd here); it is now BUILT.

It is a protocol adapter, not a new store: every blob it serves is backed by the same R2 blobs the
[native CAS](/surfaces/native-cas.md) and [Action Cache](/surfaces/action-cache.md) surfaces use, so
a result cached by one front-door is visible to the others. It lives in the container plane and
translates REAPI paths/digests into `BazelAdapter`/`FindMissingHandler` calls, mapping bridge errors
to canonical HTTP status codes.

# Role
It mounts the five REAPI v2 REST cache endpoints (CAS read/write, AC read/write, and
`findMissingBlobs`) under `/bazel/v2/:instance/...`, where `:instance` is the tenant boundary, PLUS
four stock-Bazel HTTP alias endpoints under `/bazel/cache/{cas,ac}/:hash` where the tenant is the
header. The route layer extracts path params, builds the REAPI digest, reads Worker-injected caller
metadata, and dispatches.

# How it works
1. The router registers the five REAPI REST routes under `/bazel/v2/:instance/...` plus the four stock-alias routes under `/bazel/cache/...` (`crates/corelink-container/src/routes/bazel_v2.rs:304-346`).
2. CAS read is `GET /bazel/v2/:instance/blobs/:hash/:size` → `handle_cas_read` (`crates/corelink-container/src/routes/bazel_v2.rs:305-308`).
3. AC read/write share `GET`/`PUT /bazel/v2/:instance/blobs/ac/:hash/:size`, differentiated by method (`crates/corelink-container/src/routes/bazel_v2.rs:314-317`).
4. CAS write is `PUT /bazel/v2/:instance/uploads/:uuid/blobs/:hash/:size` (`crates/corelink-container/src/routes/bazel_v2.rs:319-322`).
5. Batch find-missing is `POST /bazel/v2/:instance/findMissingBlobs` (`crates/corelink-container/src/routes/bazel_v2.rs:324-327`); its `handle_find_missing` scope gate is fail-CLOSED on the find-missing capability — `if !scope.can_find_missing()` → 403 BEFORE any tenant/storage access (`crates/corelink-container/src/routes/bazel_v2.rs:826`).
6. The stock-Bazel HTTP alias mounts `GET/PUT /bazel/cache/cas/:hash` and `GET/PUT /bazel/cache/ac/:hash` onto the SAME adapter/handlers (`crates/corelink-container/src/routes/bazel_v2.rs:338-345`); the alias handlers derive the REAPI `instance` from the `x-corelink-tenant-id` header (not a path segment) and run the IDENTICAL gate sequence as the REST handlers (`crates/corelink-container/src/routes/bazel_v2.rs:880-1129`).
7. Caller metadata is read from Worker-injected headers via `header_str`, defaulting when absent (`crates/corelink-container/src/routes/bazel_v2.rs:352-359`).
8. The authenticated tenant is extracted fail-CLOSED by `caller_tenant`, which routes its missing/empty/reserved-sentinel rejection through the SHARED `crate::auth_tenant::is_reserved_sentinel` — so this REAPI surface (which has no `AuthTenant` extractor) rejects the full reserved set including `_oci` and `_public` (`PUBLIC_NAMESPACE`), and `BazelPutGuard` reserves a write slot only for a non-sentinel tenant via the same check (`crates/corelink-container/src/routes/bazel_v2.rs:376-386`; `crates/corelink-container/src/routes/bazel_v2.rs:193`).
9. Each CAS/AC read and write records a fire-and-forget usage-metering event into the in-process display aggregator [`crate::usage_meter`] — a `ReadHit` / `ReadMiss` on reads and a `Write` on writes — off the hot path (no await/I/O), DISPLAY telemetry only, never gating the response (`crates/corelink-container/src/routes/bazel_v2.rs:576`; `crates/corelink-container/src/routes/bazel_v2.rs:670`; `crates/corelink-container/src/routes/bazel_v2.rs:800`).

# Invariants
- REST scheme: the `:instance` path segment MUST equal the Worker-injected `x-corelink-tenant-id`; a mismatch is 403 with an audit row — the executed enforcer maps the bridge's `BazelBridgeError::CrossTenantDenied` to `StatusCode::FORBIDDEN` (`crates/corelink-container/src/routes/bazel_v2.rs:513`). The documented rule lives in the module doc (`crates/corelink-container/src/routes/bazel_v2.rs:45-48`).
- Client-vs-server error hygiene: `map_bridge_err` maps a CALLER's fault to a 4xx, never a 5xx — a malformed `findMissingBlobs` JSON body surfaces as `BazelBridgeError::InvalidRequest` → `400 BAD_REQUEST` (`crates/corelink-container/src/routes/bazel_v2.rs:504`), so a bad request can never pollute the server error-rate / SLO signal; only genuine server faults (lock poisoning, response serialisation) map to `Internal` → 500.
- Stock alias: there is NO `:instance` to mismatch — the tenant is the header alone, so isolation is by the per-tenant namespace and a cross-tenant hash is a UNIFORM 404 (never another tenant's bytes), while a missing/sentinel tenant is 401. Both schemes run the same `scope → tenant → PAT → quota` gate order; the alias CAS write keeps the SHA-256 content-addressing boundary check (`crates/corelink-container/src/routes/bazel_v2.rs:987`) and the alias AC write keeps the WP5b runner-job AC-key pin (`crates/corelink-container/src/routes/bazel_v2.rs:1088`). The four WRITE handlers use `pat_gate_reject_write` (not `pat_gate_reject`), which enforces the PAT's D1-derived `can_write` capability on top of the Worker scope header — so a read-only PAT is rejected `403` on a write path even if the header were wrong (deep-audit B/F-1, matching cargo/OCI).
- `findMissingBlobs` is gated on the least-privilege **find-missing** capability, NOT plain read (ADR-0071): the handler rejects `403` unless `scope.can_find_missing()` — which is satisfied by an explicit `find-missing` grant OR any read grant (read ⊇ find-missing), so pre-ADR-0071 `cas:r`/`cas:rw`/`admin` PATs are unchanged while a find-only PAT passes here and ONLY here (never a CAS download) (`crates/corelink-container/src/routes/bazel_v2.rs:826`).
- All routes use the matchit `{name}` capture form, never `:name` (post axum-0.8), per the DEBT-029 rule (`crates/corelink-container/src/routes/bazel_v2.rs:59-67`).
- Per-tenant concurrent writes are bounded by `BAZEL_WRITE_CONCURRENCY_LIMIT` (`crates/corelink-container/src/routes/bazel_v2.rs:140`) on both schemes.
- The bytes served are the same R2 blobs as the native CAS/AC endpoints (a shared store, not a copy) (`crates/corelink-container/src/routes/bazel_v2.rs:5-7`); the stock alias reads/writes that same store.
- A missing/empty/reserved-sentinel `x-corelink-tenant-id` is rejected `401` through the shared `is_reserved_sentinel`, so a `_oci`/`_public` masquerade can never reach the cross-tenant dedup namespace via either scheme (`crates/corelink-container/src/routes/bazel_v2.rs:376-386`).

# Gotchas
- `/blobs/ac/:hash/:size` resolves AHEAD of `/blobs/:hash/:size` because axum/matchit ranks the literal
  `ac` segment above the `:hash` wildcard — AC and CAS read share a path prefix and only differ by that
  literal, so route ordering, not method, disambiguates them.
- Tenant isolation on the REST scheme is enforced inside the adapter (which emits the audit row), not
  duplicated in the route layer, so the 403 path is the bridge's, not a route-local check. On the stock
  alias the adapter's cross-tenant guard is a tautology (`instance == caller_tenant` by construction),
  so isolation rests entirely on the per-tenant namespace (a uniform 404, not a 403).
- The stock alias carries no `:size`; reads use a `0` placeholder size (the read addresses purely by
  hash) and writes use the actual body length (making the adapter's size-match check a tautology, so
  the SHA-256 boundary check is the sole CAS integrity gate). Buck2 is out of scope on BOTH schemes:
  it speaks REAPI over gRPC only (cache AND execution), and this surface is HTTP — CoreLink is
  cache-only and serves no gRPC.

# Citations
1. `crates/corelink-container/src/routes/bazel_v2.rs:304-346` — the router (five REAPI REST endpoints + four stock-alias endpoints).
2. `crates/corelink-container/src/routes/bazel_v2.rs:305-308` — CAS read route (REST).
3. `crates/corelink-container/src/routes/bazel_v2.rs:314-317` — AC read/write route (method-differentiated).
4. `crates/corelink-container/src/routes/bazel_v2.rs:319-322` — CAS write (uploads/:uuid) route.
5. `crates/corelink-container/src/routes/bazel_v2.rs:324-327` — `findMissingBlobs` batch route.
5a. `crates/corelink-container/src/routes/bazel_v2.rs:826` — `handle_find_missing` fail-CLOSED scope gate on `can_find_missing()` (ADR-0071: read ⊇ find-missing; find-only PATs pass, read PATs unchanged).
6. `crates/corelink-container/src/routes/bazel_v2.rs:338-345` — stock-Bazel HTTP alias route registration (`/bazel/cache/{cas,ac}/:hash`).
7. `crates/corelink-container/src/routes/bazel_v2.rs:880-1129` — the four stock-alias handlers (tenant from header; same gate sequence + integrity checks as the REST handlers).
8. `crates/corelink-container/src/routes/bazel_v2.rs:352-359` — `header_str` Worker-metadata reader.
9. `crates/corelink-container/src/routes/bazel_v2.rs:513` — `CrossTenantDenied → 403`: the executed enforcer of the REST `:instance` == `x-corelink-tenant-id` isolation rule (the documented rule is in the module doc at `:44-47`).
10. `crates/corelink-container/src/routes/bazel_v2.rs:987` — stock-alias CAS-write SHA-256 content-addressing boundary check.
11. `crates/corelink-container/src/routes/bazel_v2.rs:1088` — stock-alias AC-write WP5b runner-job AC-key pin.
12. `crates/corelink-container/src/routes/bazel_v2.rs:59-67` — matchit `{name}` capture-form rule (DEBT-029, post axum-0.8).
13. `crates/corelink-container/src/routes/bazel_v2.rs:140` — `BAZEL_WRITE_CONCURRENCY_LIMIT`.
14. `crates/corelink-container/src/routes/bazel_v2.rs:5-7` — same R2 blobs as native CAS/AC.
15. `crates/corelink-container/src/routes/bazel_v2.rs:376-386` — `caller_tenant` fail-CLOSED via the shared `is_reserved_sentinel` (rejects `_oci`/`_public`).
16. `crates/corelink-container/src/routes/bazel_v2.rs:193` — `BazelPutGuard` reserves only for a non-sentinel tenant (same shared check).
17. `crates/corelink-container/src/routes/bazel_v2.rs:9-27` — client contract: two schemes (REAPI REST + stock HTTP alias), one store.
18. `crates/corelink-container/src/routes/bazel_v2.rs:576` (CAS read HIT), `crates/corelink-container/src/routes/bazel_v2.rs:670` (CAS write), `crates/corelink-container/src/routes/bazel_v2.rs:800` (AC write) — fire-and-forget usage-metering `record` calls (DISPLAY telemetry, off the hot path).
