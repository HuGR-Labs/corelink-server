---
type: "AuthMechanism"
title: "D1 PAT store + existence/scope check"
description: "Where PAT verifier rows live in D1, how mint and revoke write them, and how the stored scope is interpreted."
source_files:
  - "crates/corelink-container/src/customer_d1_billing_keys.rs"
  - "crates/corelink-container/src/customer_d1_maps_calendar.rs"
  - "crates/corelink-container/src/routes/internal_pat/part-00-00.rs"
  - "crates/corelink-container/src/routes/internal_pat/part-00-01.rs"
  - "crates/corelink-container/src/routes/internal_pat/part-01.rs"
  - "crates/corelink-container/src/adapter_pat_lookup.rs"
  - "crates/corelink-container/src/adapter_pat_verifier/part-02.rs"
  - "crates/corelink-container/src/scope.rs"
source_blobs:
  - "crates/corelink-container/src/customer_d1_billing_keys.rs@1788eda7d562547a4941cbd01f425660a2164fe8"
  - "crates/corelink-container/src/customer_d1_maps_calendar.rs@e7e470f8d4c7f025b8e5478ed6d2dcceea3646c5"
  - "crates/corelink-container/src/routes/internal_pat/part-00-00.rs@434225d8195195dd8ee364752fc7d8463e76c89c"
  - "crates/corelink-container/src/routes/internal_pat/part-00-01.rs@b257d228d8c062827dc45f98c8b070cd0073f4b1"
  - "crates/corelink-container/src/routes/internal_pat/part-01.rs@b74194b4466ff5f135c7d114830f57b93e0a4a9e"
  - "crates/corelink-container/src/adapter_pat_lookup.rs@a43ee870fd784a39efd14c68bcf0e34d3b1ebeb7"
  - "crates/corelink-container/src/adapter_pat_verifier/part-02.rs@b59252c1e5d8ecd2b3abf6fdf91e52e816d8bf8f"
  - "crates/corelink-container/src/scope.rs@bb38593c90351cd25e9955615f76e8ba77c10c4f"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["auth", "pat", "d1", "store", "scope"]
timestamp: "2026-07-17T00:00:00Z"
---

# D1 PAT store + existence/scope check

The D1 `pat` row is the authority for a token's tenant, stored hash, scope, expiry, and revocation state. The plaintext PAT is returned once by a mint caller; verification reads the row by token id and rejects missing, expired, or revoked rows (`crates/corelink-container/src/adapter_pat_lookup.rs:45-190`).

Self-serve key creation maps requested capabilities to the constrained stored scope and an additive `find_only` bit, then writes the PAT row. Listing reverses that mapping. Revocation is a tenant-scoped, idempotent update (`crates/corelink-container/src/customer_d1_maps_calendar.rs:44-98`; `crates/corelink-container/src/customer_d1_billing_keys.rs:132-278`; `crates/corelink-container/src/customer_d1_billing_keys.rs:347-381`).

The internal mint endpoint has a dedicated authentication key. It does not fall back to the shared internal key, and the route is unavailable when the dedicated key is missing, blank, or too short. Its authenticated handler mints and returns the PAT material but does not write the D1 row; the caller owns persistence (`crates/corelink-container/src/routes/internal_pat/part-00-00.rs:1-80`; `crates/corelink-container/src/routes/internal_pat/part-00-01.rs:139-324`; `crates/corelink-container/src/routes/internal_pat/part-01.rs:18-32`).

Scope checks use exact recognized tokens. A missing or empty scope grants no cache read or write. The adapter verifier rejects a find-only PAT before evaluating the cache-read grant (`crates/corelink-container/src/scope.rs:73-95`; `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:428-475`).

# Invariants

- The verifier reads tenant, hash, scope, expiry, revocation, and find-only state through one row decoder (`crates/corelink-container/src/adapter_pat_lookup.rs:45-190`).
- Self-serve mint cannot grant the admin scope, and find-only remains separate from the constrained stored `scope` label (`crates/corelink-container/src/customer_d1_maps_calendar.rs:44-98`; `crates/corelink-container/src/customer_d1_billing_keys.rs:198-278`).
- Revoke is tenant-scoped and idempotent (`crates/corelink-container/src/customer_d1_billing_keys.rs:347-381`).
- The adapter verifier rejects find-only PATs before cache-read grants (`crates/corelink-container/src/adapter_pat_verifier/part-02.rs:428-475`).
- The internal PAT mint route requires the dedicated key, parses only after authentication, and does not disclose internal mint errors to the caller (`crates/corelink-container/src/routes/internal_pat/part-00-00.rs:1-80`; `crates/corelink-container/src/routes/internal_pat/part-00-01.rs:139-324`; `crates/corelink-container/src/routes/internal_pat/part-01.rs:18-32`).

# Citations

1. `crates/corelink-container/src/adapter_pat_lookup.rs:45-190` — PAT row shape and the shared fail-closed row decoder.
2. `crates/corelink-container/src/customer_d1_maps_calendar.rs:44-98` — requested-scope mapping, find-only classification, and listing reverse map.
3. `crates/corelink-container/src/customer_d1_billing_keys.rs:198-278` — self-serve key create and D1 persistence.
4. `crates/corelink-container/src/customer_d1_billing_keys.rs:347-381` — tenant-scoped listing and idempotent revoke.
5. `crates/corelink-container/src/routes/internal_pat/part-00-00.rs:1-80` — dedicated-key-only mint security model.
6. `crates/corelink-container/src/routes/internal_pat/part-00-01.rs:139-324` — mint route and handler.
7. `crates/corelink-container/src/routes/internal_pat/part-01.rs:18-32` — dedicated-key environment setup and fail-closed route mounting.
8. `crates/corelink-container/src/adapter_pat_verifier/part-02.rs:428-475` — find-only rejection before the adapter cache-read grant.
9. `crates/corelink-container/src/scope.rs:73-95` — exact-token cache read/write gate; an empty or unknown scope grants neither capability.
