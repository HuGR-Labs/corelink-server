---
type: "CacheSurface"
title: "Action Cache (AC) surface"
description: "The first-party Action Cache routes mapping an action digest to its cached result, with cross-tenant denial and a canonical-digest gate shared with native CAS."
source_files:
  - "crates/corelink-container/src/routes/ac.rs"
checkpoint_sha: "ab4e1a5f80d90d5e95e4ec8f477c3f64cbaa5984"
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
2. A lookup is served by `handle_lookup` (`crates/corelink-container/src/routes/ac.rs:505`); an update
   by `handle_update` (`crates/corelink-container/src/routes/ac.rs:564`).
3. The `:action_digest` segment is validated as exactly 64 lowercase-hex chars by `is_canonical_digest`
   BEFORE it derives an R2 key (`crates/corelink-container/src/routes/ac.rs:459-463`).
4. On the READ path (lookup / ref-list) the native PAT possession gate `pat_gate_reject` re-verifies the
   bearer against the claimed tenant and rejects forged/wrong-tenant tokens before storage
   (`crates/corelink-container/src/routes/ac.rs:471-482`).
5. On the WRITE path (update / delete) the gate escalates to `pat_gate_reject_write`, which independently
   re-derives the PAT's D1-stored `can_write` capability at the container via `NativePatGate::verify_write`
   (not just tenant possession) — a read-only PAT is rejected 403 even if the Worker-set scope header
   claimed write, upholding the Option-B invariant that a compromised Worker cannot grant write on its own
   (`crates/corelink-container/src/routes/ac.rs:608` update / `crates/corelink-container/src/routes/ac.rs:708` delete; helper `verify_write` at `crates/corelink-container/src/routes/ac.rs:501`).
6. Delete and per-tenant ref enumeration are served by `handle_delete` and `handle_list_refs`
   (`crates/corelink-container/src/routes/ac.rs:674`; `crates/corelink-container/src/routes/ac.rs:742`).

# Invariants
- A non-canonical action digest (not 64 lowercase-hex) is rejected 400 before storage (`crates/corelink-container/src/routes/ac.rs:459-463`).
- Cross-tenant attempts are rejected **HTTP 403** by the route-level tenant check
  (`crates/corelink-container/src/routes/ac.rs:517-518`, `:580-581`, `:683-684`), which returns
  **before** `lookup`/`update` runs — so this reject path itself writes **no** audit row. (The
  `LookupDenied`/`UpdateDenied` audit rows are emitted by the lookup/update handlers on
  authorized-but-denied paths, not by this route-level cross-tenant 403.)
- Per-tenant concurrent writes are bounded by `AC_WRITE_CONCURRENCY_LIMIT` (`crates/corelink-container/src/routes/ac.rs:168`).
- A forged or wrong-tenant bearer PAT is rejected by the read-path possession gate before any storage access (`crates/corelink-container/src/routes/ac.rs:471-482`).
- An AC update/delete requires a WRITE-capable PAT re-derived at the container (`pat_gate_reject_write` → `NativePatGate::verify_write`), not just tenant possession — a read-only PAT is rejected 403 even if the Worker-set scope header claimed write (`crates/corelink-container/src/routes/ac.rs:608`, `:708`).
- A **create-only (deny-overwrite)** runner-job cred may CREATE a new `(tenant, action_digest)` entry
  but is rejected **HTTP 409 `AC_CREATE_ONLY`** on any OVERWRITE of an existing one (anti AC-squat, the
  AC analog of the runner-job deny-DELETE). Enforced ATOMICALLY off the store's put-if-absent `durable`
  signal — a non-durable write is the conflict (`crates/corelink-container/src/routes/ac.rs:648`),
  and a divergent-body overwrite maps to the same conflict (`crates/corelink-container/src/routes/ac.rs:663`)
  rather than the generic divergent-body 409. The conflict body is built by `ac_create_only_conflict`
  (`crates/corelink-container/src/routes/ac.rs:807`).

# Gotchas
- The route MUST use the matchit `:name` capture form, not `{name}`; matchit 0.7.3 (pinned via
  `axum = "0.7"`) treats `{name}` as literal path bytes, which silently 404s every real request — this
  was DEBT-029, closed on `ac.rs`/`admin.rs`.
- Lowercase-only canonicalization is deliberate: it prevents case-variant key collisions in the
  content-addressing space, so a mixed-case digest is rejected rather than normalized.

# Citations
1. `crates/corelink-container/src/routes/ac.rs:442-450` — the AC router (lookup/update/delete + ref list).
2. `crates/corelink-container/src/routes/ac.rs:505` — `handle_lookup`.
3. `crates/corelink-container/src/routes/ac.rs:564` — `handle_update`.
4. `crates/corelink-container/src/routes/ac.rs:459-463` — `is_canonical_digest` (64 lowercase-hex) gate.
5. `crates/corelink-container/src/routes/ac.rs:471-482` — `pat_gate_reject` native PAT possession gate (read path).
5b. `crates/corelink-container/src/routes/ac.rs:491-501` — `pat_gate_reject_write`: re-derives D1 `can_write` via `NativePatGate::verify_write` (line 501) on the update/delete write paths.
5c. `crates/corelink-container/src/routes/ac.rs:608` — `pat_gate_reject_write` call in `handle_update`.
5d. `crates/corelink-container/src/routes/ac.rs:708` — `pat_gate_reject_write` call in `handle_delete`.
6. `crates/corelink-container/src/routes/ac.rs:674` — `handle_delete`.
7. `crates/corelink-container/src/routes/ac.rs:742` — `handle_list_refs`.
8. `crates/corelink-container/src/routes/ac.rs:517-518` — cross-tenant 403 (lookup), route-level, before storage.
8b. `crates/corelink-container/src/routes/ac.rs:580-581` — cross-tenant 403 (update), route-level, before storage.
8c. `crates/corelink-container/src/routes/ac.rs:683-684` — cross-tenant 403 (delete), route-level, before storage.
9. `crates/corelink-container/src/routes/ac.rs:168` — `AC_WRITE_CONCURRENCY_LIMIT`.
