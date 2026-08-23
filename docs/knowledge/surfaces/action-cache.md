---
type: "CacheSurface"
title: "Action Cache (AC) surface"
description: "The first-party Action Cache routes mapping an action digest to its cached result, with cross-tenant denial and a canonical-digest gate shared with native CAS."
source_files:
  - "crates/corelink-container/src/routes/ac.rs"
checkpoint_sha: "8af9ed65caf286d3f800e91d3f823face3aefd31"
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
   its SLO-emitting handler (`crates/corelink-container/src/routes/ac.rs:447-455`).
2. A lookup is served by `handle_lookup` (`crates/corelink-container/src/routes/ac.rs:510`); an update
   by `handle_update` (`crates/corelink-container/src/routes/ac.rs:569`).
3. The `:action_digest` segment is validated as exactly 64 lowercase-hex chars by `is_canonical_digest`
   BEFORE it derives an R2 key (`crates/corelink-container/src/routes/ac.rs:464-468`).
4. On the READ path (lookup / ref-list) the native PAT possession gate `pat_gate_reject` re-verifies the
   bearer against the claimed tenant and rejects forged/wrong-tenant tokens before storage
   (`crates/corelink-container/src/routes/ac.rs:476-487`).
5. On the WRITE path (update / delete) the gate escalates to `pat_gate_reject_write`, which independently
   re-derives the PAT's D1-stored `can_write` capability at the container via `NativePatGate::verify_write`
   (not just tenant possession) — a read-only PAT is rejected 403 even if the Worker-set scope header
   claimed write, upholding the Option-B invariant that a compromised Worker cannot grant write on its own
   (`crates/corelink-container/src/routes/ac.rs:613` update / `crates/corelink-container/src/routes/ac.rs:713` delete; helper `verify_write` at `crates/corelink-container/src/routes/ac.rs:506`).
6. Delete and per-tenant ref enumeration are served by `handle_delete` and `handle_list_refs`
   (`crates/corelink-container/src/routes/ac.rs:679`; `crates/corelink-container/src/routes/ac.rs:747`).

# Invariants
- A non-canonical action digest (not 64 lowercase-hex) is rejected 400 before storage (`crates/corelink-container/src/routes/ac.rs:464-468`).
- Cross-tenant attempts are rejected **HTTP 403** by the route-level tenant check
  (`crates/corelink-container/src/routes/ac.rs:522-523`, `:580-581`, `:683-684`), which returns
  **before** `lookup`/`update` runs — so this reject path itself writes **no** audit row. (The
  `LookupDenied`/`UpdateDenied` audit rows are emitted by the lookup/update handlers on
  authorized-but-denied paths, not by this route-level cross-tenant 403.)
- Per-tenant concurrent writes are bounded by `AC_WRITE_CONCURRENCY_LIMIT` (`crates/corelink-container/src/routes/ac.rs:173`).
- A forged or wrong-tenant bearer PAT is rejected by the read-path possession gate before any storage access (`crates/corelink-container/src/routes/ac.rs:476-487`).
- An AC update/delete requires a WRITE-capable PAT re-derived at the container (`pat_gate_reject_write` → `NativePatGate::verify_write`), not just tenant possession — a read-only PAT is rejected 403 even if the Worker-set scope header claimed write (`crates/corelink-container/src/routes/ac.rs:613`, `:713`).
- A **create-only (deny-overwrite)** runner-job cred may CREATE a new `(tenant, action_digest)` entry
  but is rejected **HTTP 409 `AC_CREATE_ONLY`** on any OVERWRITE of an existing one (anti AC-squat, the
  AC analog of the runner-job deny-DELETE). Enforced ATOMICALLY off the store's put-if-absent `durable`
  signal — a non-durable write is the conflict (`crates/corelink-container/src/routes/ac.rs:653`),
  and a divergent-body overwrite maps to the same conflict (`crates/corelink-container/src/routes/ac.rs:668`)
  rather than the generic divergent-body 409. The conflict body is built by `ac_create_only_conflict`
  (`crates/corelink-container/src/routes/ac.rs:812`).

# Gotchas
- **⚠️ The route MUST use the matchit `{name}` BRACE capture form, NOT `:name`.** The workspace is
  pinned to **axum 0.8** (`Cargo.toml`: `axum = { version = "0.8", ... }`) / matchit 0.8, where `{name}`
  is the capture and a bare `:name` is a LITERAL path segment that silently 404s every real request —
  the INVERSE of matchit 0.7's syntax. This was DEBT-029, closed on `ac.rs`/`admin.rs` by moving TO the
  brace form; the real route consts are `{tenant}` / `{action_digest}`
  (`crates/corelink-container/src/routes/ac.rs:59,91`), and a regression test
  (`list_route_constant_uses_axum_0_8_brace_syntax`) pins brace-presence + colon-absence so a future
  "fix" can't flip it back. A contributor who follows the old `:name` guidance ships a route that 404s.
- Lowercase-only canonicalization is deliberate: it prevents case-variant key collisions in the
  content-addressing space, so a mixed-case digest is rejected rather than normalized.

# Citations
1. `crates/corelink-container/src/routes/ac.rs:447-455` — the AC router (lookup/update/delete + ref list).
2. `crates/corelink-container/src/routes/ac.rs:510` — `handle_lookup`.
3. `crates/corelink-container/src/routes/ac.rs:569` — `handle_update`.
4. `crates/corelink-container/src/routes/ac.rs:464-468` — `is_canonical_digest` (64 lowercase-hex) gate.
5. `crates/corelink-container/src/routes/ac.rs:476-487` — `pat_gate_reject` native PAT possession gate (read path).
5b. `crates/corelink-container/src/routes/ac.rs:496-506` — `pat_gate_reject_write`: re-derives D1 `can_write` via `NativePatGate::verify_write` (line 506) on the update/delete write paths.
5c. `crates/corelink-container/src/routes/ac.rs:613` — `pat_gate_reject_write` call in `handle_update`.
5d. `crates/corelink-container/src/routes/ac.rs:713` — `pat_gate_reject_write` call in `handle_delete`.
6. `crates/corelink-container/src/routes/ac.rs:679` — `handle_delete`.
7. `crates/corelink-container/src/routes/ac.rs:747` — `handle_list_refs`.
8. `crates/corelink-container/src/routes/ac.rs:522-523` — cross-tenant 403 (lookup), route-level, before storage.
8b. `crates/corelink-container/src/routes/ac.rs:585-586` — cross-tenant 403 (update), route-level, before storage.
8c. `crates/corelink-container/src/routes/ac.rs:688-689` — cross-tenant 403 (delete), route-level, before storage.
9. `crates/corelink-container/src/routes/ac.rs:173` — `AC_WRITE_CONCURRENCY_LIMIT`.
