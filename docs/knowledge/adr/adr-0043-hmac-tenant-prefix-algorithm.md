---
type: "ADR"
title: "ADR-0043 — HMAC-SHA256 + b64url-trunc-16 tenant path prefix"
description: "Canonicalizes the tenant path prefix as HMAC-SHA256(TDK, tenant_id) base64-url-no-pad truncated to 16 ASCII chars — FIPS-approved, path-safe, compact, and unforgeable across tenants."
source_files:
  - "specs/03_architecture/adrs/ADR-0043-hmac-tenant-prefix-algorithm.md"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "s01", "tenant-isolation", "hmac", "crypto-load-bearing"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-0043 — HMAC-SHA256 + b64url-trunc-16 tenant path prefix

Every R2 / KV / D1 key in CoreLink carries a tenant-bound prefix, and that prefix is the fifth layer
of tenant isolation: an attacker holding one tenant's PAT must not be able to forge another tenant's
namespace. This ADR is the decision record that pins exactly how that prefix is derived and why
HMAC-SHA256 wins over BLAKE3 and HKDF — it is the cryptographic foundation under
[tenant isolation](/tenancy/isolation.md).

# Context

Layer 5 of `INV-TENANT-ISOLATION` requires every storage key to carry a tenant-bound prefix that a
PAT-holder cannot forge for another tenant, under three competing pressures: it must be
cryptographically tenant-bound, path-safe for S3-style URLs, and compact for indexing and logs; three
primitives were weighed — HMAC-SHA256 (FIPS-approved), HMAC-BLAKE3 (faster, non-FIPS), and HKDF.

# Decision

**HMAC-SHA256 + URL-safe base64 (no padding) + truncate to 16 ASCII chars** is canonical:
`HMAC16 = b64url_no_pad(HMAC_SHA256(TDK, tenant_id_bytes))[..16]`. FIPS 140-2 compliance is the hard
gate, so BLAKE3's marginal speedup does not justify failing SOC 2 enterprise audits; HKDF was rejected
because its `info`-label domain separation duplicates a property HMAC's PRF guarantee already provides
for a single-message-space consumer, at roughly twice the cost. The internal `TenantPrefix([u8; 16])`
newtype stores the 16 ASCII bytes verbatim and is consumed by both BLOB(16) D1 columns and textual
R2/KV keys without re-encoding.

# Consequences

The derivation is deterministic (enabling content-addressable namespacing and cron-time path
reconstruction without TDK access) and zero-allocation, with cross-language reproducible vectors; the
accepted trade-offs are 96 bits of displayed entropy rather than 128 (collisions never grant
authorization, only namespace co-residence), a breaking algorithm migration that would re-key every
stored prefix (mitigated by ADR-0036's materialized-prefix mandate), and no dependency-level FIPS-mode
hard requirement (tracked for the S-20 SOC 2 pass).

# Citations

1. `specs/03_architecture/adrs/ADR-0043-hmac-tenant-prefix-algorithm.md:31-39` — Context: Layer-5 isolation requirement + the three competing requirements + the canonical formula already pinned.
2. `specs/03_architecture/adrs/ADR-0043-hmac-tenant-prefix-algorithm.md:43-43` — Decision: HMAC-SHA256 + b64url-no-pad + truncate-to-16 is canonical.
3. `specs/03_architecture/adrs/ADR-0043-hmac-tenant-prefix-algorithm.md:60-69` — the decision matrix conclusion: FIPS is the hard gate; BLAKE3 and HKDF rejected.
4. `specs/03_architecture/adrs/ADR-0043-hmac-tenant-prefix-algorithm.md:73-84` — Consequences: deterministic + zero-alloc vs the 96-bit entropy, breaking-migration, and FIPS-mode trade-offs.
