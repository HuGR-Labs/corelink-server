---
type: "ADR"
title: "ADR-0031 — Auth Neon schema: pgcrypto + RLS + DSR cascade"
description: "Why the auth domain lives in Neon Postgres with pgcrypto column encryption, default-on RLS, deterministic HMAC email-hash lookup, additive-only migrations, and a hard-coded DSR cascade."
source_files:
  - "specs/03_architecture/adrs/ADR-0031-neon-schema-pgcrypto.md"
checkpoint_sha: "0aad76e1d132cd98d35c814a5bb23008c226d08e"
provenance: "AUTHORED"
tags: ["adr", "neon", "postgres", "pgcrypto", "rls", "dsr", "auth", "s03"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0031 — Auth Neon schema: pgcrypto + RLS + DSR cascade

The auth domain is global, relational, and PII-bearing, so it cannot ride on regional D1 and cannot trust a single defence layer. This ADR records the DESIGN-INTENT for why the auth tables would live in Neon Postgres and how five load-bearing invariants — default-on RLS, encrypted PII at rest, additive-only migrations, a complete DSR cascade, and audit pseudonymization — are each met by a specific schema decision.

> **Status vs shipped code (2026-06-28):** Neon Postgres is NOT the live auth store. The deployed control plane is **D1 + Clerk** — identity/session live in Clerk, and the per-tenant/PAT rows live in D1 (see [the D1 PAT store](/auth/d1-pat-store.md)). The pgcrypto BYTEA encryption, default-on RLS, HMAC `email_hash` lookup, and the `account → tenant/membership/pat/webauthn` DSR cascade described below are spec-level design that is DEFERRED in production; the cascade exists only as a host-side in-memory simulator of the WI-S03-005 schema (`corelink-auth/src/schema/sim.rs`, the `dsr_hard_delete_account` cascade), not against a real Neon database (this ADR's own deferral note, spec lines 178-184).

# Context

The schema must satisfy five CRITICAL/HIGH invariants from the registry — RLS-default-on (a forgotten tenant filter returns zero rows, not cross-tenant data), PII-encrypted (`email`/`billing_email`/WebAuthn keys ride as BYTEA ciphertext), additive-only migrations, a `account → tenant/membership/pat/webauthn` DSR cascade, and audit pseudonymization that survives erasure — and D1 cannot provide pgcrypto, logical replication, or rich JSONB for this global PII-bearing domain (ADR-0031:24-79).

# Decision

Neon is the canonical auth store with UUIDv7 minted app-side; encrypted columns ride as `pgp_sym_encrypt_bytea` BYTEA with a sibling `<col>_key_id` enabling zero-downtime key rotation; equality lookups use a deterministic `email_hash = HMAC-SHA256` whose key is HKDF-derived with domain-separated info bytes (so the randomised cipher column needn't be table-scanned); RLS is default-on with `SET LOCAL app.current_tenant` per request transaction and a `before_acquire` reset hook; migrations are additive-only enforced by a CI scanner; and the DSR cascade is hard-coded in FK policy with `pat.issued_to_user` nullable and `revocation_log.pat_id` a non-FK column so service tokens and propagation rows survive user erasure (ADR-0031:82-184).

# Consequences

An insider DB read or backup leak sees ciphertext only, app-layer tenant-filter bugs fail safe to zero rows, key rotation is gradual, and GDPR Art. 17 / LGPD Art. 18 erasure is a single SQL transaction — traded against ~5-10% RLS hot-path overhead, a required rotation worker, and `email_hash_key` secrecy as a rainbow-attack dependency mitigated by its own separate quarterly rotation (ADR-0031:186-204). It governs the [D1 PAT store](/auth/d1-pat-store.md) and [tenant isolation](/tenancy/isolation.md).

# Citations

1. `specs/03_architecture/adrs/ADR-0031-neon-schema-pgcrypto.md:29-48` — the five load-bearing invariants the schema must satisfy (Context).
2. `specs/03_architecture/adrs/ADR-0031-neon-schema-pgcrypto.md:82-156` — Neon store, UUIDv7, pgcrypto BYTEA + key_id rotation, HMAC/HKDF email_hash, RLS default-on (Decision).
3. `specs/03_architecture/adrs/ADR-0031-neon-schema-pgcrypto.md:158-184` — additive-only migration governance and the DSR cascade FK policy (Decision).
4. `specs/03_architecture/adrs/ADR-0031-neon-schema-pgcrypto.md:186-204` — ciphertext-only-on-leak and fail-safe-RLS vs RLS overhead and email_hash_key risk (Consequences).
