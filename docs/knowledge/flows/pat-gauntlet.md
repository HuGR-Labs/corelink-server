---
type: "RequestFlow"
title: "PAT verification gauntlet"
description: "The container-plane PAT possession pipeline: a SHA-256 verify-cache, then HMAC fast-reject -> D1 lookup -> Argon2id -> scope, all bounded against exhaustion."
source_files:
  - "crates/corelink-container/src/native_pat_gate.rs"
  - "crates/corelink-container/src/adapter_pat.rs"
  - "crates/corelink-container/src/scope.rs"
checkpoint_sha: "ae712b8cc1ea8844572d72f7400efd674725b4f5"
provenance: "AUTHORED"
tags: ["flows", "auth", "pat", "argon2id", "security", "hot-path"]
timestamp: "2026-06-26T00:00:00Z"
---

# PAT verification gauntlet

The Worker edge proves PAT possession with a cheap HMAC only — fast, but a leaked `PAT_SIGNING_KEY` would let an attacker forge a valid-looking PAT for ANY tenant. The container closes that gap by re-running the FULL possession check on the native data plane: it verifies the random per-token secret (stored only as an Argon2id hash in D1) and that the PAT's D1 tenant matches the claimed tenant. The gauntlet is ordered cheapest-to-most-expensive and is bounded at every CPU-heavy stage so a flood of garbage tokens cannot exhaust the box. This flow spans the native gate's cache + single-flight wrapper (`native_pat_gate.rs`), the shared verification pipeline (`adapter_pat.rs`), and the scope decision (`scope.rs`). It is the gate the [CAS write flow](/flows/cas-write.md) and the [introspection / runners-fabric flow](/flows/introspection-fabric.md) both invoke.

# Role

This is the data-plane possession gate and the second of CoreLink's 2-level PAT moat. Layer 1 is the Worker's HMAC fast-reject; layer 2 is this gauntlet, which adds the Argon2id secret proof + tenant-binding the HMAC layer skips — so a forged or cross-tenant token is rejected at the container even if it satisfied the edge.

The public `verify(tenant, bearer)` and `verify_write(tenant, bearer)` are thin wrappers over the shared `verify_inner` flow (`crates/corelink-container/src/native_pat_gate.rs:159-231`); `verify_write` additionally requires the PAT's D1-derived `can_write` capability (`decide` returns `403` on a genuine read-only PAT), so a `cas:r` token cannot write via Bazel/Turbo even if the Worker-set scope header were wrong — the two-layer enforcement cargo/OCI already do (deep-audit B/F-1). The verify cache carries the `can_write` bit alongside the tenant, and `verify` routes through `verify_capability` (behaviour-identical for reads, since `verify` was `verify_capability(..).map(|(t,_)| t)`).

# How it works

1. Entry: `NativePatGate::verify(tenant, bearer)` strips the `Bearer ` prefix, rejects an empty token 401, and computes a SHA-256 fingerprint used as the cache key (never the plaintext) (`crates/corelink-container/src/native_pat_gate.rs:156-165`).
2. Verify-cache fast path: a non-expired cached entry returns immediately, skipping Argon2id — but the cached tenant must still equal the claimed tenant or it is rejected like a miss (`crates/corelink-container/src/native_pat_gate.rs:167-176`).
3. Single-flight on miss: concurrent misses for the same fingerprint coalesce onto ONE verification via a fixed shard array, with a double-checked cache read after the lock, so a same-PAT burst cannot stampede D1 / the Argon2id pool into 503s (`crates/corelink-container/src/native_pat_gate.rs:182-192`).
4. The full pipeline runs through `PatVerifier::verify` -> `verify_capability` (`crates/corelink-container/src/native_pat_gate.rs:196-196`; `crates/corelink-container/src/adapter_pat.rs:1169-1173`; `crates/corelink-container/src/adapter_pat.rs:924-927`).
5. Stage 1 — HMAC fast-reject (pre-D1): the plaintext is HMAC-checked against the signing-key overlap set; a forged token is rejected as `InvalidPat` with NO D1 round-trip (`crates/corelink-container/src/adapter_pat.rs:932-933`).
6. Stage 2 — D1 lookup by the non-secret `token_id`, expiry-filtered in SQL; an unknown/expired/revoked id runs a bounded dummy Argon2id burn for timing parity, then returns `InvalidPat` (`crates/corelink-container/src/adapter_pat.rs:936-989`).
7. Stage 3 — Argon2id, MEMOISED. The verifier first consults `SecretMatchMemo` (bounded 4096, TTL 300 s) keyed by a domain-separated, length-prefixed SHA-256 over `(plaintext, token_id, stored pat_hash)`; a hit skips Argon2id and takes no permit. A cold miss single-flights on that fingerprint (256 fixed shards, double-checked after the lock) so a same-PAT burst runs ONE verify (`crates/corelink-container/src/adapter_pat.rs:1009-1019`). On the miss path the secret segment is verified against the stored PHC hash on a blocking thread, gated by a global concurrency permit AND a per-tenant sub-permit (consistent global->per-tenant order); an acquire-timeout fails closed as `Backend` (`crates/corelink-container/src/adapter_pat.rs:1030-1064`).
8. Stage 4 — scope gate (fail-closed): a find-only PAT (`row.find_only`) is rejected FIRST, before the read grant (`crates/corelink-container/src/adapter_pat.rs:1085-1087`, ADR-0071 — this adapter/OCI plane authorizes from the D1 `scope` directly with no `x-corelink-scope` header, so the find-only PAT's `read-only` base would otherwise `docker pull`); then no cache-read capability -> `InvalidPat`; otherwise the read/write split is surfaced (`crates/corelink-container/src/adapter_pat.rs:1085-1093`; `crates/corelink-container/src/scope.rs:73-73`; `crates/corelink-container/src/scope.rs:93-93`).
9. Tenant binding: on a genuine PAT the gate caches `fp -> tenant`, then returns Ok ONLY if the resolved tenant equals the claimed tenant; a real PAT for tenant A against tenant B's path is a uniform 401 (`crates/corelink-container/src/native_pat_gate.rs:196-205`).
10. Cache write: a verified entry is inserted with a short TTL so the next request in the burst skips Argon2id, and expired entries are evicted on read to bound the map (`crates/corelink-container/src/native_pat_gate.rs:218-244`).

# Invariants

- A token that fails the HMAC layer NEVER reaches Argon2id — the fast-reject is pre-D1 and pre-permit (`crates/corelink-container/src/adapter_pat.rs:932-933`).
- The D1 row lookup runs BEFORE the expensive Argon2id, so a valid-HMAC token for a nonexistent/expired/revoked `token_id` is decided cheaply and only hits the bounded dummy burn (`crates/corelink-container/src/adapter_pat.rs:936-989`).
- Argon2id concurrency is double-bounded (global permit + per-tenant sub-permit) so neither a global flood nor one tenant flooding distinct PATs can drain the pool; saturation fails closed 503, never serves (`crates/corelink-container/src/adapter_pat.rs:1030-1064`).
- The scope gate is fail-closed: a find-only PAT is rejected before the read grant (`crates/corelink-container/src/adapter_pat.rs:1085-1087`), and an empty/absent scope grants nothing (`crates/corelink-container/src/adapter_pat.rs:1088-1090`; `crates/corelink-container/src/scope.rs:73-73`).
- Possession is bound to the CLAIMED tenant — a genuine PAT for a different tenant is rejected with the SAME uniform 401 as a forgery (no oracle) (`crates/corelink-container/src/native_pat_gate.rs:196-205`; `crates/corelink-container/src/native_pat_gate.rs:247-251`).
- The cache key is a SHA-256 fingerprint, never the plaintext, so the in-memory map cannot leak a usable secret (`crates/corelink-container/src/native_pat_gate.rs:257-261`).
- The two caches in this flow are NOT the same kind of object and their TTLs are not comparable. `NativePatGate`'s verify cache memoises the DECISION and skips D1 on a hit, so its 5 s TTL is a revocation window. The verifier's `SecretMatchMemo` memoises only the immutable "plaintext matches this PHC hash" and still reads D1 every request, so revocation, expiry and scope are never cached on the adapter plane and apply on the next request (`crates/corelink-container/src/adapter_pat.rs:370-390`; `crates/corelink-container/src/adapter_pat.rs:936-989`).

# Gotchas

- A cache hit returns Ok WITHOUT re-consulting D1, so a PAT revoked mid-TTL keeps native access until the entry expires; the TTL is deliberately short (5s) to bound that window rather than pay a per-request D1 read (`crates/corelink-container/src/native_pat_gate.rs:57-70`).
- Every rejection condition (forged, expired, missing, wrong-tenant) collapses to a single 401 "invalid PAT"; a D1/backend fault is a distinct 503 — so on the wire you cannot tell WHY a PAT was rejected, only that it was.
- In dev/CI the gate is absent (`None`) and skipped; in prod a `None` is a fatal boot condition enforced in `main.rs`, not a silent downgrade.

# Citations

1. `crates/corelink-container/src/native_pat_gate.rs:57-70` — verify-cache TTL rationale (revocation-window bound).
2. `crates/corelink-container/src/native_pat_gate.rs:156-205` — `verify`: Bearer-strip, fingerprint, cache fast path, single-flight, full pipeline, tenant binding.
3. `crates/corelink-container/src/native_pat_gate.rs:218-257` — cache get/put eviction and the SHA-256 fingerprint helper.
4. `crates/corelink-container/src/native_pat_gate.rs:247-251` — uniform 401 for every PAT-rejection condition (no oracle).
5. `crates/corelink-container/src/adapter_pat.rs:924-1093` — `verify_capability`: HMAC fast-reject -> D1 lookup (+ bounded dummy burn) -> bounded Argon2id -> scope gate.
6. `crates/corelink-container/src/adapter_pat.rs:1169-1173` — `verify`: thin wrapper returning the owning tenant id.
7. `crates/corelink-container/src/scope.rs:73-73` — `requires_cache_read` (fail-closed read capability).
8. `crates/corelink-container/src/scope.rs:93-93` — `requires_cache_write` (the write-capability split).
</content>
