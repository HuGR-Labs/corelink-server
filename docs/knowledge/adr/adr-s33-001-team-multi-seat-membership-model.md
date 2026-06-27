---
type: "ADR"
title: "ADR-S33-001 — Team multi-seat: additive membership model (not Clerk Orgs)"
description: "Decides to build the sold team tier as an additive custom D1 membership layer rather than a risky pre-launch migration of solo tenants to Clerk Organizations."
source_files:
  - "specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md"
  - "crates/corelink-container/src/customer_d1.rs"
  - "migrations/d1/0074_team_member.sql"
checkpoint_sha: "10218d5bf423d6666228c796ee4118222f3456d7"
provenance: "AUTHORED"
tags: ["adr", "tenancy", "identity", "authz", "team", "multi-seat", "clerk", "s33"]
timestamp: "2026-06-26T00:00:00Z"
---

# ADR-S33-001 — Team multi-seat: additive membership model (not Clerk Orgs)

CoreLink sells a paid `team` tier. This ADR (Proposed) decided to build it as an additive custom
membership layer rather than migrating every solo tenant to Clerk Organizations, which would be a
high-risk pre-launch identity rewrite. **The 501/synthesized-owner framing in this ADR is now stale —
the decision has since been built** (see the Status note below); the design rationale is retained as the
record of *why* the membership model was chosen over Clerk Orgs. Related: [the PAT moat](/auth/pat-moat.md) and [the D1 PAT store](/auth/d1-pat-store.md) that member PATs reuse unchanged.

# Context

The product is architecturally solo-tenant — a tenant binds to a single Clerk user and the session JWT carries `tenant_id` directly with no Clerk Organization behind it — yet a paid `team` tier is sold while the `team` feature ships as a 501/synthesized-owner stub, making the three multi-seat journeys (invite → second-seat scoped access → seat-removal revokes access) a correctly-gated honest v1 gap, as framed at `specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md:28-47`.

# Decision

The decision builds multi-seat as an additive custom membership layer: a new D1 `team_member` table (email-hash only, roles owner/admin/member/viewer), invitation via the Clerk Backend API with admin-created memberships for the test path, a session→tenant resolver change (the one auth-path change) that resolves an active member to their team tenant, per-seat tenant-scoped PATs whose scope cannot exceed the member's role, and a seat-removal that load-bearingly revokes every PAT the member holds (not just flips a row) — recorded at `specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md:49-90`. Adopting Clerk Organizations was rejected for launch as a high-risk identity rewrite; a fake-200 DELETE was rejected as gambiarra.

# Consequences

The sold `team` tier becomes real with existing solo tenants unaffected and the data-plane authz unchanged (member PATs are ordinary tenant PATs), but the session→tenant resolver sits in the safety-critical authz path and MUST be covered by isolation tests (member resolves to team tenant, removed member resolves to nothing, cross-team denial); it is a sequenced multi-WP feature, and until WP-1..7 land the team feature stays the honest 501/single-owner v1 with this ADR as the tracking reference, per `specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md:104-136`.

# Status vs shipped code

**This ADR is stale the other way: the membership model is BUILT, not a 501 stub.** Migration
`migrations/d1/0074_team_member.sql` creates the `team_member` table; `invite()` durably INSERTs an
`invited` row (SHA-256 `email_hash`, CHECK-safe role) and **returns success, not a 501**
(`crates/corelink-container/src/customer_d1.rs:1140-1196`); `remove()` load-bearingly revokes **every
live PAT** the member holds via a tenant+principal-scoped `UPDATE pat SET revoked_at_ms`, returning the
revoked count (`crates/corelink-container/src/customer_d1.rs:1209-1281`). So the per-seat PAT model and
the PAT-revoking seat removal this ADR decided are implemented; the in-text "team invite is a 501 /
synthesized-owner stub" framing should be read as the pre-build state.

**Unresolved product contradiction (owner call):** this ADR builds out the `team` tier, but
[ADR-S19-001](/adr/adr-s19-001-tier-taxonomy-amendment-5-to-6.md)'s canonical 6-tier taxonomy
(`free|solo|starter|pro|max|enterprise`) **REMOVES `team`** as a sold plan. The two ADRs disagree on
whether `team` is a product SKU at all; the membership *mechanism* is built and reusable, but whether a
customer-facing `team` tier ships is an open product question that S-19's taxonomy did not reconcile
with this one.

# Citations

1. `specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md:28-47` — the solo-tenant identity model, the sold-but-stubbed team tier, and the gated journeys (Context).
2. `specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md:49-90` — the additive-membership decision (team_member table, Clerk invite, resolver change, per-seat PATs, PAT-revoking seat removal).
3. `specs/03_architecture/adrs/ADR-S33-001-team-multi-seat-membership-model.md:104-136` — consequences, the authz-path risk + required isolation tests, and the sequenced WP plan.
4. `migrations/d1/0074_team_member.sql:28` — the shipped `CREATE TABLE team_member` migration: the membership model is BUILT, not the 501 stub the ADR text describes.
