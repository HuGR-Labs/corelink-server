---
type: "CacheSurface"
title: "sccache / cargo (WebDAV) surface"
description: "The sccache HTTP build-cache surface mounted at /cargo/<tenant>/<key>, with nest_service path bridging and F27 two-layer write enforcement."
source_files:
  - "crates/corelink-container/src/routes/cargo.rs"
checkpoint_sha: "84e3a88ad674702ef0f33d362bfe918e2c5d6319"
provenance: "AUTHORED"
tags: ["surfaces", "sccache", "cargo", "cache"]
timestamp: "2026-06-26T00:00:00Z"
---

# sccache / cargo (WebDAV) surface

This surface mounts the `corelink_adapter_host::cargo` adapter — an sccache HTTP storage backend
(`GET`/`PUT`/`HEAD /<key>`) — so a Rust/sccache user can point their build cache at
`/cargo/<tenant>/<key>` and share compilation artifacts. It closed a real gap: the Worker already
forwarded `/cargo/*` to the container, but the adapter was never mounted, so every forwarded request
404'd. It is the canonical example of CoreLink's adapter-host pattern: the path tenant is for DO routing
only, the real tenant is derived from the PAT, and writes are gated by both a Worker-set scope header
AND the PAT's own D1 write bit. It shares the single `PatVerifier` with the npm/pip/brew/oci adapters.

# Role
It bridges the Worker's 3-segment wire path `/cargo/<tenant>/<key>` onto the adapter's own
single-segment `/:key` router and layers a gate that enforces per-operation cache scope and, for
writes, the F27 two-layer check. Artifacts flow through a 2-level moat (key→content_hash→blob),
namespaced per tenant.

# How it works
1. The router builds the per-tenant moat store and mounts the adapter via `nest_service("/cargo", …)`
   with the gate layered on (`crates/corelink-container/src/routes/cargo.rs:211-247`).
2. `nest_service` (not `nest`) is required so the adapter's `/:key` route is preserved rather than
   flattened, which would reject the 3-segment wire path (`crates/corelink-container/src/routes/cargo.rs:23-26`; `crates/corelink-container/src/routes/cargo.rs:252`).
3. The tenant is derived from the PAT via the shared `PatVerifier`, wrapped by `resolver_from_verifier`;
   the path `<tenant>` is never trusted for storage (`crates/corelink-container/src/routes/cargo.rs:187`; `crates/corelink-container/src/routes/cargo.rs:29-35`).
4. `cargo_gate` first short-circuits a WebDAV `MKCOL` (opendal's parent-"directory" create issued
   before a sharded `PUT`) as a success no-op — `201 CREATED` under a cache-write scope, else `403` —
   because the cargo store is a FLAT content-addressed KV with implicit directories; without it the
   real `sccache` binary can never write (`crates/corelink-container/src/routes/cargo.rs:295-301`).
5. It then enforces per-operation scope: PUT requires cache-write, GET/HEAD require cache-read,
   any other method fails closed 403 (`crates/corelink-container/src/routes/cargo.rs:303-316`).
6. For PUT, the F27 second layer requires the PAT's D1-verified `can_write` bit from the SAME single
   verification (no redundant verify) — the executed gate checks the Worker-set scope header
   (`scope_ok`) and then `resolve_with_capability(...).can_write` (`crates/corelink-container/src/routes/cargo.rs:331-381`).

# Invariants
- Tenant identity comes from the PAT, re-verified against D1; the path `<tenant>` is NEVER trusted for storage (`crates/corelink-container/src/routes/cargo.rs:27-35`).
- A write must pass BOTH the scope header AND the PAT-derived `can_write` bit (F27), closing the single-header-trust gap — enforced in `cargo_gate` (`crates/corelink-container/src/routes/cargo.rs:331-381`).
- Per-operation scope is enforced before the adapter runs: PUT→write, GET/HEAD→read (`crates/corelink-container/src/routes/cargo.rs:303-316`).
- A WebDAV `MKCOL` is a success no-op still gated on cache-write scope (the flat store's directories are implicit); it short-circuits before the F27 resolver and stores no body (`crates/corelink-container/src/routes/cargo.rs:295-301`).
- An unmapped HTTP method is denied at the gate rather than assumed safe (fail-closed) (`crates/corelink-container/src/routes/cargo.rs:307-312`).

# Gotchas
- `nest` would flatten the adapter's `/:key` into a 2-segment matcher (`/cargo/:key`) and reject the
  3-segment `/cargo/<tenant>/<key>` before the gate ever runs — `nest_service` is load-bearing, not a
  style choice.
- There is exactly ONE PAT verification per request (via the resolver port); the F27 `can_write` bit
  comes from that same verification, so the two-layer guarantee does not cost a second Argon2id.

# Citations
1. `crates/corelink-container/src/routes/cargo.rs:211-247` — the router: moat store + `nest_service` mount + gate layer.
2. `crates/corelink-container/src/routes/cargo.rs:252` — `nest_service("/cargo", …)` mount.
3. `crates/corelink-container/src/routes/cargo.rs:23-26` — why `nest_service` not `nest` (path-shape bridging).
4. `crates/corelink-container/src/routes/cargo.rs:187` — `resolver_from_verifier` (shared `PatVerifier`).
5. `crates/corelink-container/src/routes/cargo.rs:29-35` — PAT-derived tenant; path never trusted.
6. `crates/corelink-container/src/routes/cargo.rs:27-35` — tenant + scope trust model.
7. `crates/corelink-container/src/routes/cargo.rs:277-316` — `cargo_gate` per-operation scope enforcement.
8. `crates/corelink-container/src/routes/cargo.rs:303-316` — method→scope mapping + 403.
9. `crates/corelink-container/src/routes/cargo.rs:307-312` — fail-closed unmapped method.
10. `crates/corelink-container/src/routes/cargo.rs:331-381` — `cargo_gate` F27 two-layer write enforcement (scope header + `resolve_with_capability` `can_write`), executed.
11. `crates/corelink-container/src/routes/cargo.rs:295-301` — `cargo_gate` WebDAV `MKCOL` success no-op (gated on cache-write), the sccache/opendal real-client fix.
