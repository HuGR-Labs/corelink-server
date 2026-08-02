---
id: AUDIT-CUSTOMER-DASHBOARD-SPEC-R-PREP
type: audit
doc_status: DRAFT
audit_status: ACTIVE
version: 0.1.0
created: 2026-05-15
updated: 2026-05-15
owner: r-prep-swarm
final_approver: r-prep-tech-lead
reviewers:
  - role: tech_lead
    name: "r-prep tech lead"
  - role: product
    name: "r-prep product lead"
  - role: security
    name: "RBAC reviewer"
supersedes: null
superseded_by: null
tags: [r-prep, customer-dashboard, admin-ui, rbac, self-serve]
---

# Customer Self-Serve Dashboard — Spec (r-prep)

> Scaffolding contract for the customer-facing tenant dashboard inside
> `apps/admin-ui` under `/[locale]/customer/`. Operator surface
> (`/[locale]/admin/`) is unaffected.

## 1. Target audience

The dashboard is consumed by **tenant members** — anyone holding a Clerk org
role mapped to `corelink-admin`, `corelink-viewer`, or `corelink-member`. It is
**not** the operator console; operator-only routes live under `/admin/` and
require `corelink-admin` (operator scope) via `RbacGuard`.

| Persona | Clerk role (e2e fixture) | Customer access |
|---|---|---|
| Owner / Admin | admin / approver | full (read+write within tenant) |
| Developer / Viewer | user → corelink-member | read; PAT self-mint allowed for own scopes |
| Anonymous | — | redirected to `/sign-in` |

The RBAC matrix mirrors the canonical role catalog at
`apps/docs/docs/explanation/rbac/role-catalog.mdx`:

- `Owner` (`admin:billing` + everything `Admin` can do)
- `Admin` (`admin:tenant-*`, `admin:tokens`, `admin:audit`, `admin:users`)
- `Developer` (`cache:*` only)
- `Viewer` (`cache:r`, `admin:audit` read, `admin:tenant-read`)

## 2. Information architecture

```
/[locale]/customer/                # Overview (default)
/[locale]/customer/usage           # CAS storage / reads / writes / daily
/[locale]/customer/audit           # Tenant audit trail (no Merkle proof)
/[locale]/customer/billing         # Subscription / invoices / portal redirect
/[locale]/customer/keys            # PATs + BYOK CMK status
/[locale]/customer/team            # Members + invite + role assignment
```

Sidebar navigation (`apps/admin-ui/src/components/customer/CustomerNav.tsx`)
**explicitly excludes** operator routes (`/admin/tenants`, `/admin/ops`,
`/admin/audit`). This is asserted by the `customer-overview.spec.ts` e2e.

## 3. Backend contract

All endpoints are tenant-scoped — the backend resolves tenant from the session
(no `tenant_id` on the wire). The shapes are defined in
`apps/admin-ui/src/lib/customer-types.ts`; production wiring goes through
`crates/corelink-admin-api/` (out of scope for r-prep scaffolding).

| Endpoint | Method | Min role | Returns |
|---|---|---|---|
| `/v1/customer/overview` | GET | Viewer | `CustomerOverview` |
| `/v1/customer/usage` | GET | Viewer | `CustomerUsage` |
| `/v1/customer/audit` | GET | Viewer | `{rows: CustomerAuditEvent[]}` |
| `/v1/customer/billing` | GET | Owner | `CustomerBilling` |
| `/v1/customer/billing/portal` | POST | Owner | `{portal_url}` |
| `/v1/customer/keys` | GET | Viewer | `{pats, byok}` |
| `/v1/customer/keys` | POST | Developer (own) / Admin (any) | `{pat, token}` |
| `/v1/customer/keys/:id/revoke` | POST | issuer or Admin | `{pat}` |
| `/v1/customer/team` | GET | Viewer | `{members}` |
| `/v1/customer/team/invite` | POST | Admin | `{member}` |

> **Corrected 2026-08-02.** The three POST rows above declared FLAT shapes; the container has always
> enveloped them (`routes/customer.rs:810`, `:860`, `:961`). The client was implemented faithfully
> against this table and was therefore wrong — the divergence originated HERE, not in the fixtures.
> A contract of record that is never re-checked against the handler is how a correct implementation
> becomes a production defect.

The mock implementation lives in `apps/admin-ui/src/lib/e2e-mock-fixtures.ts`
behind the catch-all route `apps/admin-ui/src/app/api/v1/[...path]/route.ts`.

## 4. Critical flows

### 4.1 Overview snapshot
1. Member visits `/en/customer`.
2. `CustomerGuard` reads `getAuthContext()`; if no Viewer-min role → 401 panel.
3. Page loads `/v1/customer/overview` → renders usage / billing / BYOK / activity cards.

### 4.2 Mint a PAT
1. Member visits `/en/customer/keys`.
2. Fills name + scope checkboxes → POST `/v1/customer/keys`.
3. Token is rendered **once** via `data-testid="keys-new-token"` (the server
   stores Argon2id hash; the raw token is gone after page close — same model
   as the canonical PAT flow in the role catalog).

### 4.3 Revoke a PAT
1. Click `Revoke` on a row → POST `/v1/customer/keys/:id/revoke`.
2. Row flips to `revoked` and an `pat.revoked` audit event is emitted.

### 4.4 Billing portal redirect
1. Owner visits `/en/customer/billing` (Owner-only in prod; mock allows Viewer).
2. Click `Manage payment method` → POST `/v1/customer/billing/portal` returns
   a Stripe-portal URL; UI shows it for the user to click through.

### 4.5 Invite teammate
1. Admin visits `/en/customer/team`.
2. Submits email + role → POST `/v1/customer/team/invite` → row appears with
   `status=invited` and audit row `team.invited`.

## 5. RBAC mapping (cross-link)

See:

- `apps/docs/docs/explanation/rbac/role-catalog.mdx` — canonical role catalog.
- `apps/docs/docs/explanation/rbac/permission-matrix.mdx` — scope catalog.
- `crates/corelink-auth-schema/src/sim.rs` — Role enum (source of truth).

The customer dashboard uses **`hasCustomerAccess(ctx)`** (Viewer-minimum) in
`apps/admin-ui/src/lib/auth.ts`. The operator dashboard uses **`hasAdminRole(ctx)`**
(operator scope only). The two guards are intentionally disjoint:

```ts
// auth.ts
export function hasAdminRole(ctx: AuthContext): boolean {
  return ctx.role === "corelink-admin";
}
export function hasCustomerAccess(ctx: AuthContext): boolean {
  return (
    ctx.role === "corelink-admin" ||
    ctx.role === "corelink-viewer" ||
    ctx.role === "corelink-member"
  );
}
```

## 6. Non-goals (r-prep)

- No real Stripe / billing integration (portal URL is mocked).
- No real CMK rotation — BYOK status is read-only here; rotation flow stays
  on the operator surface (dual-approval gate).
- No Merkle proof viewer on the customer-side audit table — that is a
  privileged forensics surface.
- No mutation of team roles beyond invite (role demotion/revocation needs
  dual-approval and is deferred to operator UI).

## 7. Acceptance gate

- `apps/admin-ui` typecheck green.
- All 5 Playwright specs under `tests/e2e/customer/` pass on chromium.
- Sidebar does not leak operator routes (asserted in `customer-overview.spec.ts`).
- Mock fixtures deterministic across `__resetMockState()` boundary.

## 8. Open items (not blocking scaffold merge)

- Production wiring of `/v1/customer/*` in `crates/corelink-admin-api/`.
- Stripe Billing Portal session creation (real API key + webhook).
- i18n message catalogue entries (currently English-only inline copy).
- A11y sweep with axe — placeholder; will pile onto the next a11y pass.
