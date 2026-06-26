---
type: "CrateCluster"
title: "Auth/PAT crate cluster"
description: "The identity primitives — the canonical PAT format with its HMAC fast-fail + Argon2id possession layers, the Clerk JWT/JWKS adapter, and the aggregated auth-context surface that fronts both."
source_files:
  - "crates/corelink-auth/src/lib.rs"
  - "crates/corelink-pat/src/lib.rs"
  - "crates/corelink-pat/src/sig.rs"
  - "crates/corelink-pat/src/argon.rs"
  - "crates/corelink-clerk/src/lib.rs"
checkpoint_sha: "5571b910292cbe3d53cbf46d7e0f120dbef877e2"
provenance: "AUTHORED"
tags: ["crates", "auth", "pat", "clerk", "jwt", "security"]
timestamp: "2026-06-26T00:00:00Z"
---

# Auth/PAT crate cluster

Every request into CoreLink is authenticated by one of two credentials — a machine Personal Access Token (cache clients, CI) or a human Clerk session JWT (admin-ui, dashboard) — and this cluster owns the cryptographic primitives for both. They are grouped because they share one boundary contract: resolve an opaque credential to a trusted `TenantContext`/principal cheaply enough to survive a flood of garbage, and never leak via timing whether a given token *exists*. `corelink-pat` defines the PAT wire format and its two-layer (HMAC → Argon2id) verify; `corelink-clerk` validates RS256 JWTs against a cached JWKS; `corelink-auth` is the aggregator that re-exports both plus the WebAuthn, D1 RLS schema, and tenant-path enforcer behind one canonical import.

# Role

The cluster is the trust spine feeding the [2-level PAT moat](/auth/pat-moat.md) and the [PAT verification gauntlet](/flows/pat-gauntlet.md). It produces the server-trusted tenant identity that every downstream isolation check keys on, and it is the only place credential secrets (signing keys, JWT keys, PAT secret bytes) are handled in cleartext.

# How it works

- `corelink-pat` defines the canonical PAT plaintext `corelink_<env>_<token_id>.<random_secret>.<hmac_sig>` and a four-step verify pipeline: parse → `verify_hmac_sig` (rejects unsigned spam in ≤100µs) → DB row lookup by `token_id` → `verify_argon2id` possession proof. The format/pipeline shape lives in the barrel (`crates/corelink-pat/src/lib.rs:11-43`); the HMAC fast-fail enforcer is `verify_hmac_sig` (`crates/corelink-pat/src/sig.rs:64-104`) and the possession proof is `verify_argon2id`, which additionally rejects an OWASP-underprovisioned stored hash (`crates/corelink-pat/src/argon.rs:127-170`).
- The cold path (parse fail / sig mismatch / row absent) must call `dummy_verify_for_constant_time` so end-to-end latency cannot be used as an existence oracle — the pad is the same Argon2id work as the warm path (`crates/corelink-pat/src/argon.rs:217-256`).
- `corelink-clerk` validates a Clerk JWT by decoding the header for `kid`, KV-cached JWKS lookup with a single lazy refresh on miss, then explicit `Validation::new(Algorithm::RS256)` so `alg=none`/HS-confusion is rejected at the decoder boundary, then exact `iss`/`aud` allowlist match (`crates/corelink-clerk/src/lib.rs:54-65`, `crates/corelink-clerk/src/lib.rs:72-79`).
- `corelink-auth` is an Option-A aggregator re-exporting the 6 auth primitives (`clerk`, `clerk_cf`, `pat`, `schema`, `webauthn`, `tenant_path`) at canonical `corelink_auth::*` submodule paths with behaviour preserved 1:1 (`crates/corelink-auth/src/lib.rs:1-15`, `crates/corelink-auth/src/lib.rs:136-141`).

# Invariants

- PAT secret material is unloggable: `PatPlaintext` has no `Display`/`Serialize`/secret-revealing `Debug`, `PatSigningKey` redacts + zeroizes on drop and rejects keys < 32 bytes (`crates/corelink-pat/src/lib.rs:45-56`).
- The HMAC fast-fail layer runs before any DB hit, bounding the cost a forged token can impose; the truncated-MAC compare is constant-time, evaluates the whole overlap key set without early-return, and an empty key set fails closed (`crates/corelink-pat/src/sig.rs:64-104`).
- The Clerk adapter accepts RS256 only — no HS256, no `alg=none`, no allowlist relaxation, HTTPS-only JWKS, ≤120s skew (`crates/corelink-clerk/src/lib.rs:72-79`).
- The aggregator never redefines types or weakens charter guarantees — RLS-default-on, constant-time PAT compare, and `SecretString` are preserved by reference, not re-implemented (`crates/corelink-auth/src/lib.rs:76-91`).

# Gotchas

- Argon2id at OWASP-2024 cost exceeds a CF Worker CPU budget under wasm32, so the deployed Argon2id verify runs only in the host/container plane; the Worker edge does the cheap HMAC fast-fail. See [the Argon2id verify](/auth/argon2id-verify.md).
- `corelink-clerk`'s `jwt-adapter` feature pulls `ring`, which does not build for `wasm32`; the trait-surface modules stay wasm-clean so `corelink-clerk-cf` can compile the CF Worker bindings.
- A container CAS 401 means bad HMAC OR no live D1 row — the Argon2id step is only reached once a row exists; it is not necessarily a wrong password.

# Citations

1. `crates/corelink-auth/src/lib.rs:1-15` — the single-import aggregator over the 6 auth primitives.
2. `crates/corelink-auth/src/lib.rs:76-91` — charter guarantees (RLS, constant-time, `SecretString`) preserved by reference.
3. `crates/corelink-auth/src/lib.rs:136-141` — the canonical `pub mod` map (clerk/pat/schema/webauthn/tenant_path).
4. `crates/corelink-pat/src/lib.rs:11-43` — canonical PAT format + the 4-step verify pipeline (HMAC fast-fail → Argon2id).
5. `crates/corelink-pat/src/sig.rs:64-104` — `verify_hmac_sig`: HMAC fast-fail pre-DB, constant-time overlap-set compare, fail-closed on empty key set.
6. `crates/corelink-pat/src/argon.rs:127-170` — `verify_argon2id` possession proof + OWASP-2024 cost-floor rejection of a downgraded hash.
7. `crates/corelink-pat/src/lib.rs:45-56` — secret-handling newtype invariants (no Display/Serialize; zeroize; ≥32B key).
8. `crates/corelink-pat/src/argon.rs:217-256` — `dummy_verify_for_constant_time`: the cold-path Argon2id pad against the existence oracle.
9. `crates/corelink-clerk/src/lib.rs:54-65` — the JWT validate path (kid → JWKS cache → RS256 → iss/aud).
10. `crates/corelink-clerk/src/lib.rs:72-79` — anti-scope: RS256-only, no `alg=none`, HTTPS-only, ≤120s skew.
