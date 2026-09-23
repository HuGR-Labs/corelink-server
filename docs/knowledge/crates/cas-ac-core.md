---
type: "CrateCluster"
title: "CAS/AC core crate cluster"
description: "The content-addressable storage primitives — digest integrity, the chunk/manifest/R2 multipart machinery, and the REAPI orchestration that ties verify → R2 → D1 into one correctness order."
source_files:
  - "crates/corelink-cas/src/lib.rs"
  - "crates/corelink-hash/src/lib.rs"
  - "crates/corelink-hash/src/verified_body.rs"
  - "crates/corelink-hash/src/digest.rs"
  - "crates/corelink-reapi/src/lib.rs"
  - "crates/corelink-container/src/routes/cas.rs"
  - "crates/corelink-container/src/routes/cas/foundation_core.rs"
  - "crates/corelink-container/src/routes/cas/single_handlers.rs"
  - "crates/corelink-container/src/routes/cas/batch_write.rs"
  - "crates/corelink-container/src/routes/cas/foundation_state.rs"
source_blobs:
  - "crates/corelink-container/src/routes/cas/single_handlers.rs@f4259f925d39cff5d70ae6fb8fc5996f2109dba2"
  - "crates/corelink-container/src/routes/cas/foundation_state.rs@8dcd20ecd23850e331763d5ccfcf4d0260489f0e"
  - "crates/corelink-cas/src/lib.rs@5eef35d4a9c8ad713604a6df5c7493d88b27248e"
  - "crates/corelink-hash/src/lib.rs@403f7e2e4ef8c7bb30a47e7d7eee3ce94a785730"
  - "crates/corelink-hash/src/verified_body.rs@d387eca974b5314dad46b6e92b1623cb8b3e10d5"
  - "crates/corelink-hash/src/digest.rs@40aea50c8127ada98133e3719171b65f047f2bcd"
  - "crates/corelink-reapi/src/lib.rs@a4e6596b8741aded884e05b445df1b13edf1419b"
  - "crates/corelink-container/src/routes/cas.rs@41c65ce80c66084ef424ac4189b644bbe8461f95"
  - "crates/corelink-container/src/routes/cas/foundation_core.rs@63671990ccb9f0e9e6cba8804457737b42c18120"
  - "crates/corelink-container/src/routes/cas/batch_write.rs@3561b0e3188b5ff05ff5998fdad3f33ee5f6ad78"
checkpoint_sha: "648ecdccd229bdb5154b86843053c28b9cce9d36"
provenance: "AUTHORED"
tags: ["crates", "cas", "ac", "integrity", "blake3", "core"]
timestamp: "2026-06-26T00:00:00Z"

---
# CAS/AC core crate cluster

This is the bottom of CoreLink's value stack: the crates that turn a blob into a content address and refuse to ever store one that does not match its claimed digest. They exist as a cluster because the CAS hot path was historically fragmented across ten separate crates (chunker, dedup, edge, eviction, lru-tracker, manifest, multipart-schema, meta, handler, r2-multipart), and the integrity guarantee (`INV-CAS-INTEGRITY`) has to hold across all of them as one type-driven boundary, not ten independent ones. `corelink-cas` is the aggregator that re-exports the absorbed primitives under one canonical import; `corelink-hash` owns the type that makes a forged write unrepresentable; `corelink-reapi` is the REAPI/gRPC orchestration that pins the write order for that surface (its `CasWriteOrchestrator` is a designed seam, NOT a live enforcer on the native HTTP path — the native path's verify→R2→D1 guarantee lives in the container's `CasWriteHandler`).

# Role

The cluster sits below every cache surface ([native CAS](/surfaces/native-cas.md), [AC](/surfaces/action-cache.md), [Bazel REAPI](/surfaces/bazel-reapi.md)) and above R2/D1 storage. It owns the single rule that makes a content-addressed cache trustworthy — a body is only writable once it has proved it hashes to its address (the `VerifiedBody` type, enforced on EVERY surface) — and the per-blob order (verify → R2 PUT → D1 commit) whose reordering would produce orphan classes. That order is enforced per-surface: by `CasWriteOrchestrator` in the REAPI/gRPC handlers, and by the `CasWriteHandler` trait object on the live native HTTP path.

# How it works

- `corelink-cas` is an Option-A aggregator: it `pub use`s/`pub mod`s the ten CAS-tier primitives under canonical `corelink_cas::*` paths so every consumer imports from one target while behaviour is preserved 1:1 (`crates/corelink-cas/src/lib.rs:1-19`, `crates/corelink-cas/src/lib.rs:86-95`).
- The integrity type lives in `corelink-hash`: `VerifiedBody::new` is the *only* constructor and it **always** runs the BLAKE3 verify, so no caller can write a body to storage without first proving it matches its claimed digest (`crates/corelink-hash/src/lib.rs:5-13`).
- `Digest` is a newtype with a private field, constructible only via the server-side `compute` or the untrusted-input `from_hex` parser — raw-byte construction outside the crate is forbidden by design (`crates/corelink-hash/src/lib.rs:6-13`, `crates/corelink-hash/src/lib.rs:56-59`).
- `corelink-reapi` is the REAPI/gRPC surface that chains the prior primitives into one pipeline: `PatValidator → TenantCtx → VerifiedBody::new → ScopedR2Writer.put_verified → MetaStore.commit_put`, exported through `CasWriteOrchestrator` (`crates/corelink-reapi/src/lib.rs:1-27`, `crates/corelink-reapi/src/lib.rs:123-128`). STATUS: `CasWriteOrchestrator` is a designed seam used by `corelink-reapi`'s OWN gRPC handlers (`handler/per_blob.rs`, `handler/bytestream.rs`) + its prop tests; it has **zero consumers outside the crate**. The live native CAS HTTP transport does NOT route through it (see the invariant below).

# Invariants

- Every body is verified before storage: the only path to a `VerifiedBody` runs the hash check — `VerifiedBody::new` computes the digest then `verify_constant_time`, returning `Err(HashMismatch)` on mismatch (`crates/corelink-hash/src/verified_body.rs:33-44`; `crates/corelink-hash/src/lib.rs:9-13`).
- Security-sensitive digest comparison uses constant-time `verify_constant_time`, not short-circuiting `PartialEq`, so timing cannot leak (`crates/corelink-hash/src/digest.rs:78-79`; `crates/corelink-hash/src/lib.rs:40-45`).
- The per-blob order verify → R2 → D1 is the correctness guarantee against orphan classes, but it is NOT enforced by `CasWriteOrchestrator` on the live native path. `CasWriteOrchestrator` has zero production callers (only `corelink-reapi`'s own gRPC handlers + prop tests); the cited `crates/corelink-reapi/src/lib.rs:56-61` is an anti-pattern rustdoc, NOT a live enforcement seam. The live native CAS write enforcer is the `CasWriteHandler` trait object — `handle_write` (`crates/corelink-container/src/routes/cas/single_handlers.rs:133-214`) and the batch path (`crates/corelink-container/src/routes/cas/batch_write.rs:162-175`) call `state.write.write(req)`, and content-verify + R2 PUT + D1 commit (plus the `AccountingCasHandler` byte-accounting decorator) all happen INSIDE that single `state.write` chokepoint that every CAS write surface (native / Bazel / OCI / adapters) shares. Upstream of that chokepoint the native write path also re-derives the PAT's D1-stored `can_write` capability at the container (`pat_gate_reject_write`) so a read-only token can never reach the verify→R2→D1 write (`crates/corelink-container/src/routes/cas/single_handlers.rs:133-214`). `CasWriteOrchestrator` is the designed REAPI seam, not the load-bearing native guarantee.
- The whole cluster is memory-safe by construction: `#![forbid(unsafe_code)]` at each crate root (`crates/corelink-cas/src/lib.rs:83`, `crates/corelink-hash/src/lib.rs:49`, `crates/corelink-reapi/src/lib.rs:71`).

# Gotchas

- The aggregator (`corelink-cas`) is a re-export façade, not the source of truth — absorbed submodules own their own correctness tests; the smoke tests here only prove the canonical paths resolve at compile time.
- `PartialEq` on `Digest` is acceptable for non-adversarial uses (D1 indexing, set membership) but must NOT be used where an attacker observes timing; the call-site rustdoc states which mode applies.
- `corelink-reapi`'s pure-logic modules compile to `wasm32` without the `host-server` feature; the gRPC glue is feature-gated, so the same orchestration runs in a Worker or a container.

# Citations

1. `crates/corelink-cas/src/lib.rs:1-19` — the single-import aggregator surface over the 10 absorbed CAS primitives.
2. `crates/corelink-cas/src/lib.rs:83` — `#![forbid(unsafe_code)]` at the cluster root.
3. `crates/corelink-cas/src/lib.rs:86-95` — the canonical `pub mod` map (chunker/dedup/manifest/meta/handler/r2_multipart…).
4. `crates/corelink-hash/src/lib.rs:5-13` — `Digest`/`VerifiedBody` type-driven integrity; verifying constructors only.
5. `crates/corelink-hash/src/lib.rs:40-45` — constant-time digest compare vs short-circuiting `PartialEq`.
6. `crates/corelink-hash/src/lib.rs:49` — `#![forbid(unsafe_code)]`.
7. `crates/corelink-hash/src/lib.rs:56-59` — the small public surface (`Digest`, `VerifiedBody`, errors).
8. `crates/corelink-reapi/src/lib.rs:1-27` — the verify → R2 → D1 orchestration pipeline diagram.
9. `crates/corelink-reapi/src/lib.rs:56-61` — the "never bypass `CasWriteOrchestrator`" anti-pattern RUSTDOC (a doc-comment / design guideline scoped to REAPI transports, NOT a live enforcement seam — the orchestrator has zero production callers).
14. `crates/corelink-container/src/routes/cas/single_handlers.rs:198-212` + `crates/corelink-container/src/routes/cas/batch_write.rs:162-175` — the LIVE native CAS write enforcer: `handle_write` / batch call `state.write.write(req)`, a `CasWriteHandler` trait object where content-verify + R2 PUT + D1 commit happen (the real verify→R2→D1 guarantee on the native path).
15. `crates/corelink-container/src/routes/cas/single_handlers.rs:133-214` — `pat_gate_reject_write` re-derives the PAT's D1 `can_write` at the container BEFORE the write chokepoint, so a read-only PAT never reaches the verify→R2→D1 write.
10. `crates/corelink-reapi/src/lib.rs:71` — `#![forbid(unsafe_code)]`.
11. `crates/corelink-reapi/src/lib.rs:123-128` — the `CasWriteOrchestrator` / outcome exports.
12. `crates/corelink-hash/src/verified_body.rs:33-44` — `VerifiedBody::new`: compute-then-`verify_constant_time`, `Err(HashMismatch)` on mismatch.
13. `crates/corelink-hash/src/digest.rs:78-79` — `verify_constant_time` (`ct_eq`) constant-time digest integrity compare.
16. `crates/corelink-container/src/routes/cas.rs:45-51` — executable route module includes.
17. `crates/corelink-container/src/routes/cas/foundation_state.rs:1-20` — the live CAS route state owns separate read/write/delete/list handler objects.
18. `crates/corelink-container/src/routes/cas/foundation_core.rs:110-119` — batch request count and payload ceilings.
19. `crates/corelink-container/src/routes/cas/single_handlers.rs:133-212` — single native write gates, commit, and status.
20. `crates/corelink-container/src/routes/cas/batch_write.rs:162-175` — batch writes delegate verified per-object requests into the same write handler.
