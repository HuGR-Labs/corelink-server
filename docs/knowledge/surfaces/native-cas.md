---
type: "CacheSurface"
title: "Native CAS surface"
description: "CoreLink's first-party content-addressable storage surface — the GET/PUT/DELETE/list + bulk-batch CAS routes every other surface ultimately stores into."
source_files:
  - "crates/corelink-container/src/routes/cas.rs"
  - "crates/corelink-container/src/routes/turbo_v8.rs"
  - "crates/corelink-container/src/native_pat_gate.rs"
checkpoint_sha: "41d84e271568cb47df664806fa3dc9798c134249"
provenance: "AUTHORED"
tags: ["surfaces", "cas", "cache", "hot-path"]
timestamp: "2026-06-26T00:00:00Z"
---

# Native CAS surface

The native CAS surface is CoreLink's first-party content-addressable cache: a blob is named by its
own BLAKE3/SHA-256 digest, so a write is idempotent and a read resolves by digest. That
content-addressing is the *storage* shape — but a request here is NOT a bare key fetch: every
billable op first runs a possession + control-plane gauntlet (the native `Argon2id` PAT gate, the
`$`-ceiling quota charge, the 410-tombstone gate) BEFORE any R2 I/O
(`crates/corelink-container/src/native_pat_gate.rs:196`; `crates/corelink-container/src/routes/cas.rs:654-687`). It is the
foundational data plane — the REAPI (Bazel), sccache/cargo and `_public` package surfaces are alternate
front-doors that share the cloned CAS trait objects (the same R2 blobs this surface exposes). Turborepo
is the exception: it uses a SEPARATE R2 bucket via its own `R2KvStore`, so there is no cross-surface
blob visibility with it (`crates/corelink-container/src/routes/turbo_v8.rs:978-980`). It sits in the
Rust container plane behind the Worker→DO auth hop, so by the time a request
reaches a handler here the tenant is already authenticated; the surface's job is the cheap
canonical-key, scope, possession and quota gates BEFORE any storage I/O. It shares the
digest-validation helper with the [Action Cache surface](/surfaces/action-cache.md).

# Role
It serves the single-object read/write/delete/list contract (`/v1/cas/:tenant/:hash`) plus the bulk
length-framed batch routes (`/batch`, `/batch-read`, `/batch-exists`) that amortize the per-object D1
round-trip on cold-start uploads. Tenant isolation is keyed on the authenticated tenant, never the
path segment.

# How it works
1. The router mounts read/write/delete on `CAS_READ_ROUTE` and adds the three bulk routes as static
   siblings ranked above the `:hash` wildcard by matchit (`crates/corelink-container/src/routes/cas.rs:515-531`).
2. A read runs, in order, the native PAT possession gate (`Argon2id`, warm-cache-skipped), the
   per-tenant `$`-ceiling quota charge (402 over-ceiling), and the 410-Gone tombstone gate — each
   BEFORE the R2 GET — then the digest-keyed lookup via `handle_read`
   (`crates/corelink-container/src/routes/cas.rs:625`; gate order `crates/corelink-container/src/routes/cas.rs:654-687`).
   These control-plane gates are OPTIONAL `CasHandlerState` fields (`pat_gate`/`quota`/`tombstones`) —
   wired in prod, `None` in dev/CI (`crates/corelink-container/src/routes/cas.rs:142-154`).
3. A write rejects a path/auth tenant mismatch with 403, then validates the digest, then the write
   scope, then the native PAT possession gate, all before storage (`crates/corelink-container/src/routes/cas.rs:713-747`).
4. A per-tenant in-flight reservation runs as an extractor BEFORE the body is buffered, returning 429
   over the concurrency cap (`crates/corelink-container/src/routes/cas.rs:285-289`).
5. Bulk uploads are split into a newline-framed manifest + concatenated payload at the first blank
   line by `split_manifest` (`crates/corelink-container/src/routes/cas.rs:582-595`); a wrong/absent
   content-type is rejected 415 before parse — the `content_type_is` predicate
   (`crates/corelink-container/src/routes/cas.rs:538-545`) at the batch-handler call-site
   (`crates/corelink-container/src/routes/cas.rs:847-848`).

# Invariants
- A non-canonical `:hash` is rejected 400 BEFORE it derives an R2 key (`crates/corelink-container/src/routes/cas.rs:732`).
- A path `:tenant` that differs from the authenticated tenant is denied 403 BEFORE storage (`crates/corelink-container/src/routes/cas.rs:728`).
- A write requires a `cas:rw` (write-capable) scope; a read-only token is rejected 403 (`crates/corelink-container/src/routes/cas.rs:738-739`).
- A batch is capped at `BATCH_MAX_OBJECTS` objects and `BATCH_MAX_BYTES` of payload, over-cap → 413 (`crates/corelink-container/src/routes/cas.rs:115`; `crates/corelink-container/src/routes/cas.rs:121`; `crates/corelink-container/src/routes/cas.rs:881`).
- Per-tenant concurrent uploads are bounded; over the limit returns 429 before buffering (`crates/corelink-container/src/routes/cas.rs:285-289`).

# Gotchas
- The native CAS path proves PAT possession with the `NativePatGate` backstop (`pat_gate_reject`,
  red-team finding #4) layered on top of the scope check — and that gate runs the FULL HMAC
  fast-reject → D1 liveness → **`Argon2id`** possession proof → scope gauntlet, NOT a bare HMAC check
  (`crates/corelink-container/src/native_pat_gate.rs:196`). `Argon2id` is therefore NOT "adapter-plane
  only": it runs here too, just MEMOIZED — a warm fingerprint-cache hit skips the `Argon2id` round, so
  the cost lands roughly once per token rather than per request
  (`crates/corelink-container/src/native_pat_gate.rs:170`). A CAS 401 means a forged/expired/wrong-tenant
  PAT or no D1 row; a 503 is a verifier-backend (D1) fault, fail-CLOSED.
- The bulk routes are real route SIBLINGS of the `:hash` wildcard, not captures of it; the framing
  contract (manifest, blank line, payload) is strict and a missing blank-line terminator fails the
  whole request 400.

# Citations
1. `crates/corelink-container/src/routes/cas.rs:515-531` — the router: single-object + three bulk batch routes.
2. `crates/corelink-container/src/routes/cas.rs:625` — `handle_read`, the digest-keyed read.
3. `crates/corelink-container/src/routes/cas.rs:713-747` — `handle_write`: cross-tenant 403, canonical-digest, scope, native PAT gate order.
4. `crates/corelink-container/src/routes/cas.rs:728` — the cross-tenant 403.
5. `crates/corelink-container/src/routes/cas.rs:732` — canonical-digest reject before storage.
6. `crates/corelink-container/src/routes/cas.rs:738-739` — the write-scope (`cas:rw`) gate.
6b. `crates/corelink-container/src/routes/cas.rs:741-743` — the native PAT possession gate (after scope, before storage).
7. `crates/corelink-container/src/routes/cas.rs:285-289` — pre-body per-tenant concurrency 429.
8. `crates/corelink-container/src/routes/cas.rs:582-595` — `split_manifest` length-framed bulk parser.
9. `crates/corelink-container/src/routes/cas.rs:538-545` — `content_type_is` predicate for the 415 gate.
9b. `crates/corelink-container/src/routes/cas.rs:847-848` — batch-handler call-site that returns 415 on wrong/absent content-type.
10. `crates/corelink-container/src/routes/cas.rs:115` — `BATCH_MAX_OBJECTS` cap.
11. `crates/corelink-container/src/routes/cas.rs:121` — `BATCH_MAX_BYTES` cap.
12. `crates/corelink-container/src/routes/cas.rs:881` — over-cap 413 on bulk write.
13. `crates/corelink-container/src/routes/turbo_v8.rs:978-980` — Turborepo's SEPARATE `R2KvStore` (own bucket) — NOT the shared CAS blobs, so no cross-surface visibility with Turbo.
14. `crates/corelink-container/src/native_pat_gate.rs:196` — `NativePatGate::verify` delegates to the full HMAC→D1→`Argon2id`→scope gauntlet (the native backstop, finding #4).
15. `crates/corelink-container/src/native_pat_gate.rs:170` — warm fingerprint-cache fast-path that skips the `Argon2id` round on a hit.
16. `crates/corelink-container/src/routes/cas.rs:654-687` — the read-path gate order: native PAT gate → `$`-ceiling quota → 410 tombstone, all BEFORE the R2 GET.
17. `crates/corelink-container/src/routes/cas.rs:142-154` — the OPTIONAL `CasHandlerState` control-plane fields (`tombstones`/`quota`/`pat_gate`); `None` in dev/CI, wired in prod.
