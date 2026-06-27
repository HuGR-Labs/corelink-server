---
type: "RequestFlow"
title: "PAT verification gauntlet"
description: "The container-plane PAT possession pipeline: a SHA-256 verify-cache, then HMAC fast-reject -> D1 lookup -> Argon2id -> scope, all bounded against exhaustion."
source_files:
  - "crates/corelink-container/src/native_pat_gate.rs"
  - "crates/corelink-container/src/adapter_pat.rs"
  - "crates/corelink-container/src/scope.rs"
  - "crates/corelink-pat/src/verify.rs"
  - "crates/corelink-pat/src/sig.rs"
checkpoint_sha: "09f92d5193f5a529e18c8f65c3b5a0c57bb430a1"
provenance: "AUTHORED"
tags: ["flows", "auth", "pat", "argon2id", "security", "hot-path"]
timestamp: "2026-06-26T00:00:00Z"
---

# PAT verification gauntlet

The Worker edge proves PAT possession with a cheap HMAC only — fast, but a leaked `PAT_SIGNING_KEY` would let an attacker forge a valid-looking PAT for ANY tenant. The container closes that gap by re-running the FULL possession check on the native data plane: it verifies the random per-token secret (stored only as an Argon2id hash in D1) and that the PAT's D1 tenant matches the claimed tenant. The gauntlet is ordered cheapest-to-most-expensive and is bounded at every CPU-heavy stage so a flood of garbage tokens cannot exhaust the box. This flow spans the native gate's cache + single-flight wrapper (`native_pat_gate.rs`), the shared verification pipeline (`adapter_pat.rs`), and the scope decision (`scope.rs`). It is the gate the [CAS write flow](/flows/cas-write.md) and the [introspection / runners-fabric flow](/flows/introspection-fabric.md) both invoke.

# Role

This is the data-plane possession gate and the second of CoreLink's 2-level PAT moat. Layer 1 is the Worker's HMAC fast-reject; layer 2 is this gauntlet, which adds the Argon2id secret proof + tenant-binding the HMAC layer skips — so a forged or cross-tenant token is rejected at the container even if it satisfied the edge.

# How it works

1. Entry: `NativePatGate::verify(tenant, bearer)` strips the `Bearer ` prefix, rejects an empty token 401, and computes a SHA-256 fingerprint used as the cache key (never the plaintext) (`crates/corelink-container/src/native_pat_gate.rs:156-165`).
2. Verify-cache fast path: a non-expired cached entry returns immediately, skipping Argon2id — but the cached tenant must still equal the claimed tenant or it is rejected like a miss (`crates/corelink-container/src/native_pat_gate.rs:167-176`).
3. Single-flight on miss: concurrent misses for the same fingerprint coalesce onto ONE verification via a fixed shard array, with a double-checked cache read after the lock, so a same-PAT burst cannot stampede D1 / the Argon2id pool into 503s (`crates/corelink-container/src/native_pat_gate.rs:182-192`).
4. The full pipeline runs through `PatVerifier::verify` -> `verify_capability` (`crates/corelink-container/src/native_pat_gate.rs:196-196`; `crates/corelink-container/src/adapter_pat.rs:731-735`; `crates/corelink-container/src/adapter_pat.rs:534-537`).
5. Stage 1 — HMAC fast-reject (pre-D1): the `verify_capability` call site rejects a forged token as `InvalidPat` with NO D1 round-trip (`crates/corelink-container/src/adapter_pat.rs:538-543`), but that line DELEGATES — and so does its callee: `verify_hmac_only_multi` parses the plaintext then itself CALLS `verify_hmac_sig_multi` (`crates/corelink-pat/src/verify.rs:119-125`). The check is PERFORMED one hop deeper, at the in-repo LEAF: the constant-time fold `matched |= expected.ct_eq(sig_bytes)` over the signing-key overlap set, fail-CLOSED `InvalidPat` on no match (`crates/corelink-pat/src/sig.rs:108-117`). (Contract §3 authoring rule: ground on the in-repo LEAF that performs the check — the `ct_eq` fold — not the delegating wrappers above it.)
6. Stage 2 — D1 lookup by the non-secret `token_id`, expiry-filtered in SQL; an unknown/expired/revoked id runs a bounded dummy Argon2id burn for timing parity, then returns `InvalidPat` (`crates/corelink-container/src/adapter_pat.rs:545-599`).
7. Stage 3 — Argon2id: the secret segment is verified against the stored PHC hash on a blocking thread, gated by a global concurrency permit AND a per-tenant sub-permit (consistent global->per-tenant order); an acquire-timeout fails closed as `Backend` (`crates/corelink-container/src/adapter_pat.rs:602-650`).
8. Stage 4 — scope gate (fail-closed): no cache-read capability -> `InvalidPat`; otherwise the read/write split is surfaced — the executed scope allowlists are `matches!(t, "cas:rw" | "cas:r" | "cas:w" | "read-write" | "read-only" | "admin")` (read) and `matches!(t, "cas:rw" | "cas:w" | "read-write" | "admin")` (write) (`crates/corelink-container/src/adapter_pat.rs:652-660`; `crates/corelink-container/src/scope.rs:77`; `crates/corelink-container/src/scope.rs:94`).
9. Tenant binding: on a genuine PAT the gate caches `fp -> tenant`, then returns Ok ONLY if the resolved tenant equals the claimed tenant; a real PAT for tenant A against tenant B's path is a uniform 401 (`crates/corelink-container/src/native_pat_gate.rs:196-205`).
10. Cache write: a verified entry is inserted with a short TTL so the next request in the burst skips Argon2id, and expired entries are evicted on read to bound the map (`crates/corelink-container/src/native_pat_gate.rs:218-244`).

# Invariants

- A token that fails the HMAC layer NEVER reaches Argon2id — the rejection is the in-repo LEAF `ct_eq` fold that PERFORMS the constant-time compare and returns `InvalidPat` on no match (`crates/corelink-pat/src/sig.rs:108-117`), reached via `verify_hmac_only_multi` (`crates/corelink-pat/src/verify.rs:119-125`) from the pre-D1, pre-permit call site (`crates/corelink-container/src/adapter_pat.rs:538-543`).
- The D1 row lookup runs BEFORE the expensive Argon2id, so a valid-HMAC token for a nonexistent/expired/revoked `token_id` is decided cheaply and only hits the bounded dummy burn (`crates/corelink-container/src/adapter_pat.rs:545-599`).
- Argon2id concurrency is double-bounded (global permit + per-tenant sub-permit) so neither a global flood nor one tenant flooding distinct PATs can drain the pool; saturation fails closed 503, never serves (`crates/corelink-container/src/adapter_pat.rs:602-650`).
- The scope gate is fail-closed: an empty/absent scope grants nothing — the executed `tokens(scope).any(|t| matches!(t, …))` allowlist returns `false` on no matching token (`crates/corelink-container/src/adapter_pat.rs:652-660`; `crates/corelink-container/src/scope.rs:77`).
- Possession is bound to the CLAIMED tenant — a genuine PAT for a different tenant is rejected with the SAME uniform 401 as a forgery (no oracle) (`crates/corelink-container/src/native_pat_gate.rs:196-205`; `crates/corelink-container/src/native_pat_gate.rs:247-251`).
- The cache key is a SHA-256 fingerprint, never the plaintext, so the in-memory map cannot leak a usable secret (`crates/corelink-container/src/native_pat_gate.rs:253-257`).

# Gotchas

- A cache hit returns Ok WITHOUT re-consulting D1, so a PAT revoked mid-TTL keeps native access until the entry expires; the TTL is deliberately short (5s) to bound that window rather than pay a per-request D1 read (`crates/corelink-container/src/native_pat_gate.rs:57-70`).
- Every rejection condition (forged, expired, missing, wrong-tenant) collapses to a single 401 "invalid PAT"; a D1/backend fault is a distinct 503 — so on the wire you cannot tell WHY a PAT was rejected, only that it was.
- In dev/CI the gate is absent (`None`) and skipped; in prod a `None` is a fatal boot condition enforced in `main.rs`, not a silent downgrade.

# Citations

1. `crates/corelink-container/src/native_pat_gate.rs:57-70` — verify-cache TTL rationale (revocation-window bound).
2. `crates/corelink-container/src/native_pat_gate.rs:156-205` — `verify`: Bearer-strip, fingerprint, cache fast path, single-flight, full pipeline, tenant binding.
3. `crates/corelink-container/src/native_pat_gate.rs:218-257` — cache get/put eviction and the SHA-256 fingerprint helper.
4. `crates/corelink-container/src/native_pat_gate.rs:247-251` — uniform 401 for every PAT-rejection condition (no oracle).
5. `crates/corelink-container/src/adapter_pat.rs:534-660` — `verify_capability`: HMAC fast-reject -> D1 lookup (+ bounded dummy burn) -> bounded Argon2id -> scope gate (the HMAC line at `:538-543` delegates to the callee below).
5b. `crates/corelink-pat/src/verify.rs:119-125` — `verify_hmac_only_multi` (parse + CALL `verify_hmac_sig_multi` over the overlap key set): the intermediate call-chain hop — itself a wrapper, NOT the leaf — between the adapter call site and the sig.rs enforcer.
5c. `crates/corelink-pat/src/sig.rs:108-117` — the in-repo LEAF enforcer: the constant-time fold `matched |= expected.ct_eq(sig_bytes)` over the overlap key set + the fail-CLOSED `InvalidPat` on no match — the line that ACTUALLY PERFORMS the Stage-1 fast-reject (`ct_eq` = `subtle::ConstantTimeEq`, external-crate call, so this is the deepest line in our own code; contract §3 authoring rule).
6. `crates/corelink-container/src/adapter_pat.rs:731-735` — `verify`: thin wrapper returning the owning tenant id.
7. `crates/corelink-container/src/scope.rs:77` — the executed `requires_cache_read` allowlist `matches!(t, "cas:rw" | "cas:r" | "cas:w" | "read-write" | "read-only" | "admin")` (fail-closed read capability; the fn-sig is `:73`).
8. `crates/corelink-container/src/scope.rs:94` — the executed `requires_cache_write` allowlist `matches!(t, "cas:rw" | "cas:w" | "read-write" | "admin")` (the write-capability split; the fn-sig is `:93`).
</content>
