---
type: "AuthMechanism"
title: "Edge identity resolution (Clerk session → tenant)"
description: "How the Worker verifies a Clerk session and resolves the verified subject to an isolated CoreLink tenant."
source_files:
  - "worker/src/lib/clerk_auth.ts"
  - "worker/src/lib/githugr_provision.ts"
  - "worker/src/lib/tenant_lookup.ts"
  - "migrations/d1/0074_team_member.sql"
source_blobs:
  - "worker/src/lib/clerk_auth.ts@8e368e1447a8084cd6788d115e4434e9a3c52453"
  - "worker/src/lib/githugr_provision.ts@a0c7da1423f36f48b6b345a9d88613dbc7a01d73"
  - "worker/src/lib/tenant_lookup.ts@dd58aa2b958486c3f9803d850dc52f7327577537"
  - "migrations/d1/0074_team_member.sql@5cc43828b3baeaf573ae22e57c28483ddce39b58"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["auth", "clerk", "tenant-resolution", "edge"]
timestamp: "2026-06-27T00:00:00Z"
---

# Edge identity resolution (Clerk session → tenant)

The dashboard path accepts a Clerk bearer token, verifies it against the configured issuer and authorized-party allowlist, and requires a verified subject. It resolves that subject first against the owner row and then against an active `team_member` seat. The role defaults to `viewer`; a missing tenant or D1 failure returns an error without assigning a tenant (`worker/src/lib/clerk_auth.ts:106-343`; `migrations/d1/0074_team_member.sql:28-66`).

The optional githugr path is dormant unless its issuer and public key are configured and opt-in is enabled. It verifies the githugr token, pins the issuer, then provisions or looks up a tenant derived from the verified subject. A D1 fault fails closed; it never falls back to a shared tenant (`worker/src/lib/clerk_auth.ts:116-151`; `worker/src/lib/clerk_auth.ts:384-479`; `worker/src/lib/githugr_provision.ts:104-210`).

The internal tenant lookup is a separate owner-only endpoint. It requires a subject and reads the owner mapping; it does not use the dashboard team-member fallback (`worker/src/lib/tenant_lookup.ts:79-137`).

# Invariants

- The tenant identity comes from a verified Clerk subject and a database mapping, not from a caller-supplied tenant id (`worker/src/lib/clerk_auth.ts:106-343`).
- Team-member resolution requires an active seat; the seat table is keyed for the `(user_id, status)` lookup (`worker/src/lib/clerk_auth.ts:284-343`; `migrations/d1/0074_team_member.sql:28-66`).
- A githugr user resolves to a subject-derived isolated tenant, and provisioning or lookup faults fail closed (`worker/src/lib/clerk_auth.ts:384-479`; `worker/src/lib/githugr_provision.ts:104-210`).
- The internal tenant lookup remains owner-only and does not resolve team seats (`worker/src/lib/tenant_lookup.ts:79-137`).

# Citations

1. `worker/src/lib/clerk_auth.ts:106-343` — CoreLink Clerk verification, owner lookup, active team-member fallback, role default, and fail-closed error handling.
2. `worker/src/lib/clerk_auth.ts:384-479` — githugr issuer verification and subject-derived tenant resolution.
3. `worker/src/lib/githugr_provision.ts:104-210` — deterministic tenant derivation, lookup-first provisioning, and authoritative read-back.
4. `worker/src/lib/tenant_lookup.ts:79-137` — internal owner-only tenant lookup.
5. `migrations/d1/0074_team_member.sql:28-66` — active-seat table definition and lookup index.
