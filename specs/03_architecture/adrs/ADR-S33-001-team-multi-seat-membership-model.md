---
id: "ADR-S33-001"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-25"
updated: "2026-09-05"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "identity", "authz", "team", "multi-seat", "billing-seats", "clerk"]
references:
  - "crates/corelink-container/src/customer_d1.rs"
  - "crates/corelink-ratelimit/src/tier.rs"
  - "tests/e2e-user-journeys/src/journeys/team.rs"
---

# ADR-S33-001 — Team multi-seat: additive membership model (not a Clerk-Orgs migration)

- **Status:** Proposed (2026-06-25)
- **Deciders:** CoreLink tech lead (architecture delegated by the owner/stakeholder)
- **Context tags:** identity, authz, billing-seats, launch-scope

## Status

IMPLEMENTED — B-073 shipped the token-bound redemption path, verified Clerk
primary-email gate, expiry/replay checks, and atomic audit/update. The old
501/synthesized-owner wording below is historical context only; the served
invite route returns 201 and the acceptance route is live.

## Context

CoreLink **sells a paid `team` tier** (`solo | starter | team | pro | max` —
`crates/corelink-ratelimit/src/tier.rs`, `apps/signup-worker/src/webhooks/stripe.ts`).
The additive membership model is now live: `D1CustomerHandler` persists
`team_member` rows, returns a one-time invitation token, and the Worker redeems
that token only for the token-bound tenant and a verified Clerk primary email.
The earlier synthesized-owner/501 description is retained below only as the
historical pre-B-073 gap, not as the current served behavior.

The product is architecturally **solo-tenant**: a tenant binds to a single
Clerk **user** (`tenant.clerk_user_id`), and the Clerk **session JWT carries the
`tenant_id` directly** (admin-ui `auth.ts`: `org_id: decoded.tenant_id`). There
is no Clerk **Organization** behind a tenant — admin-ui literally documents the
"active organization" as "the self-serve solo-tenant case".

Charging for `team` while shipping single-owner is a real product gap. This ADR
fixes the *design* so the feature can be built for real — without a risky
pre-launch rewrite of the identity model.

## Decision

Build multi-seat as an **additive custom membership layer**, NOT a migration of
existing tenants to Clerk Organizations.

1. **Durable membership.** New D1 table `team_member`:
   `(tenant_id, user_id, email_hash, invitation_token_hash, role, status, invited_at_ms, joined_at_ms,
   invited_by)`, PK `(tenant_id, user_id)`. `role ∈ {owner, admin, member,
   viewer}`; `status ∈ {invited, active, removed}`. `email_hash` only (never raw
   email — CTRL-PRIV-001). The tenant's `clerk_user_id` remains the canonical
   **owner**; the synthesized Owner row is replaced by a real `owner` member row
   on first migration/backfill.

2. **Cryptographic invitation redemption (B-073).** `invite` creates an
   `invited` row and returns a 256-bit opaque token once; only its SHA-256
   digest is stored (migration 0108). The authenticated invitee redeems it at
   `POST /v1/customer/team/accept`. The Worker binds the target tenant from
   the token row, requires the Clerk primary email to be explicitly verified,
   enforces the 14-day window, and atomically audits plus flips the row to
   `active`. Legacy rows without a token digest are not redeemable.

3. **Session → tenant resolution consults membership.** The crux. Today a
   session resolves to the tenant baked in its JWT. For an invited member, the
   session must resolve to the **tenant they are an active member of**. This is
   the one auth-path change and is done in the Clerk session-token customization
   + the worker's tenant resolver: `tenant_id := the active team_member.tenant_id
   for this user` (with the owner's own tenant as the default when the user is
   not a member elsewhere). A user active in exactly one tenant is unambiguous;
   multi-tenant membership is out of scope for v1 (a member belongs to one team).

4. **Per-seat PATs, tenant-scoped.** An active member mints PATs scoped to the
   **tenant**, with a `scope` not exceeding their `role` (a `viewer` cannot mint
   `read-write`; only `owner`/`admin` may mint `admin`). PATs continue to carry
   the tenant; member PATs are indistinguishable at the data plane from owner
   PATs except by scope — preserving the existing native-plane authz unchanged.

5. **Seat-removal revokes access.** `DELETE /v1/customer/team/:user_id`
   (owner/admin only, never self, never the last owner): flips the row to
   `removed` AND **revokes every PAT** the member holds for that tenant
   (`UPDATE pat SET revoked_at_ms=? WHERE tenant_id=? AND principal_id=?`) AND
   removes/revokes the Clerk side. Returning 200 without the PAT revocation would
   be a security hole (removed member retains data-plane access) — the revocation
   is the load-bearing part, not the row flip.

## Alternatives considered

- **Adopt Clerk Organizations (rejected for launch).** The SOTA-native model
  (Clerk has first-class orgs/roles/invitations), but adopting it migrates every
  existing solo tenant to an org and rewrites the session→tenant derivation, the
  signup-worker tenant-creation path, and admin-ui auth — a high-risk identity
  rewrite for a pre-launch product whose solo model works. Revisit post-launch if
  org-level features (SSO, domains) are demanded. The additive model does not
  preclude a later org adoption.
- **Pretend (501 → fake 200 DELETE).** Rejected — a seat-removal that does not
  revoke PATs is gambiarra (the removed member keeps access). Violates the rigor
  compact.

## Consequences

- **Positive:** the sold `team` tier becomes real; existing solo tenants are
  unaffected (the owner row is additive); the data-plane authz is unchanged
  (member PATs are ordinary tenant PATs); seat-removal is a true security
  boundary; the three gated journeys become honestly closeable.
- **Negative / risk:** the session→tenant resolver change remains in the
  safety-critical authz path and is covered by the worker and handler focal
  tests. Billing must count active seats against the `team` tier's seat allowance
  (separate follow-up; out of scope here).
- **Scope:** the migration, handler trait/D1 implementation, token redemption,
  Clerk verification, tenant resolver, per-seat PAT scope and isolation tests
  are shipped; future Clerk-Organizations migration remains out of scope.

## Implementation WPs (shipped sequence)

1. D1 migration `0108_team_member_invitation_security.sql` + membership rows.
2. `corelink-handler-customer`: extend the team trait (`invite`/`list`/**`remove`**)
   + request/response types + InMemory impl + unit tests.
3. `customer_d1.rs`: real D1-backed `invite`/`list`/`remove` (+ PAT revocation on
   remove) replacing the 501/synthesized stubs.
4. Clerk Backend-API integration for verified-primary-email redemption.
5. Worker session→tenant resolver via the active `team_member` row, with
   cross-tenant and removed-seat safety tests.
6. Per-seat PAT scope gate (role ≤ scope) on `keys/create`.
7. Invite/accept and seat-isolation focal coverage; the old 501 branch is not a
   served path and is retained only for compatibility cleanup.
