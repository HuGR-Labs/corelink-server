---
type: "CrateCluster"
title: "Auth/PAT crate cluster"
description: "The identity primitives — the canonical PAT format with its HMAC fast-fail + Argon2id possession layers, the Clerk JWT/JWKS adapter, and the aggregated auth-context surface that fronts both."
source_files:
  - "crates/corelink-auth/src/lib.rs"
  - "crates/corelink-pat/src/lib.rs"
  - "crates/corelink-pat/src/verify.rs"
  - "crates/corelink-pat/src/sig.rs"
  - "crates/corelink-pat/src/argon.rs"
  - "crates/corelink-pat/src/types.rs"
  - "crates/corelink-clerk/src/lib.rs"
  - "crates/corelink-clerk/src/adapter.rs"
checkpoint_sha: "04a7eccfdbe5733a9059ab12518f6ee571db0248"
provenance: "AUTHORED"
tags: ["crates", "auth", "pat", "clerk", "jwt", "security"]
timestamp: "2026-06-26T00:00:00Z"
---

# Auth/PAT crate cluster

Every request into CoreLink is authenticated by one of two credentials — a machine Personal Access Token (cache clients, CI) or a human Clerk session JWT (admin-ui, dashboard) — and this cluster owns the cryptographic primitives for both. They are grouped because they share one boundary contract: resolve an opaque credential to a trusted `TenantContext`/principal cheaply enough to survive a flood of garbage, and never leak via timing whether a given token *exists*. `corelink-pat` defines the PAT wire format and its two-layer (HMAC → Argon2id) verify; `corelink-clerk` validates RS256 JWTs against a cached JWKS; `corelink-auth` is the aggregator that re-exports both plus the WebAuthn, D1 RLS schema, and tenant-path enforcer behind one canonical import.

# Role

The cluster is the trust spine feeding the [2-level PAT moat](/auth/pat-moat.md) and the [PAT verification gauntlet](/flows/pat-gauntlet.md). It produces the server-trusted tenant identity that every downstream isolation check keys on, and it is the only place credential secrets (signing keys, JWT keys, PAT secret bytes) are handled in cleartext.

# How it works

- `corelink-pat` defines the canonical PAT plaintext `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` and a four-step verify pipeline: parse → constant-time `token_id` match → `verify_hmac_sig_multi` (rejects unsigned spam in ≤100µs) → `verify_argon2id` possession proof — the executed orchestrator (`crates/corelink-pat/src/verify.rs:70-105`), the HMAC fast-fail step (`crates/corelink-pat/src/sig.rs:93-110`).
- The cold path (parse fail / sig mismatch / row absent) must call `dummy_verify_for_constant_time` — which runs a real Argon2id verify against a fixed dummy PHC — so end-to-end latency cannot be used as an existence oracle (`crates/corelink-pat/src/argon.rs:217-256`, with `verify_argon2id` at `crates/corelink-pat/src/argon.rs:127-134`).
- `corelink-clerk` validates a Clerk JWT by decoding the header for `kid`, KV-cached JWKS lookup with a single lazy refresh on miss, then explicit `Validation::new(Algorithm::RS256)` so `alg=none`/HS-confusion is rejected at the decoder boundary (`crates/corelink-clerk/src/adapter.rs:299`, `crates/corelink-clerk/src/adapter.rs:350`), then exact `iss`/`aud` allowlist match (`crates/corelink-clerk/src/adapter.rs:406-414`).
- `corelink-auth` is an Option-A aggregator re-exporting the 6 auth primitives (`clerk`, `clerk_cf`, `pat`, `schema`, `webauthn`, `tenant_path`) at canonical `corelink_auth::*` submodule paths with behaviour preserved 1:1 (`crates/corelink-auth/src/lib.rs:1-15`, `crates/corelink-auth/src/lib.rs:136-141`).

# Invariants

- PAT secret material is unloggable: `PatPlaintext` has no `Display`/`Serialize`/secret-revealing `Debug` and zeroizes on drop (`crates/corelink-pat/src/types.rs:41-89`); `PatSigningKey` is `ZeroizeOnDrop`, redacts in `Debug`, and rejects keys < 32 bytes with `SigningKeyTooShort` (`crates/corelink-pat/src/types.rs:219-241`).
- The HMAC fast-fail layer runs before any Argon2id work, bounding the cost a forged token can impose (`crates/corelink-pat/src/verify.rs:96`, `crates/corelink-pat/src/sig.rs:93-110`).
- The Clerk adapter accepts RS256 only — no HS256, no `alg=none`, no allowlist relaxation, ≤120s leeway (`crates/corelink-clerk/src/adapter.rs:299`).
- The aggregator never redefines types or weakens charter guarantees — RLS-default-on, constant-time PAT compare, and `SecretString` are preserved by reference, not re-implemented (`crates/corelink-auth/src/lib.rs:76-91`).

# Gotchas

- Argon2id at OWASP-2024 cost exceeds a CF Worker CPU budget under wasm32, so the deployed Argon2id verify runs only in the host/container plane; the Worker edge does the cheap HMAC fast-fail. See [the Argon2id verify](/auth/argon2id-verify.md).
- `corelink-clerk`'s `jwt-adapter` feature pulls `ring`, which does not build for `wasm32`; the trait-surface modules stay wasm-clean so `corelink-clerk-cf` can compile the CF Worker bindings.
- A container CAS 401 means bad HMAC OR no live D1 row — the Argon2id step is only reached once a row exists; it is not necessarily a wrong password.

# Citations

1. `crates/corelink-auth/src/lib.rs:1-15` — the single-import aggregator over the 6 auth primitives.
2. `crates/corelink-auth/src/lib.rs:76-91` — charter guarantees (RLS, constant-time, `SecretString`) preserved by reference.
3. `crates/corelink-auth/src/lib.rs:136-141` — the canonical `pub mod` map (clerk/pat/schema/webauthn/tenant_path).
3b. `crates/corelink-pat/src/lib.rs:96-103` — the `corelink-pat` crate `pub mod` map (argon/sig/types/verify/format/mint/scopes).
3c. `crates/corelink-clerk/src/lib.rs:114-133` — the `corelink-clerk` crate `pub mod` map (adapter/config/jwks/principal).
4. `crates/corelink-pat/src/verify.rs:70-105` — `verify_with_hash_multi`: the executed 4-step verify pipeline (parse → constant-time token_id → HMAC fast-fail → Argon2id).
5. `crates/corelink-pat/src/sig.rs:93-110` — `verify_hmac_sig_multi`: the constant-time HMAC fast-fail rejecting unsigned spam pre-Argon2id.
6. `crates/corelink-pat/src/argon.rs:217-256` — `dummy_verify_for_constant_time`: the constant-time pad (real Argon2id over a fixed dummy PHC) against the existence oracle.
7. `crates/corelink-pat/src/types.rs:41-89` — `PatPlaintext` secret-handling newtype (no Display/Serialize/Debug; zeroize on drop).
8. `crates/corelink-pat/src/types.rs:219-241` — `PatSigningKey`: `ZeroizeOnDrop`, `Debug`-redacted, rejects keys < 32 bytes (`SigningKeyTooShort`).
9. `crates/corelink-pat/src/argon.rs:127-134` — `verify_argon2id`: the Argon2id PHC possession proof.
10. `crates/corelink-clerk/src/adapter.rs:299` — RS256-only alg enforcement at the header-decode boundary (`AlgNotAllowed`).
11. `crates/corelink-clerk/src/adapter.rs:350` — `Validation::new(Algorithm::RS256)`: RS256 pinned at the signature decoder.
12. `crates/corelink-clerk/src/adapter.rs:406-414` — exact `iss`/`aud` allowlist match enforcement.
