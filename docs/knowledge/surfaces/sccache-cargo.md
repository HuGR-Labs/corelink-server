---
type: "CacheSurface"
title: "sccache / cargo (WebDAV) surface"
description: "The sccache HTTP build-cache surface mounted at /cargo/<tenant>/<key>, with nest_service path bridging, WebDAV MKCOL/PROPFIND/DELETE support for the real sccache/opendal client, and F27 two-layer write enforcement."
source_files:
  - "crates/corelink-container/src/routes/cargo.rs"
checkpoint_sha: "77db914cce647aa983819a6b9955c9315f7d5a11"
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

The real `sccache` binary talks WebDAV via opendal, which needs more than `GET`/`PUT`/`HEAD`: it also
issues `MKCOL` (create parent dir), `PROPFIND` (stat) and `DELETE` (write-check cleanup). axum's
`MethodRouter` cannot route a non-standard method (`MKCOL`/`PROPFIND`) or an unregistered one
(`DELETE`), so all three are handled directly in `cargo_gate` — the single place all WebDAV-compat lives.

# Role
It bridges the Worker's 3-segment wire path `/cargo/<tenant>/<key>` onto the adapter's own
single-segment `/:key` router and layers a gate that enforces per-operation cache scope and, for
writes, the F27 two-layer check. Artifacts flow through a 2-level moat (key→content_hash→blob),
namespaced per tenant.

# How it works
1. The router builds the per-tenant moat store and mounts the adapter via `nest_service("/cargo", …)`
   with the gate layered on (`crates/corelink-container/src/routes/cargo.rs:232-264`).
2. `nest_service` (not `nest`) is required so the adapter's `/:key` route is preserved rather than
   flattened, which would reject the 3-segment wire path (`crates/corelink-container/src/routes/cargo.rs:23-26`; `crates/corelink-container/src/routes/cargo.rs:264`).
3. The tenant is derived from the PAT via the shared `PatVerifier`, wrapped by `resolver_from_verifier`;
   the path `<tenant>` is never trusted for storage (`crates/corelink-container/src/routes/cargo.rs:188`; `crates/corelink-container/src/routes/cargo.rs:29-35`).
4. `cargo_gate` short-circuits a WebDAV `MKCOL` (opendal's parent-"directory" create issued before a
   sharded `PUT`) as a success no-op — `201 CREATED` under a cache-write scope, else `403` — because the
   cargo store is a FLAT content-addressed KV with implicit directories (`crates/corelink-container/src/routes/cargo.rs:307-313`).
5. `cargo_gate` short-circuits a WebDAV `PROPFIND` (opendal's stat) — gated on cache-read, it synthesizes
   a `207 Multi-Status` from the per-tenant moat lookup, carrying the stored blob's real byte length as
   `getcontentlength` on a hit, or `404` on a miss (opendal then proceeds to write); a trailing-slash
   collection path returns a minimal `207` (`crates/corelink-container/src/routes/cargo.rs:324-340`;
   `crates/corelink-container/src/routes/cargo.rs:525-558`; `crates/corelink-container/src/routes/cargo.rs:641-658`).
6. `cargo_gate` short-circuits a WebDAV `DELETE` (opendal's write-check cleanup) — gated on cache-write AND
   the PAT's D1 `can_write` bit (F27, keyed by the PAT-resolved tenant), it removes the per-tenant
   key→content-hash map row and returns `204` (idempotent even if absent); the CAS blob is left for GC
   (`crates/corelink-container/src/routes/cargo.rs:350-363`; `crates/corelink-container/src/routes/cargo.rs:566-606`).
7. It then enforces per-operation scope for the adapter methods: PUT requires cache-write, GET/HEAD require
   cache-read, any other method fails closed 403 (`crates/corelink-container/src/routes/cargo.rs:366-378`).
8. For PUT, the F27 second layer requires the PAT's D1-verified `can_write` bit from the SAME single
   verification (no redundant verify) — the executed gate checks the Worker-set scope header
   (`scope_ok`) and then `resolve_with_capability(...).can_write` (`crates/corelink-container/src/routes/cargo.rs:392-459`).

# Invariants
- Tenant identity comes from the PAT, re-verified against D1; the path `<tenant>` is NEVER trusted for storage — for reads/PROPFIND and for PUT/DELETE alike (`crates/corelink-container/src/routes/cargo.rs:27-35`; `crates/corelink-container/src/routes/cargo.rs:577-583`).
- A write must pass BOTH the scope header AND the PAT-derived `can_write` bit (F27), closing the single-header-trust gap — enforced for PUT in `cargo_gate` and mirrored for the WebDAV `DELETE` (`crates/corelink-container/src/routes/cargo.rs:392-459`; `crates/corelink-container/src/routes/cargo.rs:577-583`).
- Per-operation scope is enforced before the adapter runs: PUT→write, GET/HEAD→read; PROPFIND is a read (cache-read), DELETE is a write (cache-write) (`crates/corelink-container/src/routes/cargo.rs:366-378`; `crates/corelink-container/src/routes/cargo.rs:325-326`; `crates/corelink-container/src/routes/cargo.rs:351-352`).
- The WebDAV control methods are handled in the gate, not routed to the adapter: `MKCOL`/`PROPFIND`/`DELETE` short-circuit `cargo_gate` because axum's `MethodRouter` cannot route them (they would 405) (`crates/corelink-container/src/routes/cargo.rs:307-313`; `crates/corelink-container/src/routes/cargo.rs:324-340`; `crates/corelink-container/src/routes/cargo.rs:350-363`).
- A PROPFIND on an existing key reports the stored blob's real byte length in `getcontentlength`; an absent key is `404` (opendal treats it as not-found and writes) (`crates/corelink-container/src/routes/cargo.rs:550-556`; `crates/corelink-container/src/routes/cargo.rs:641-658`).
- A DELETE removes only the url→content-hash map row (the CAS blob is left for GC, as it may be shared by dedup); it is idempotent — `204` even for an absent key (`crates/corelink-container/src/routes/cargo.rs:599-605`).
- An unmapped HTTP method is denied at the gate rather than assumed safe (fail-closed) (`crates/corelink-container/src/routes/cargo.rs:374-374`).

# Gotchas
- `nest` would flatten the adapter's `/:key` into a 2-segment matcher (`/cargo/:key`) and reject the
  3-segment `/cargo/<tenant>/<key>` before the gate ever runs — `nest_service` is load-bearing, not a
  style choice.
- There is exactly ONE PAT verification per request (via the resolver port); the F27 `can_write` bit
  comes from that same verification, so the two-layer guarantee does not cost a second Argon2id.
- The WebDAV handlers extract owned inputs (`path`, bearer) from the request BEFORE the first `.await`
  so the gate future stays `Send`; holding a `&Request` (whose body is not `Sync`) across an await would
  make `from_fn` reject the layer (`crates/corelink-container/src/routes/cargo.rs:329-340`).

# Citations
1. `crates/corelink-container/src/routes/cargo.rs:232-264` — the router: moat store + `nest_service` mount + gate layer.
2. `crates/corelink-container/src/routes/cargo.rs:264` — `nest_service("/cargo", …)` mount.
3. `crates/corelink-container/src/routes/cargo.rs:23-26` — why `nest_service` not `nest` (path-shape bridging).
4. `crates/corelink-container/src/routes/cargo.rs:188` — `resolver_from_verifier` (shared `PatVerifier`).
5. `crates/corelink-container/src/routes/cargo.rs:29-35` — PAT-derived tenant; path never trusted.
6. `crates/corelink-container/src/routes/cargo.rs:27-35` — tenant + scope trust model.
7. `crates/corelink-container/src/routes/cargo.rs:366-378` — `cargo_gate` per-operation scope mapping (PUT→write, GET/HEAD→read) + 403.
8. `crates/corelink-container/src/routes/cargo.rs:374-374` — fail-closed unmapped method (`_ => false`).
9. `crates/corelink-container/src/routes/cargo.rs:392-459` — `cargo_gate` F27 two-layer write enforcement (scope header + `resolve_with_capability` `can_write`), executed.
10. `crates/corelink-container/src/routes/cargo.rs:307-313` — `cargo_gate` WebDAV `MKCOL` success no-op (gated on cache-write).
11. `crates/corelink-container/src/routes/cargo.rs:324-340` — `cargo_gate` WebDAV `PROPFIND` short-circuit (cache-read gated), the opendal stat.
12. `crates/corelink-container/src/routes/cargo.rs:525-558` — `handle_propfind`: moat lookup → `207` (present) / `404` (absent) / collection.
13. `crates/corelink-container/src/routes/cargo.rs:641-658` — `PROPFIND_LAST_MODIFIED` + `propfind_file_response`: the `207 Multi-Status` XML with `getcontentlength` **and `getlastmodified`** (opendal's WebDAV stat deserializer treats `getlastmodified` as REQUIRED — a 207 without it fails `missing field getlastmodified` → sccache flags storage ReadOnly and never writes; a stable epoch httpdate is used since CAS objects are immutable).
14. `crates/corelink-container/src/routes/cargo.rs:350-363` — `cargo_gate` WebDAV `DELETE` short-circuit (cache-write gated).
15. `crates/corelink-container/src/routes/cargo.rs:566-606` — `handle_delete`: F27 `can_write` + PAT-resolved tenant → moat map-row removal → `204`.
