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
checkpoint_sha: "86e439a821d3c407cc94f4b30ba2b7a7c563d958"
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
- The per-blob order verify → R2 → D1 is the correctness guarantee against orphan classes, but it is NOT enforced by `CasWriteOrchestrator` on the live native path. `CasWriteOrchestrator` has zero production callers (only `corelink-reapi`'s own gRPC handlers + prop tests); the cited `crates/corelink-reapi/src/lib.rs:56-61` is an anti-pattern rustdoc, NOT a live enforcement seam. The live native CAS write enforcer is the `CasWriteHandler` trait object — `handle_write` (`crates/corelink-container/src/routes/cas.rs:917`) and the batch path (`crates/corelink-container/src/routes/cas.rs:1083`) call `state.write.write(req)`, and content-verify + R2 PUT + D1 commit (plus the `AccountingCasHandler` byte-accounting decorator) all happen INSIDE that single `state.write` chokepoint that every CAS write surface (native / Bazel / OCI / adapters) shares. `CasWriteOrchestrator` is the designed REAPI seam, not the load-bearing native guarantee.
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
14. `crates/corelink-container/src/routes/cas.rs:917` + `crates/corelink-container/src/routes/cas.rs:1083` — the LIVE native CAS write enforcer: `handle_write` / batch call `state.write.write(req)`, a `CasWriteHandler` trait object where content-verify + R2 PUT + D1 commit happen (the real verify→R2→D1 guarantee on the native path).
10. `crates/corelink-reapi/src/lib.rs:71` — `#![forbid(unsafe_code)]`.
11. `crates/corelink-reapi/src/lib.rs:123-128` — the `CasWriteOrchestrator` / outcome exports.
12. `crates/corelink-hash/src/verified_body.rs:33-44` — `VerifiedBody::new`: compute-then-`verify_constant_time`, `Err(HashMismatch)` on mismatch.
13. `crates/corelink-hash/src/digest.rs:78-79` — `verify_constant_time` (`ct_eq`) constant-time digest integrity compare.
