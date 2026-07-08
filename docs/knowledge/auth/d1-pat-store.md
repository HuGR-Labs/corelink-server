---
type: "AuthMechanism"
title: "D1 PAT store + existence/scope check"
description: "Where PAT rows live in D1, how they are created and revoked, and the scope mapping that gates self-serve key mint."
source_files:
  - "crates/corelink-container/src/customer_d1.rs"
  - "crates/corelink-container/src/routes/internal_pat.rs"
  - "crates/corelink-container/src/adapter_pat.rs"
  - "crates/corelink-container/src/scope.rs"
checkpoint_sha: "7f3714648744937ca2c0b89e04ecdace6272e5ed"
provenance: "AUTHORED"
tags: ["auth", "pat", "d1", "store", "scope"]
timestamp: "2026-06-26T00:00:00Z"
---

# D1 PAT store + existence/scope check

A PAT's secret is never stored — only its non-secret `token_id` (the lookup key), its tenant, its
Argon2id PHC `pat_hash`, and its `scope` string live in the D1 `pat` table. That row is the existence,
expiry, revocation, and scope authority every verification path consults: a verifier resolves a token by
`token_id` and a missing/expired/revoked row is the auth decision. Two writers populate the store — the
internal mint route (Clerk auto-provision) and the customer keys-create handler (self-serve) — and a
single revoke handler soft-deletes rows tenant-scoped (`crates/corelink-container/src/customer_d1.rs:18-22`).

# Role

This is the persistence layer behind the moat. The container verifier and the Worker hot path both read
the same `pat` row; see [the 2-level PAT moat](/auth/pat-moat.md) and
[the Argon2id verify](/auth/argon2id-verify.md) for how that row is consumed. The store also owns the
self-serve scope vocabulary: it decides which capability a requested key may carry before it is ever
written.

# How it works

- Self-serve key create mints a PAT then writes the row with `INSERT INTO pat (pat_id, tenant_id,
  pat_hash, scope, expires_ms, …, token_id, name)` — the hash and the non-secret `token_id` are
  persisted, the plaintext is returned once and never stored
  (`crates/corelink-container/src/customer_d1.rs:1238-1253`).
- The requested-scope → stored-scope map is FROZEN and fail-CLOSED: `admin` is NEVER grantable
  self-serve, anything-with-write → `read-write`, else `read-only`
  (`crates/corelink-container/src/customer_d1.rs:306-328`).
- Revoke is an idempotent, tenant-scoped soft delete: `UPDATE pat SET revoked_at_ms = ?1 WHERE pat_id =
  ?2 AND tenant_id = ?3 AND revoked_at_ms IS NULL` — a PAT owned by another tenant is simply not found
  (`crates/corelink-container/src/customer_d1.rs:1316-1318`).
- Listing is read-only and always self-tenant-scoped, mapping the stored `scope` back to a dashboard
  scopes list (`crates/corelink-container/src/customer_d1.rs:1149-1150`).
- The internal mint route is the SECOND writer's mint half: `POST /_internal/pat/mint` mints the PAT
  and returns plaintext + Argon2id hash, gated by a constant-time compare against the DEDICATED
  `CORELINK_PAT_MINT_AUTH_KEY` ONLY — with NO fallback to the shared `CORELINK_INTERNAL_AUTH_KEY`. Because
  this surface can mint ANY tenant's PAT (incl. `SCOPE_ADMIN_ALL`), the dedicated key is REQUIRED and an
  unset/blank/`< 32`-char value fails CLOSED: the route is NOT mounted and the endpoint is unavailable
  (503) rather than silently widened to the broad shared signup-worker credential — the DD-HIGH
  remediation (`crates/corelink-container/src/routes/internal_pat.rs:1-22`).
- That mint route is a PURE function — it NEVER persists the row itself; the caller (signup-worker)
  writes the returned `hash` to the D1 `pat` row and discards the plaintext after one use
  (`crates/corelink-container/src/routes/internal_pat.rs:502-659`).
- A mint failure returns an OPAQUE 503 body (`{"error":"mint_failed"}`): the real `PatError` detail
  (e.g. `SigningKeyTooShort`, entropy/hash-corruption internals) is logged SERVER-SIDE only and never
  disclosed in the response — even to a holder of the mint auth key
  (`crates/corelink-container/src/routes/internal_pat.rs:608-620`).

# Invariants

- The PAT plaintext is never logged or persisted at the mint route; the caller writes it to Clerk
  session metadata once and discards it (`crates/corelink-container/src/routes/internal_pat.rs:73-75`).
- The `admin` scope is never grantable via self-serve key creation; unrecognized tokens fail CLOSED
  (`crates/corelink-container/src/customer_d1.rs:306-328`).
- Revoke is tenant-scoped: a cross-tenant `pat_id` cannot be revoked (or even observed)
  (`crates/corelink-container/src/customer_d1.rs:1316-1318`).

# Gotchas

- The mint route returns the Argon2id `hash` in its 200 body because the signup-worker — not the
  container — is what WRITES that hash to the D1 `pat` row (the M7 follow-up to move the write into the
  container is deferred) (`crates/corelink-container/src/routes/internal_pat.rs:88-92`).
- `scope` may be NULL on legacy rows; the verifier's D1 row reader maps that to `""`
  (`crates/corelink-container/src/adapter_pat.rs:144-150`), which then fails CLOSED at the scope gate
  rather than erroring (`crates/corelink-container/src/scope.rs:73-95`).
- `last_used_at` is not tracked; the list handler always reports `None`
  (`crates/corelink-container/src/customer_d1.rs:18-20`).

# Citations

1. `crates/corelink-container/src/customer_d1.rs:18-22` — the PAT-store overview: tables, create, revoke.
2. `crates/corelink-container/src/customer_d1.rs:306-328` — `map_requested_scopes`: the FROZEN requested-scope → `pat.scope` map (admin never grantable, unrecognized tokens fail CLOSED).
3. `crates/corelink-container/src/customer_d1.rs:1149-1150` — tenant-scoped key listing SELECT.
4. `crates/corelink-container/src/customer_d1.rs:1238-1253` — `INSERT INTO pat` (hash + token_id persisted, plaintext not).
5. `crates/corelink-container/src/customer_d1.rs:1316-1318` — idempotent tenant-scoped revoke UPDATE.
6. `crates/corelink-container/src/routes/internal_pat.rs:1-22` — the `/_internal/pat/mint` route + the DEDICATED-key-only auth gate (no shared-key fallback; fail-CLOSED — DD-HIGH remediation).
7. `crates/corelink-container/src/routes/internal_pat.rs:73-75` — plaintext never persisted; caller's responsibility.
8. `crates/corelink-container/src/routes/internal_pat.rs:88-92` — M7: signup-worker writes the hash to the D1 row.
9. `crates/corelink-container/src/routes/internal_pat.rs:502-659` — `handle_mint`: mint is a pure function returning plaintext + hash; a mint failure returns an OPAQUE 503 body (detail logged server-side only) (`crates/corelink-container/src/routes/internal_pat.rs:608-620`).
10. `crates/corelink-container/src/adapter_pat.rs:144-150` — the verifier's D1 row reader mapping a NULL `scope` to `""`.
11. `crates/corelink-container/src/scope.rs:73-95` — the fail-CLOSED scope gate (`""` grants nothing).
