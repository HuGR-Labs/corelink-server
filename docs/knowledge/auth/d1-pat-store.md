---
type: "AuthMechanism"
title: "D1 PAT store + existence/scope check"
description: "Where PAT rows live in D1, how they are created and revoked, and the scope mapping that gates self-serve key mint."
source_files:
  - "crates/corelink-container/src/customer_d1.rs"
  - "crates/corelink-container/src/routes/internal_pat.rs"
  - "crates/corelink-container/src/adapter_pat.rs"
  - "crates/corelink-container/src/scope.rs"
checkpoint_sha: "92140a5af7b41d50d8b43e2dfa7ab45686a596ac"
provenance: "AUTHORED"
tags: ["auth", "pat", "d1", "store", "scope"]
timestamp: "2026-07-17T00:00:00Z"
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
  (`crates/corelink-container/src/customer_d1.rs:1292-1305`).
- The requested-scope → stored-scope map is FROZEN and fail-CLOSED: `admin` is NEVER grantable
  self-serve, anything-with-write → `read-write`, any read → `read-only`, an explicit find-only request →
  the new `find-missing` value (ADR-0071), else `read-only`
  (`crates/corelink-container/src/customer_d1.rs:310-328`).
- The minted PAT's embedded scope bitset mirrors that stored `scope` string: `read-write` carries
  `SCOPE_CACHE_RW | SCOPE_CACHE_FIND`, `find-missing` carries ONLY `SCOPE_CACHE_FIND`, and the
  read-only default carries `SCOPE_CACHE_R | SCOPE_CACHE_FIND` — read is a superset of find-missing, so
  every read/read-write PAT also carries the FIND bit (`crates/corelink-container/src/customer_d1.rs:1236-1241`).
- Revoke is an idempotent, tenant-scoped soft delete: `UPDATE pat SET revoked_at_ms = ?1 WHERE pat_id =
  ?2 AND tenant_id = ?3 AND revoked_at_ms IS NULL` — a PAT owned by another tenant is simply not found
  (`crates/corelink-container/src/customer_d1.rs:1360-1361`).
- Listing is read-only and always self-tenant-scoped, mapping the stored `scope` back to a dashboard
  scopes list via `scope_to_list` (`find-missing` → `["cache:find-missing"]`)
  (`crates/corelink-container/src/customer_d1.rs:1184-1185`; `crates/corelink-container/src/customer_d1.rs:334-342`).
- The internal mint route is the SECOND writer's mint half: `POST /_internal/pat/mint` mints the PAT
  and returns plaintext + Argon2id hash, gated by a constant-time compare against the DEDICATED
  `CORELINK_PAT_MINT_AUTH_KEY` ONLY — with NO fallback to the shared `CORELINK_INTERNAL_AUTH_KEY`. Because
  this surface can mint ANY tenant's PAT (incl. `SCOPE_ADMIN_ALL`), the dedicated key is REQUIRED and an
  unset/blank/`< 32`-char value fails CLOSED: the route is NOT mounted and the endpoint is unavailable
  (503) rather than silently widened to the broad shared signup-worker credential — the DD-HIGH
  remediation (`crates/corelink-container/src/routes/internal_pat.rs:1-22`).
- That mint route is a PURE function — it NEVER persists the row itself; the caller (signup-worker)
  writes the returned `hash` to the D1 `pat` row and discards the plaintext after one use
  (`crates/corelink-container/src/routes/internal_pat.rs:550-710`).
- A mint failure returns an OPAQUE 503 body (`{"error":"mint_failed"}`): the real `PatError` detail
  (e.g. `SigningKeyTooShort`, entropy/hash-corruption internals) is logged SERVER-SIDE only and never
  disclosed in the response — even to a holder of the mint auth key
  (`crates/corelink-container/src/routes/internal_pat.rs:659-671`).

# Invariants

- The PAT plaintext is never logged or persisted at the mint route; the caller writes it to Clerk
  session metadata once and discards it (`crates/corelink-container/src/routes/internal_pat.rs:73-75`).
- The `admin` scope is never grantable via self-serve key creation; unrecognized tokens fail CLOSED
  (`crates/corelink-container/src/customer_d1.rs:310-328`).
- Revoke is tenant-scoped: a cross-tenant `pat_id` cannot be revoked (or even observed)
  (`crates/corelink-container/src/customer_d1.rs:1360-1361`).

# Gotchas

- The mint route returns the Argon2id `hash` in its 200 body because the signup-worker — not the
  container — is what WRITES that hash to the D1 `pat` row. **M7 reclassified (WP-D L12a, 2026-07-17):**
  this is NOT a security exposure, not just a deferred follow-up — `hash` is a one-way Argon2id verifier
  of a high-entropy random secret (not replayable on its own), it crosses only the internal
  Worker↔`_system`-DO boundary the plaintext already crosses on the SAME response, and it is never
  logged. Dropping it would need moving the D1 `pat`-row write into the container (high blast radius) for
  zero security gain, so it stays an optional future consolidation, not a fix
  (`crates/corelink-container/src/routes/internal_pat.rs:86-108`).
- `scope` may be NULL on legacy rows; the verifier's D1 row reader maps that to `""`
  (`crates/corelink-container/src/adapter_pat.rs:145-151`), which then fails CLOSED at the scope gate
  rather than erroring (`crates/corelink-container/src/scope.rs:73-95`).
- `last_used_at` is not tracked; the list handler always reports `None`
  (`crates/corelink-container/src/customer_d1.rs:18-20`).

# Citations

1. `crates/corelink-container/src/customer_d1.rs:18-22` — the PAT-store overview: tables, create, revoke.
2. `crates/corelink-container/src/customer_d1.rs:310-328` — `map_requested_scopes`: the FROZEN requested-scope → `pat.scope` map (admin never grantable, unrecognized tokens fail CLOSED; `FindMissing` → `find-missing` per ADR-0071).
3. `crates/corelink-container/src/customer_d1.rs:334-342` — `scope_to_list`: inverse map (`find-missing` → `["cache:find-missing"]`) for the dashboard listing.
4. `crates/corelink-container/src/customer_d1.rs:1184-1185` — tenant-scoped key listing SELECT.
5. `crates/corelink-container/src/customer_d1.rs:1236-1241` — the minted PAT's scope bitset mirrors the `scope` string (read/read-write carry `SCOPE_CACHE_FIND`; find-only carries only it).
6. `crates/corelink-container/src/customer_d1.rs:1292-1305` — `INSERT INTO pat` (hash + token_id persisted, plaintext not).
7. `crates/corelink-container/src/customer_d1.rs:1360-1361` — idempotent tenant-scoped revoke UPDATE.
8. `crates/corelink-container/src/routes/internal_pat.rs:1-22` — the `/_internal/pat/mint` route + the DEDICATED-key-only auth gate (no shared-key fallback; fail-CLOSED — DD-HIGH remediation).
9. `crates/corelink-container/src/routes/internal_pat.rs:73-75` — plaintext never persisted; caller's responsibility.
10. `crates/corelink-container/src/routes/internal_pat.rs:86-108` — M7 reclassified: hash-on-wire is a one-way verifier of a high-entropy secret crossing an already-trusted internal boundary, never logged; removal (moving the D1 write into the container) is optional future consolidation, not a security fix.
11. `crates/corelink-container/src/routes/internal_pat.rs:550-710` — `handle_mint`: mint is a pure function returning plaintext + hash; a mint failure returns an OPAQUE 503 body (detail logged server-side only) (`crates/corelink-container/src/routes/internal_pat.rs:659-671`).
12. `crates/corelink-container/src/adapter_pat.rs:145-151` — the verifier's D1 row reader mapping a NULL `scope` to `""`.
13. `crates/corelink-container/src/scope.rs:73-95` — the fail-CLOSED scope gate (`""` grants nothing).
