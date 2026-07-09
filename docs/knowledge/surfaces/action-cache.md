---
type: "CacheSurface"
title: "Action Cache (AC) surface"
description: "The first-party Action Cache routes mapping an action digest to its cached result, with cross-tenant denial and a canonical-digest gate shared with native CAS."
source_files:
  - "crates/corelink-container/src/routes/ac.rs"
checkpoint_sha: "86e439a821d3c407cc94f4b30ba2b7a7c563d958"
provenance: "AUTHORED"
tags: ["surfaces", "action-cache", "cache", "reapi"]
timestamp: "2026-06-26T00:00:00Z"
---

# Action Cache (AC) surface

The Action Cache is the second half of a remote build cache: where the [native CAS surface](/surfaces/native-cas.md)
stores content keyed by its own digest, the AC maps an *action digest* (the hash of a build action's
inputs) to the cached *result* of running it — so a build can skip re-execution when the inputs are
unchanged. It lives in the Rust container plane and mirrors the CAS wire-up shape exactly: trait-object
handlers so the in-memory fake can be swapped for the wasm32 R2-bound impl without touching the route
layer. Its security spine is cross-tenant denial plus a canonical-digest gate; the digest validator
`is_canonical_digest` defined here is the SAME helper the native CAS surface reuses.

# Role
It serves `GET /v1/ac/:tenant/:action_digest` (lookup), `PUT` (update), `DELETE`, and the per-tenant
ref-list route, emitting AC availability/latency SLO observations on each entry. Tenant isolation is
the authenticated tenant; cross-tenant attempts are denied 403 at the route level before any storage access.

# How it works
1. The router mounts lookup/update/delete on `AC_LOOKUP_ROUTE` plus the ref-list route, each carrying
   its SLO-emitting handler (`crates/corelink-container/src/routes/ac.rs:442-450`).
2. A lookup is served by `handle_lookup` (`crates/corelink-container/src/routes/ac.rs:483`); an update
   by `handle_update` (`crates/corelink-container/src/routes/ac.rs:543`).
3. The `:action_digest` segment is validated as exactly 64 lowercase-hex chars by `is_canonical_digest`
   BEFORE it derives an R2 key (`crates/corelink-container/src/routes/ac.rs:452-461`).
4. The native PAT possession gate re-verifies the bearer against the claimed tenant and rejects
   forged/wrong-tenant tokens before storage (`crates/corelink-container/src/routes/ac.rs:463-480`).
5. Delete and per-tenant ref enumeration are served by `handle_delete` and `handle_list_refs`
   (`crates/corelink-container/src/routes/ac.rs:650`; `crates/corelink-container/src/routes/ac.rs:716`).

# Invariants
- A non-canonical action digest (not 64 lowercase-hex) is rejected 400 before storage (`crates/corelink-container/src/routes/ac.rs:459-461`).
- Cross-tenant attempts are rejected **HTTP 403** by the route-level tenant check
  (`crates/corelink-container/src/routes/ac.rs:495-496`, `:559-560`, `:660-661`), which returns
  **before** `lookup`/`update` runs — so this reject path itself writes **no** audit row. (The
  `LookupDenied`/`UpdateDenied` audit rows are emitted by the lookup/update handlers on
  authorized-but-denied paths, not by this route-level cross-tenant 403.)
- Per-tenant concurrent writes are bounded by `AC_WRITE_CONCURRENCY_LIMIT` (`crates/corelink-container/src/routes/ac.rs:168`).
- A forged or wrong-tenant bearer PAT is rejected by the possession gate before any storage access (`crates/corelink-container/src/routes/ac.rs:463-480`).
- A **create-only (deny-overwrite)** runner-job cred may CREATE a new `(tenant, action_digest)` entry
  but is rejected **HTTP 409 `AC_CREATE_ONLY`** on any OVERWRITE of an existing one (anti AC-squat, the
  AC analog of the runner-job deny-DELETE). Enforced ATOMICALLY off the store's put-if-absent `durable`
  signal — a non-durable write is the conflict (`crates/corelink-container/src/routes/ac.rs:624-625`),
  and a divergent-body overwrite maps to the same conflict (`crates/corelink-container/src/routes/ac.rs:639-640`)
  rather than the generic divergent-body 409. The conflict body is built by `ac_create_only_conflict`
  (`crates/corelink-container/src/routes/ac.rs:781`).

# Gotchas
- The route MUST use the matchit `:name` capture form, not `{name}`; matchit 0.7.3 (pinned via
  `axum = "0.7"`) treats `{name}` as literal path bytes, which silently 404s every real request — this
  was DEBT-029, closed on `ac.rs`/`admin.rs`.
- Lowercase-only canonicalization is deliberate: it prevents case-variant key collisions in the
  content-addressing space, so a mixed-case digest is rejected rather than normalized.

# Citations
1. `crates/corelink-container/src/routes/ac.rs:442-450` — the AC router (lookup/update/delete + ref list).
2. `crates/corelink-container/src/routes/ac.rs:483` — `handle_lookup`.
3. `crates/corelink-container/src/routes/ac.rs:542` — `handle_update`.
4. `crates/corelink-container/src/routes/ac.rs:452-461` — `is_canonical_digest` (64 lowercase-hex) gate.
5. `crates/corelink-container/src/routes/ac.rs:463-480` — `pat_gate_reject` native PAT possession gate.
6. `crates/corelink-container/src/routes/ac.rs:650` — `handle_delete`.
7. `crates/corelink-container/src/routes/ac.rs:716` — `handle_list_refs`.
8. `crates/corelink-container/src/routes/ac.rs:495-496` — cross-tenant 403 (lookup), route-level, before storage.
8b. `crates/corelink-container/src/routes/ac.rs:558-559` — cross-tenant 403 (update), route-level, before storage.
8c. `crates/corelink-container/src/routes/ac.rs:659-660` — cross-tenant 403 (delete), route-level, before storage.
9. `crates/corelink-container/src/routes/ac.rs:168` — `AC_WRITE_CONCURRENCY_LIMIT`.
