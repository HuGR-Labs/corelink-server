---
type: "AuthFlow"
title: "The edge PAT-mint lifecycle (one authority, three consumers)"
description: "Worker session, runner, and rotation flows share one PAT mint chokepoint and the container's dedicated mint authority."
source_files:
  - "worker/src/lib/session_exchange.ts"
  - "worker/src/lib/runner_mint.ts"
  - "worker/src/lib/auth_rotate.ts"
source_blobs:
  - "worker/src/lib/session_exchange.ts@94e6cdcecc9eb604c8992b5924d2c85b9e2b6839"
  - "worker/src/lib/runner_mint.ts@b6d3f85921c9e83314383951d214738f63d78fac"
  - "worker/src/lib/auth_rotate.ts@21a37408fa1a50485c15064aabc1f082959a910b"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["auth", "pat", "mint", "worker-edge", "tenancy"]
timestamp: "2026-06-27T00:00:00Z"
---

# The edge PAT-mint lifecycle (one authority, three consumers)

The Worker uses one `mintScopedPat` function to call the container's PAT mint endpoint and persist the returned PAT row. Its `MintGrant` capability binds the tenant, principal source, and maximum scope to one of the approved caller-specific constructors; the requested scope cannot exceed that ceiling. The mint function calls the dedicated container endpoint, then writes the PAT record through the Worker's D1 binding (`worker/src/lib/session_exchange.ts:215-263`, `worker/src/lib/session_exchange.ts:636-945`).

The session exchange calls the chokepoint with a grant derived from the verified session and limits a viewer to read-only scope. Token exchange uses its own grant and rejects an audience that differs from the authenticated tenant (`worker/src/lib/session_exchange.ts:535-592`, `worker/src/lib/session_exchange.ts:980-1071`).

Runner mint derives the tenant from the runner installation and entitlement rows, then calls the same chokepoint with the runner job as principal. PAT rotation reads the existing row, rejects revoked or cross-tenant keys, and revokes the old PAT only after the replacement mint succeeds (`worker/src/lib/runner_mint.ts:289-714`; `worker/src/lib/auth_rotate.ts:158-374`).

# Invariants

- The request body does not supply the mint tenant or scope ceiling; the `MintGrant` constructors bind these values at the trusted caller (`worker/src/lib/session_exchange.ts:215-263`; `worker/src/lib/runner_mint.ts:289-714`).
- Requested scope above the grant ceiling fails before the container mint or D1 write (`worker/src/lib/session_exchange.ts:636-764`).
- Rotation is tenant-scoped and preserves the old credential until its replacement succeeds (`worker/src/lib/auth_rotate.ts:158-374`).

# Citations

1. `worker/src/lib/session_exchange.ts:215-263` — branded `MintGrant` type and the caller-specific grant constructors.
2. `worker/src/lib/session_exchange.ts:636-945` — shared mint, scope-ceiling check, dedicated container call, D1 persistence, and failure handling.
3. `worker/src/lib/session_exchange.ts:535-592` — verified-session mint call and its viewer scope limit.
4. `worker/src/lib/session_exchange.ts:980-1071` — token exchange grant and audience-to-tenant check.
5. `worker/src/lib/runner_mint.ts:289-714` — runner identity and entitlement checks before delegating to `mintScopedPat`.
6. `worker/src/lib/auth_rotate.ts:158-374` — old-row and tenant checks, replacement mint, then old-row revocation.
