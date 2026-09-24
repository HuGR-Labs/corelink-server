---
type: "AuthMechanism"
title: "The 2-level PAT moat"
description: "The Worker rejects forged or inactive PATs cheaply, and the container independently proves possession before adapter access."
source_files:
  - "worker/src/index_auth_verify.ts"
  - "worker/src/lib/pat_verify_cache.ts"
  - "crates/corelink-container/src/adapter_pat_verifier/part-02.rs"
  - "crates/corelink-container/src/adapter_pat_gate.rs"
  - "crates/corelink-container/src/scope.rs"
source_blobs:
  - "worker/src/index_auth_verify.ts@2fc384e44afb315c32bc63171d5444bf73b6e59a"
  - "worker/src/lib/pat_verify_cache.ts@d68139b375700c173dadfee076ad6761b17f41bb"
  - "crates/corelink-container/src/adapter_pat_verifier/part-02.rs@b59252c1e5d8ecd2b3abf6fdf91e52e816d8bf8f"
  - "crates/corelink-container/src/adapter_pat_gate.rs@9dddcc493a51f29485ce204427f88ee4ec6b044a"
  - "crates/corelink-container/src/scope.rs@bb38593c90351cd25e9955615f76e8ba77c10c4f"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["auth", "pat", "security", "hot-path", "cache"]
timestamp: "2026-06-26T00:00:00Z"
---

# The 2-level PAT moat

The Worker checks the PAT format and HMAC, then reads the D1 row that carries tenant, expiry, revocation, and scope state. `verifyPatRowCached` may reuse positive row data through its in-memory and KV tiers; misses and read failures fall through to D1, and negative results are not cached. The Worker rejects invalid signatures and inactive rows before forwarding an authenticated tenant request (`worker/src/index_auth_verify.ts:12-289`; `worker/src/lib/pat_verify_cache.ts:225-380`).

The container adapter plane repeats possession verification. It performs its own HMAC fast-reject and D1 lookup, proves the secret with Argon2id, and applies the cache scope gate. This protects adapter routes if an edge decision is forged or a header is replayed outside its trusted path (`crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-475`; `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:428-475`; `crates/corelink-container/src/scope.rs:67-110`).

The Worker cache stores positive row data with a short in-memory TTL and a 30-second KV backstop. The container bounds Argon2id concurrency globally and per tenant; unknown tokens use a shared dummy-burn bucket so a missing-row response does not become a cheaper token-existence signal (`worker/src/lib/pat_verify_cache.ts:83-105`; `worker/src/lib/pat_verify_cache.ts:153-246`; `crates/corelink-container/src/adapter_pat_gate.rs:21-101`; `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-475`).

# Invariants

- The Worker checks the HMAC and liveness before forwarding a tenant-authenticated request; D1 remains the source of truth on a cache miss (`worker/src/index_auth_verify.ts:12-289`; `worker/src/lib/pat_verify_cache.ts:225-380`).
- The container independently verifies the PAT and rejects scopes that do not grant the requested cache capability (`crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-475`; `crates/corelink-container/src/scope.rs:67-110`).
- Only positive rows are cached, and cached row data expires on a bounded TTL (`worker/src/lib/pat_verify_cache.ts:153-246`; `worker/src/lib/pat_verify_cache.ts:287-380`).
- Unknown-token timing work is bounded and shed uniformly with known-token Argon2id work (`crates/corelink-container/src/adapter_pat_gate.rs:21-101`; `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-475`).

# Citations

1. `worker/src/index_auth_verify.ts:12-289` — Worker token parsing, HMAC validation, cached row lookup, and expiry/scope enforcement.
2. `worker/src/lib/pat_verify_cache.ts:225-380` — replica/primary row read and positive-only memory/KV/D1 cascade.
3. `worker/src/lib/pat_verify_cache.ts:83-105` — KV key and TTL contract.
4. `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-475` — container HMAC, D1 liveness, dummy burn, and Argon2id pipeline.
5. `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:428-475` — container cache scope gate.
6. `crates/corelink-container/src/adapter_pat_gate.rs:21-101` — global and per-tenant Argon2id bounds, including the unknown-token bucket.
7. `crates/corelink-container/src/scope.rs:67-110` — exact-token, fail-closed scope checks.
