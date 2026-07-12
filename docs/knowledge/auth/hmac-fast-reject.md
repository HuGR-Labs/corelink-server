---
type: "AuthMechanism"
title: "Native HMAC fast-reject PAT gate"
description: "The container-side defense-in-depth gate that re-proves PAT possession on the native data plane, cheaply caching verified tokens."
source_files:
  - "crates/corelink-container/src/native_pat_gate.rs"
  - "crates/corelink-container/src/main.rs"
checkpoint_sha: "d2a1f643464c2bd4636cd7fb62f17d3843c621ee"
provenance: "AUTHORED"
tags: ["auth", "pat", "security", "hot-path", "native-plane"]
timestamp: "2026-06-26T00:00:00Z"
---

# Native HMAC fast-reject PAT gate

The native cache surfaces (CAS / AC / Bazel REAPI / Turbo) trust the Worker-injected
`x-corelink-tenant-id` header for isolation, and the Worker's hot path proves possession with an HMAC
check only (it skips the CPU-heavy Argon2id). That means a leaked `PAT_SIGNING_KEY` alone is enough to
forge a valid-looking PAT for ANY tenant (`crates/corelink-container/src/native_pat_gate.rs:1-22`). The
`NativePatGate` closes that chain at the container: it re-runs the SAME full Option-B verification the
cache adapters use — HMAC fast-reject, then D1, then Argon2id of the random secret against the stored
hash — and additionally binds the result to the CLAIMED tenant. It is an additional, independent layer;
the HMAC gate is not removed.

# Role

This gate is the data-plane half of the 2-level PAT moat — see [the 2-level PAT moat](/auth/pat-moat.md).
It wraps the shared [`PatVerifier`](/auth/argon2id-verify.md) with a short-TTL verification cache so
Argon2id (tens of ms) runs at most once per token per window, keeping the billable hot path fast while
still possession-checking. It is constructed from the same env as the verifier and is `Option`-gated:
absent in dev/CI, mandatory in prod (`crates/corelink-container/src/native_pat_gate.rs:281-283`).

# How it works

- `verify(tenant, bearer)` and its write-gate sibling `verify_write(tenant, bearer)` are thin wrappers
  over the shared `verify_inner` flow, which strips an optional `Bearer ` prefix, rejects an empty token,
  then computes a SHA-256 fingerprint used as the cache key
  (`crates/corelink-container/src/native_pat_gate.rs:159-231`). `verify_write` additionally requires the
  PAT's D1-derived `can_write` bit (403 on a read-only PAT), so a `cas:r` token cannot write via Bazel/Turbo
  even if the Worker scope header were wrong (deep-audit B/F-1); the verify cache carries `can_write` and
  `verify` routes through `verify_capability` (behaviour-identical for reads).
- A non-expired cache hit skips Argon2id but still requires the cached tenant to equal the claimed
  tenant, else uniform 401 (`crates/corelink-container/src/native_pat_gate.rs:167-176`).
- A miss takes a per-fingerprint single-flight lock so a concurrent burst of the SAME PAT coalesces onto
  ONE verification instead of stampeding the verifier and 503-ing
  (`crates/corelink-container/src/native_pat_gate.rs:178-192`).
- The full verify (HMAC fast-reject → D1 lookup → Argon2id → scope gate) runs via the shared verifier;
  on success the resolved tenant is cached `fp → (tenant, now+TTL)`
  (`crates/corelink-container/src/native_pat_gate.rs:194-216`).
- A genuine PAT for tenant A presented against tenant B's path is a cross-tenant forgery and is rejected
  401 (`crates/corelink-container/src/native_pat_gate.rs:199-205`).
- The TTL is 5 seconds, bounding how long a revoked PAT keeps native access; a cache hit adds no D1
  round-trip (`crates/corelink-container/src/native_pat_gate.rs:57-70`).

# Invariants

- Every rejection (forged, wrong tenant, expired, missing) collapses to a uniform 401 with no oracle
  (`crates/corelink-container/src/native_pat_gate.rs:247-251`).
- A verifier backend (D1) fault fails CLOSED with 503, never serving an un-possession-checked billable op
  (`crates/corelink-container/src/native_pat_gate.rs:208-214`).
- The cache key is a SHA-256 fingerprint, never the plaintext, so the in-memory map cannot leak a usable
  secret (`crates/corelink-container/src/native_pat_gate.rs:257-261`).
- The cache is tenant-bound: a cached entry for tenant A is rejected when presented for tenant B
  (`crates/corelink-container/src/native_pat_gate.rs:167-176`).

# Gotchas

- Revocation latency on the native plane is bounded by the 5s TTL, not immediate — a cache hit does NOT
  re-consult D1 (`crates/corelink-container/src/native_pat_gate.rs:57-70`). The Worker + adapter D1
  lookup remain the authoritative revocation surfaces.
- In prod a `None` from the builder is a SILENT security downgrade; the container's boot path treats it
  as FATAL when prod is detected — the teeth live in `main.rs`, not this builder
  (`crates/corelink-container/src/native_pat_gate.rs:268-283`; the prod-fatal backstop is
  `should_fatal_on_missing_gate` at `crates/corelink-container/src/main.rs:77` wired at
  `crates/corelink-container/src/main.rs:300`).
- The single-flight shards are a FIXED 256-entry array, not a per-token map — bounded memory by
  construction (`crates/corelink-container/src/native_pat_gate.rs:72-77`).

# Citations

1. `crates/corelink-container/src/native_pat_gate.rs:1-22` — why the gate exists (leaked-signing-key forgery).
2. `crates/corelink-container/src/native_pat_gate.rs:57-70` — the 5s cache TTL bounding revocation latency.
3. `crates/corelink-container/src/native_pat_gate.rs:72-77` — fixed 256 single-flight shards (bounded memory).
4. `crates/corelink-container/src/native_pat_gate.rs:156-216` — `verify`: fingerprint, cache, single-flight, tenant bind.
5. `crates/corelink-container/src/native_pat_gate.rs:247-251` — the uniform 401 (no rejection oracle).
6. `crates/corelink-container/src/native_pat_gate.rs:257-261` — SHA-256 fingerprint cache key, never the plaintext.
7. `crates/corelink-container/src/native_pat_gate.rs:268-283` — env-gated builder; prod-fatal on a missing gate.
8. `crates/corelink-container/src/main.rs:77` — `should_fatal_on_missing_gate` (prod && !gate_present).
9. `crates/corelink-container/src/main.rs:300` — boot-path call site enforcing the prod-fatal backstop.
