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
  - "crates/corelink-container/src/routes/cas/foundation_state.rs"
  - "crates/corelink-container/src/routes/cas/single_handlers.rs"
  - "crates/corelink-container/src/routes/cas/single_setup.rs"
  - "crates/corelink-container/src/routes/cas/batch_write.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs"
  - "crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs"
source_blobs:
  - "crates/corelink-cas/src/lib.rs@5eef35d4a9c8ad713604a6df5c7493d88b27248e"
  - "crates/corelink-hash/src/lib.rs@403f7e2e4ef8c7bb30a47e7d7eee3ce94a785730"
  - "crates/corelink-hash/src/verified_body.rs@d387eca974b5314dad46b6e92b1623cb8b3e10d5"
  - "crates/corelink-hash/src/digest.rs@40aea50c8127ada98133e3719171b65f047f2bcd"
  - "crates/corelink-reapi/src/lib.rs@a4e6596b8741aded884e05b445df1b13edf1419b"
  - "crates/corelink-container/src/routes/cas/foundation_state.rs@8dcd20ecd23850e331763d5ccfcf4d0260489f0e"
  - "crates/corelink-container/src/routes/cas/single_handlers.rs@f4259f925d39cff5d70ae6fb8fc5996f2109dba2"
  - "crates/corelink-container/src/routes/cas/single_setup.rs@27e4090d053420fa0bfe07ee655d9c6ce6573439"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs@83438b03872e8204c62a32ad09137dd9b3da52fb"
  - "crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs@3e62bda2ed171001cf74b084d36bd61e6c1d0d39"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
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
- The per-blob order verify → R2 → D1 is the correctness guarantee against orphan classes, but it is NOT enforced by `CasWriteOrchestrator` on the live native path. The native route validates the caller, digest, scope, PAT write capability, and quota before calling `state.write.write(req)` (`crates/corelink-container/src/routes/cas/single_handlers.rs:145-198`); `R2CasHandler` verifies bytes against the digest before storage (`crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs:50-58`), performs the R2 PUT (`:298-309`), then commits the storage metadata fence (`:324-331`). The route's write trait object is the seam (`crates/corelink-container/src/routes/cas/foundation_state.rs:4-14`), with accounting applied by `AccountingCasHandler` before the inner write (`crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs:295-300`). The native PAT write gate re-derives D1 `can_write` before reaching storage (`crates/corelink-container/src/routes/cas/single_setup.rs:310-328`). `CasWriteOrchestrator` remains the designed REAPI seam, not the load-bearing native guarantee.
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
14. `crates/corelink-container/src/routes/cas/single_handlers.rs:189-198` — native route invokes the shared write trait after assembling the authenticated request.
15. `crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs:50-58,298-309,324-331` — storage write verifies content before R2 PUT and metadata-fence commit.
16. `crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs:295-300` — accounting decorator implements the write trait and delegates through its effect-aware path.
17. `crates/corelink-container/src/routes/cas/single_setup.rs:310-328` — write-capability helper independently calls `NativePatGate::verify_write`.
10. `crates/corelink-reapi/src/lib.rs:71` — `#![forbid(unsafe_code)]`.
11. `crates/corelink-reapi/src/lib.rs:123-128` — the `CasWriteOrchestrator` / outcome exports.
12. `crates/corelink-hash/src/verified_body.rs:33-44` — `VerifiedBody::new`: compute-then-`verify_constant_time`, `Err(HashMismatch)` on mismatch.
13. `crates/corelink-hash/src/digest.rs:78-79` — `verify_constant_time` (`ct_eq`) constant-time digest integrity compare.
18. `crates/corelink-container/src/routes/cas/foundation_state.rs:4-14` — declared CAS route state trait-object seam.
19. `crates/corelink-container/src/routes/cas/batch_write.rs:171-175` — batch writes use the same write-handler trait object as the single-object route.
