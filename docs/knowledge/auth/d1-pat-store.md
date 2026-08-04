---
type: "AuthMechanism"
title: "D1 PAT store + existence/scope check"
description: "Where PAT rows live in D1, how they are created and revoked, and the scope mapping that gates self-serve key mint."
source_files:
  - "crates/corelink-container/src/customer_d1.rs"
  - "crates/corelink-container/src/routes/internal_pat.rs"
  - "crates/corelink-container/src/adapter_pat.rs"
  - "crates/corelink-container/src/scope.rs"
checkpoint_sha: "7bf58501233442c34e6613dd304a38afc3337f75"
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
  pat_hash, scope, expires_ms, …, token_id, name, find_only)` — the hash and the non-secret `token_id`
  are persisted, the plaintext is returned once and never stored, and the additive `find_only` marker
  (`?10`, migration 0093) is `1` for a find-only PAT and `0` otherwise
  (`crates/corelink-container/src/customer_d1.rs:1327-1344`).
- The requested-scope → stored-scope map is FROZEN and fail-CLOSED: `admin` is NEVER grantable
  self-serve, anything-with-write → `read-write`, otherwise → `read-only`. A find-only request classifies
  as `FindMissing` but folds into the SAME CHECK-safe base `read-only` string — it is distinguished ONLY
  by the additive `find_only` marker (ADR-0071, migration 0093 — NOT a 4th `pat.scope` value, which the
  0037 CHECK would reject), computed by `mint_is_find_only`
  (`crates/corelink-container/src/customer_d1.rs:318-332`; `crates/corelink-container/src/customer_d1.rs:341-346`).
- The minted PAT's embedded scope bitset mirrors the EFFECTIVE grant, not the stored `scope` string
  alone: a find-only PAT (`find_only = 1`) carries ONLY `SCOPE_CACHE_FIND`, `read-write` carries
  `SCOPE_CACHE_RW | SCOPE_CACHE_FIND`, and the read-only default carries `SCOPE_CACHE_R | SCOPE_CACHE_FIND`
  — read is a superset of find-missing, so every read/read-write PAT also carries the FIND bit
  (`crates/corelink-container/src/customer_d1.rs:1267-1273`).
- Revoke is an idempotent, tenant-scoped soft delete: `UPDATE pat SET revoked_at_ms = ?1 WHERE pat_id =
  ?2 AND tenant_id = ?3 AND revoked_at_ms IS NULL` — a PAT owned by another tenant is simply not found
  (`crates/corelink-container/src/customer_d1.rs:1396-1397`).
- Listing is read-only and always self-tenant-scoped, reading the stored `scope` + the `find_only` marker
  and mapping them back to a dashboard scopes list via `scope_to_list(scope, find_only)` (a find-only PAT
  — base `read-only` + `find_only = 1` — surfaces as `["cache:find-missing"]`, never `["cache:read"]`)
  (`crates/corelink-container/src/customer_d1.rs:1210-1211`; `crates/corelink-container/src/customer_d1.rs:351-364`).
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
  (`crates/corelink-container/src/customer_d1.rs:318-332`).
- A find-only PAT never widens `pat.scope` beyond the CHECK-safe base `read-only`; its find-missing
  capability lives solely in the additive `find_only` marker (`crates/corelink-container/src/customer_d1.rs:341-346`).
- Revoke is tenant-scoped: a cross-tenant `pat_id` cannot be revoked (or even observed)
  (`crates/corelink-container/src/customer_d1.rs:1396-1397`).

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
  (`crates/corelink-container/src/adapter_pat.rs:162-166`), which then fails CLOSED at the scope gate
  rather than erroring (`crates/corelink-container/src/scope.rs:73-95`). The SAME reader also selects and
  parses the additive `find_only` marker (migration 0093) into `PatRow.find_only` (NULL/`0` ⇒ a normal
  PAT, `1` ⇒ find-only), which the adapter verifier fail-CLOSES on before the read grant
  (`crates/corelink-container/src/adapter_pat.rs:170`; field at `crates/corelink-container/src/adapter_pat.rs:106`).
- `last_used_at` is not tracked; the list handler always reports `None`
  (`crates/corelink-container/src/customer_d1.rs:18-20`).

# Citations

1. `crates/corelink-container/src/customer_d1.rs:18-22` — the PAT-store overview: tables, create, revoke.
2. `crates/corelink-container/src/customer_d1.rs:318-332` — `map_requested_scopes`: the FROZEN requested-scope → `pat.scope` map (admin never grantable, unrecognized tokens fail CLOSED; `FindMissing` and `ReadOnly` BOTH map to the CHECK-safe base `read-only` per ADR-0071).
2a. `crates/corelink-container/src/customer_d1.rs:341-346` — `mint_is_find_only`: classifies a find-only request so the mint sets the additive `find_only` marker (NOT a 4th `pat.scope` value).
3. `crates/corelink-container/src/customer_d1.rs:351-364` — `scope_to_list(scope, find_only)`: inverse map — a find-only PAT (base `read-only` + `find_only = 1`) surfaces as `["cache:find-missing"]` for the dashboard listing.
4. `crates/corelink-container/src/customer_d1.rs:1210-1211` — tenant-scoped key listing SELECT (now also reads `find_only`).
5. `crates/corelink-container/src/customer_d1.rs:1267-1273` — the minted PAT's scope bitset mirrors the effective grant (read/read-write carry `SCOPE_CACHE_FIND`; a find-only PAT carries only it).
6. `crates/corelink-container/src/customer_d1.rs:1327-1344` — `INSERT INTO pat` (hash + token_id persisted, plaintext not; the `find_only` marker is `?10`).
7. `crates/corelink-container/src/customer_d1.rs:1396-1397` — idempotent tenant-scoped revoke UPDATE.
8. `crates/corelink-container/src/routes/internal_pat.rs:1-22` — the `/_internal/pat/mint` route + the DEDICATED-key-only auth gate (no shared-key fallback; fail-CLOSED — DD-HIGH remediation).
9. `crates/corelink-container/src/routes/internal_pat.rs:73-75` — plaintext never persisted; caller's responsibility.
10. `crates/corelink-container/src/routes/internal_pat.rs:86-108` — M7 reclassified: hash-on-wire is a one-way verifier of a high-entropy secret crossing an already-trusted internal boundary, never logged; removal (moving the D1 write into the container) is optional future consolidation, not a security fix.
11. `crates/corelink-container/src/routes/internal_pat.rs:550-710` — `handle_mint`: mint is a pure function returning plaintext + hash; a mint failure returns an OPAQUE 503 body (detail logged server-side only) (`crates/corelink-container/src/routes/internal_pat.rs:659-671`).
12. `crates/corelink-container/src/adapter_pat.rs:162-166` — the verifier's D1 row reader mapping a NULL `scope` to `""` (the same reader parses the additive `find_only` marker at `crates/corelink-container/src/adapter_pat.rs:170` into the `PatRow.find_only` field at `crates/corelink-container/src/adapter_pat.rs:106`).
13. `crates/corelink-container/src/scope.rs:73-95` — the fail-CLOSED scope gate (`""` grants nothing).
