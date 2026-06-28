---
type: "ADR"
title: "ADR-S33-001 — Team multi-seat: additive membership model (not Clerk Orgs)"
description: "Decides to build the sold team tier as an additive custom D1 membership layer rather than a risky pre-launch migration of solo tenants to Clerk Organizations. The decision is now BUILT + wired (invite/remove/seat-isolation)."
source_files:
  - "specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md"
checkpoint_sha: "175a91320376cd80ada9797944e118ec1ff81c63"
provenance: "AUTHORED"
tags: ["adr", "tenancy", "identity", "authz", "team", "multi-seat", "clerk", "s33"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S33-001 — Team multi-seat: additive membership model (not Clerk Orgs)

CoreLink sells a paid `team` tier, and this ADR's decision is now SHIPPED + wired: the prod handler `handle_team_invite` returns **201 Created** via `state.team.invite()` (`crates/corelink-container/src/routes/customer.rs` invite handler ~845-890), the sibling `handle_team_remove` flips the seat to `removed` AND revokes the member's PATs, and seat membership is persisted email-hash-keyed in the `team_member` table (migration `0074_team_member.sql`). The old `501 NotImplemented` survives only as an unused fallback branch, not the served path. This ADR (Proposed) records the design — an additive custom membership layer rather than migrating every solo tenant to Clerk Organizations, which would be a high-risk pre-launch identity rewrite. It closed the charging-for-an-unbuilt-feature gap without destabilising the working solo-tenant model. Related: [the PAT moat](/auth/pat-moat.md) and [the D1 PAT store](/auth/d1-pat-store.md) that member PATs reuse unchanged.

# Context

The product is architecturally solo-tenant — a tenant binds to a single Clerk user and the session JWT carries `tenant_id` directly with no Clerk Organization behind it — and a paid `team` tier is sold. When this ADR was authored the `team` feature shipped as a 501/synthesized-owner stub; the three multi-seat journeys (invite → second-seat scoped access → seat-removal revokes access) were a correctly-gated honest v1 gap, as framed at `specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md:28-47`. Those journeys are now BUILT (invite 201, seat-removal revokes PATs, seat isolation keyed on `email_hash`); the 501 remains only as a dead fallback branch.

# Decision

The decision builds multi-seat as an additive custom membership layer: a new D1 `team_member` table (email-hash only, roles owner/admin/member/viewer), invitation via the Clerk Backend API with admin-created memberships for the test path, a session→tenant resolver change (the one auth-path change) that resolves an active member to their team tenant, per-seat tenant-scoped PATs whose scope cannot exceed the member's role, and a seat-removal that load-bearingly revokes every PAT the member holds (not just flips a row) — recorded at `specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md:49-90`. Adopting Clerk Organizations was rejected for launch as a high-risk identity rewrite; a fake-200 DELETE was rejected as gambiarra.

# Consequences

The sold `team` tier is real with existing solo tenants unaffected and the data-plane authz unchanged (member PATs are ordinary tenant PATs); the seat-removal path is load-bearingly destructive (it revokes the member's PATs, not just flips a row). The session→tenant resolver sits in the safety-critical authz path and MUST be covered by isolation tests (member resolves to team tenant, removed member resolves to nothing, cross-team denial), per `specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md:104-136`. With invite/remove/seat-isolation now shipped, this ADR remains the design-of-record; the residual 501 branch is dead code pending removal.

# Citations

1. `specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md:28-47` — the solo-tenant identity model, the sold-but-stubbed team tier, and the gated journeys (Context).
2. `specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md:49-90` — the additive-membership decision (team_member table, Clerk invite, resolver change, per-seat PATs, PAT-revoking seat removal).
3. `specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md:104-136` — consequences, the authz-path risk + required isolation tests, and the sequenced WP plan.
