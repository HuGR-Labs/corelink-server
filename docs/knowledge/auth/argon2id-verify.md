---
type: "AuthMechanism"
title: "Argon2id adapter-plane verification + scope"
description: "The container verifies PAT possession after the HMAC and D1 checks, bounds expensive Argon2id work, and applies a fail-closed scope gate."
source_files:
  - "crates/corelink-container/src/adapter_pat_verifier.rs"
  - "crates/corelink-container/src/adapter_pat_verifier/part-02.rs"
  - "crates/corelink-container/src/adapter_pat_crypto.rs"
  - "crates/corelink-container/src/adapter_pat_gate.rs"
  - "crates/corelink-container/src/scope.rs"
source_blobs:
  - "crates/corelink-container/src/adapter_pat_verifier.rs@ae08520ae93bba5a6ac84d138cacb87c764d2194"
  - "crates/corelink-container/src/adapter_pat_verifier/part-02.rs@b59252c1e5d8ecd2b3abf6fdf91e52e816d8bf8f"
  - "crates/corelink-container/src/adapter_pat_crypto.rs@e0915648eb64b271e4a7b40a0510ce0e03ef17a6"
  - "crates/corelink-container/src/adapter_pat_gate.rs@9dddcc493a51f29485ce204427f88ee4ec6b044a"
  - "crates/corelink-container/src/scope.rs@bb38593c90351cd25e9955615f76e8ba77c10c4f"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["auth", "pat", "argon2id", "scope", "dos"]
timestamp: "2026-06-26T00:00:00Z"
---

# Argon2id adapter-plane verification + scope

The adapter verifier proves PAT possession with Argon2id only after the constant-time HMAC check and D1 lookup. A missing, expired, or revoked row takes a dummy Argon2id path before returning the same invalid-token result, which avoids a token-existence timing signal. The verifier is wired once for the adapter plane; its pipeline and error mapping are in the current `verify_capability_full` implementation (`crates/corelink-container/src/adapter_pat_verifier.rs:31-95`; `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-475`).

Argon2id work is bounded by a global semaphore and a per-tenant sub-cap. The unknown-token timing burn uses a shared non-blocking bucket before taking a global permit, so a shed burn does not occupy that permit. Any shed maps to the same `Backend("pat verifier overloaded")` result on both the row-found and row-missing paths (`crates/corelink-container/src/adapter_pat_gate.rs:21-101`; `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-475`).

The memo stores only a digest of a proven secret match, keyed with the plaintext, token id, stored hash, scope, and find-only bit. A separate shared-future flight coalesces concurrent misses; it is retired on completion and does not cache an authorization decision (`crates/corelink-container/src/adapter_pat_crypto.rs:33-450`; `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-475`).

After possession is proved, the verifier rejects find-only PATs before checking the read grant. Scope grants use exact tokens; empty or unrecognized scopes grant nothing, and read implies the narrower find-missing capability (`crates/corelink-container/src/adapter_pat_verifier/part-02.rs:260-475`; `crates/corelink-container/src/scope.rs:67-110`).

# Invariants

- HMAC rejection and the expiry-filtered D1 lookup precede Argon2id; no forged token consumes an Argon2id permit (`crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-80`).
- Global and per-tenant concurrency remain bounded. A shed has the same backend error on both token-existence arms (`crates/corelink-container/src/adapter_pat_gate.rs:21-101`; `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-475`).
- Only secret-match proof is memoized; each request still passes the HMAC and row checks, then the fail-closed scope gate (`crates/corelink-container/src/adapter_pat_crypto.rs:96-310`; `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-475`).

# Citations

1. `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:13-475` — `verify_capability_full`: HMAC fast-reject, D1 row lookup, dummy burn, memo and flight, and row-found Argon2id verification.
2. `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:260-475` — final scope gate; rejects find-only before the read grant and returns the write bit only after authorization.
3. `crates/corelink-container/src/adapter_pat_gate.rs:21-101` — global permit, per-tenant cap, shared unknown-token bucket, and wait bound.
4. `crates/corelink-container/src/adapter_pat_crypto.rs:33-450` — bounded secret-match memo and completion-retired shared-future coalescer.
5. `crates/corelink-container/src/scope.rs:67-110` — exact-token cache grants and read/find-missing relation.
