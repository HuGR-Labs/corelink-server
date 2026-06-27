---
type: "AuthMechanism"
title: "Argon2id adapter-plane verification + scope"
description: "The container's deep PAT possession proof — bounded Argon2id, a constant-time timing-burn for unknown tokens, and the fail-closed cache scope gate."
source_files:
  - "crates/corelink-container/src/adapter_pat.rs"
  - "crates/corelink-container/src/scope.rs"
  - "crates/corelink-container/src/customer_d1.rs"
checkpoint_sha: "e3ab218a549a161083a52327b34c6a04d524f179"
provenance: "AUTHORED"
tags: ["auth", "pat", "argon2id", "scope", "dos"]
timestamp: "2026-06-26T00:00:00Z"
---

# Argon2id adapter-plane verification + scope

Argon2id is the expensive, memory-hard half of PAT verification — the step that actually proves the
caller holds the random secret itself, not merely a valid HMAC signature over it. It runs only at the bottom of the
container pipeline, after the HMAC fast-reject and the D1 row lookup have already filtered out forged and
nonexistent tokens. Because each Argon2id verify allocates tens of MiB, the verifier is wrapped in two
concurrency bounds (global + per-tenant) and an unknown-token timing-burn, so neither a token-enumeration
oracle nor a flood of valid-HMAC garbage can exploit it. Success then passes through a fail-CLOSED scope
gate before any tenant is returned.

# Role

This concept is the deep layer of [the 2-level PAT moat](/auth/pat-moat.md) and the engine behind both
the cache adapters and [the native PAT gate](/auth/hmac-fast-reject.md). The scope half here is the
fail-CLOSED **gate** — it decides what a `pat.scope` string from [the D1 PAT store](/auth/d1-pat-store.md)
is allowed to DO on a cache surface (`requires_cache_read`/`_write`). It is NOT the only owner of scope:
the requested→stored scope **vocabulary** (which capability a self-serve key may ever be MINTED with) is
owned by `customer_d1.rs`'s frozen scope map (see [the D1 PAT store](/auth/d1-pat-store.md)); `scope.rs`
gates, `customer_d1.rs` persists.

# How it works

- After HMAC + D1, the full verify runs `verify_with_hash_multi` on a `spawn_blocking` thread: it
  re-parses, constant-time matches `token_id`, re-checks HMAC, then Argon2id-verifies the secret against
  the stored PHC hash (`crates/corelink-container/src/adapter_pat.rs:602-650`).
- Concurrent Argon2id work is capped process-wide at 16 permits so a flood of valid-PAT requests cannot
  OOM-kill the shared container (`crates/corelink-container/src/adapter_pat.rs:160-170`).
- A per-tenant sub-cap (¼ of the global pool, floor 2) keeps one tenant flooding distinct PATs from
  draining all global permits and starving others — enforced at the acquire site, where the real verify
  takes `acquire_per_tenant(&row.tenant_id)` after the global permit
  (`crates/corelink-container/src/adapter_pat.rs:638-641`).
- An unknown/expired/revoked `token_id` runs a dummy Argon2id burn for timing parity so latency does not
  leak whether the token exists (`crates/corelink-container/src/adapter_pat.rs:545-598`).
- That dummy burn is routed through ONE shared synthetic bucket so a leaked-key flood across bogus
  `token_id`s cannot drain the pool via the timing-burn path — enforced where the burn path takes
  `acquire_per_tenant(UNKNOWN_TOKEN_BUCKET)` before its `spawn_blocking`
  (`crates/corelink-container/src/adapter_pat.rs:586`).
- The final scope gate fails CLOSED unless the D1 `scope` grants cache read, then surfaces the write bit
  for credential-minting callers to downscope (`crates/corelink-container/src/adapter_pat.rs:652-660`).
- The scope vocabulary lives in `scope.rs`: `requires_cache_read` / `requires_cache_write` grant by
  exact-token match (`cas:rw`/`read-write`/`admin` etc.), never substring
  (`crates/corelink-container/src/scope.rs:67-95`).

# Invariants

- Argon2id concurrency is bounded; an acquire timeout fails CLOSED as `Backend` (503), never piling on
  more 64-MiB allocations (`crates/corelink-container/src/adapter_pat.rs:616-629`).
- An empty/missing/unrecognized scope grants NOTHING — both read and write return `false`
  (`crates/corelink-container/src/scope.rs:73-95`).
- Self-serve scope classification is exact-token and fail-CLOSED: unknown grammar can never silently map
  to a privilege (`crates/corelink-container/src/scope.rs:112-149`).
- A permit is acquired only AFTER the cheap HMAC fast-reject (`crates/corelink-container/src/adapter_pat.rs:542-543`),
  so a forged token never reaches the permit acquire (`crates/corelink-container/src/adapter_pat.rs:616-629`).

# Gotchas

- The dummy timing-burn still consumes a global permit; under sustained overload the burn is skipped and
  the request fails CLOSED uniformly — the lost timing parity is acceptable because every request shares
  its fate (`crates/corelink-container/src/adapter_pat.rs:545-598`).
- `admin` is treated as a cache-rw superset by the capability checks — the executed
  `matches!(t, … | "admin")` arms in `requires_cache_read` (`crates/corelink-container/src/scope.rs:77`)
  and `requires_cache_write` (`:94`) — but admin *route* authorization is a SEPARATE internal-auth gate,
  not this scope module.
- `classify_requested_scopes` is the shared truth for the mint escalation gate and the D1 persister; the
  substring-vs-exact-token divergence it closed once let `"writes"` slip through
  (`crates/corelink-container/src/scope.rs:112-149`).
- ADR-0069 (PAT fast-hash) is NOT in tension with the `Argon2id` here: that decision is about the cheap
  HMAC **fast-reject** layer, which filters forged/unknown tokens BEFORE any expensive work. `Argon2id`
  remains the memory-hard possession proof at the bottom of this pipeline (and in the native backstop /
  adapter plane) — they are DIFFERENT layers (signature filter vs secret proof), not contradictory.

# Citations

1. `crates/corelink-container/src/adapter_pat.rs:160-170` — the global Argon2id concurrency cap (OOM guard).
2. `crates/corelink-container/src/adapter_pat.rs:638-641` — the per-tenant Argon2id sub-cap acquire site (`acquire_per_tenant(&row.tenant_id)`, the executed fairness enforcer; the const-def of the sub-cap is `:172-199`).
3. `crates/corelink-container/src/adapter_pat.rs:586` — the dummy-burn routing enforcer (`acquire_per_tenant(UNKNOWN_TOKEN_BUCKET)`, the executed shared-bucket gate; the const-def is `:220-229`).
4. `crates/corelink-container/src/adapter_pat.rs:545-598` — the None-row constant-time Argon2id timing-burn.
5. `crates/corelink-container/src/adapter_pat.rs:602-650` — the Argon2id possession verify on a blocking thread.
6. `crates/corelink-container/src/adapter_pat.rs:616-629` — permit-acquire timeout → fail-CLOSED `Backend`.
7. `crates/corelink-container/src/adapter_pat.rs:652-660` — the fail-CLOSED scope gate + write-bit surfacing.
8. `crates/corelink-container/src/scope.rs:73-95` — fail-CLOSED: empty/missing scope grants nothing (`requires_cache_read`/`_write`).
9. `crates/corelink-container/src/scope.rs:67-95` — exact-token `requires_cache_read` / `requires_cache_write`.
10. `crates/corelink-container/src/scope.rs:112-149` — `classify_requested_scopes`: the fail-CLOSED scope-classification GATE (shared by the mint escalation gate + the D1 persister); the requested→stored scope VOCABULARY is `crates/corelink-container/src/customer_d1.rs:311-324` (see [the D1 PAT store](/auth/d1-pat-store.md)) — `scope.rs` gates, `customer_d1.rs` persists.
