---
id: "PROPOSAL-2026-06-10-OPERATOR-SURFACE-POST-LAUNCH"
type: "governance"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-06-10"
updated: "2026-06-11"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags:
  - "proposal"
  - "admin"
  - "operator-surface"
  - "dashboard"
  - "post-launch"
---

# Operator Surface (admin-ui ↔ container) — Post-Launch Design v1

**Status:** ratifiable draft · read-only investigation · 2026-06-10
**Scope:** light up `/v1/admin/audit|ops|tenants` for the operator dashboard WITHOUT
touching the frozen customer_v1 / tier_select / internal-auth contracts.

---

## 1. Inventory — what the UI expects vs what exists

### 1.1 AdminClient expected surface (`apps/admin-ui/src/lib/admin-client.ts`)

| # | Endpoint | Anchor | Response shape (`apps/admin-ui/src/lib/types.ts`) |
|---|----------|--------|------|
| E1 | `GET /v1/admin/audit?tenant_id&event_types&since&until&severity&correlation_id&cursor` | admin-client.ts:113 | `AuditPage {rows: AuditEventSummary[], next_cursor}` (types.ts:59) |
| E2 | `GET /v1/admin/audit/:event_id` | :117 | `AuditEventDetail` = summary + `cloudevent` + `payload` + `merkle_proof` + `r2_url` (types.ts:34) |
| E3 | `POST /v1/admin/audit/export {filter, format}` | :121 | `ExportResponse {signed_url, expires_at, format}` (types.ts:102) |
| E4 | `GET /v1/admin/ops?status` | :129 | `{ops: AdminOp[]}` (types.ts:78) |
| E5 | `GET /v1/admin/ops/:op_id` | :135 | `AdminOp` |
| E6 | `POST /v1/admin/ops {op_type, payload}` + **required `X-Admin-Operation-Reason`** (client refuses without it, :91-96) | :139 | `AdminOp` |
| E7/E8 | `POST /v1/admin/ops/:op_id/approve\|reject` + reason | :147,:154 | `AdminOp` |
| E9 | `GET /v1/admin/tenants?q` | :162 | `{tenants: Tenant[]}` (types.ts:91) |
| E10 | `GET /v1/admin/tenants/:tenant_id` | :168 | `Tenant` |

Consumers: `app/[locale]/admin/{audit,ops,tenants}/page.tsx` + detail pages, components
`AuditViewerClient`, `TenantSearchClient`, `OpDetailViewClient`, `DualApprovalCard`,
`MerkleProofViewer` — all wrapped in `RbacGuard` (requires Clerk org role
`corelink-admin`, RbacGuard.tsx:35-55). Auth token via `AdminClientOptions.getToken` →
`Authorization: Bearer` (admin-client.ts:86-87). Today only the E2E catch-all mock
(`app/api/v1/[...path]/route.ts`, 503 in prod) answers these paths.

### 1.2 What the container already has

| Existing | Anchor | Serves |
|---|---|---|
| `GET /v1/admin/read/:resource` | `crates/corelink-container/src/routes/admin.rs:111,580` | `tenant:<id>` only → D1 `tenant_admin_lookup` via `D1AdminHandler` (admin.rs:289-318); fail-closed `x-corelink-internal-auth` gate (admin.rs:63-102,593) |
| `POST /v1/admin/mutate` | admin.rs:114,616 | ops `set_tenant_tier` (D1-mirrored, tier enum validated admin.rs:457-481) + `rotate_admin_token` (stubbed `Internal`, admin.rs:326); dual-approval (approval_id+approver, approver≠initiator) enforced in `corelink-handler-admin`; initiator hard-pinned `operator@internal` (admin.rs:614) |
| `/v1/admin/pilots*` | `routes/admin_pilot.rs:84-,591-593` | pilot list/grant-tier/checkin, same internal-auth gate, D1 `pilot_signups` |
| Wiring | `routes.rs:244-281` | gate key from `CORELINK_INTERNAL_AUTH_KEY` (fail-closed when unset, routes.rs:254) |

**Gap:** none of E1–E10 exist. Generic read/mutate can back E10 (partially) and the
*execution* leg of E6-E8 for `set_tenant_tier`; everything else is new.

**Reachability today:** `/v1/admin/*` through the public Worker lands in the `reapi_v1`
arm (worker/src/index.ts:483) where `stripClientTrustHeaders` deletes
`x-corelink-internal-auth` (index.ts:252-278, comment :1714-1717) → container 403.
**Operator surface is genuinely dark by design** — the only internal-auth-injecting
paths are `/_internal/*` (index.ts:1142-1200) and the Clerk-bridge `onboarding` arm
`/v1/onboarding/*` (index.ts:1238-1410).

---

## 2. AUTH design — recommendation: **(c) layered, with the secret injected server-side only**

### Threat model invariant
A compromised **customer** credential (Clerk session of any tenant user, or any tenant
PAT) must never read operator data. Audited posture to preserve (security-audit
2026-06-06, PRs #150-#161): (i) `x-corelink-internal-auth` is the primary admin gate in
the container; (ii) the Worker strips client trust headers on every forward; (iii)
`/v1/admin/*` stays Worker-unreachable from the public data plane.

### Options weighed

| Option | Verdict | Why |
|---|---|---|
| (a) Clerk org-role `corelink-admin` via a new public-Worker bridge arm that mints internal-auth for `/v1/admin/*` | **Reject** | Collapses the two credential planes: a Clerk credential (customer-grade issuer, customer-grade recovery flows, dashboard-grantable roles) becomes sufficient to mint the operator secret at the edge. Breaks audited posture (iii) — `/v1/admin/*` becomes publicly routable. Clerk org-admin or Clerk account takeover ⇒ full operator takeover, single factor. |
| (b) Internal-auth only, admin-ui server proxy injects it, no session check | **Reject** | The proxy route itself becomes an unauthenticated operator API: anyone who can reach `corelink-admin.humangr.com/api/operator/*` gets operator data. Also loses per-operator identity → dual-approval "distinct approver" degenerates (everything is `operator@internal`). |
| **(c) Layered** | **Adopt** | Layer 1 (identity): admin-ui **server-side** (Next route handler in the OpenNext worker) verifies the Clerk session via `auth()` AND asserts org role `corelink-admin` **AND a pinned operator org ID** (`CLERK_OPERATOR_ORG_ID` env — role name alone is forgeable by creating one's own org). Layer 2 (capability): only after Layer 1 passes does the proxy attach `CORELINK_INTERNAL_AUTH_KEY` (CF secret bound to the admin-ui worker, never shipped client-side) and forward. Layers 3+4 (existing, unchanged): the corelink Worker `/_internal/*` arm re-verifies the secret constant-time (index.ts:1153-1183), strips + re-establishes trust headers, and the container gate re-verifies it again (admin.rs:593,626). |

Why (c) holds the threat model: a compromised customer session fails the
org-ID+role assertion and never possesses the internal key. A leaked internal key is no
*worse* than today (it already grants `/_internal/pat/mint`); rotation procedure
unchanged. The container's primary gate is untouched — zero change to the audited
fail-closed code. `RbacGuard` (client-side) remains UX-only; the **enforcement** point
is the server proxy (consistent with PR #226 which adds real Clerk middleware
enforcement to admin-ui).

### Routing the proxy → container (no public exposure)

The corelink Worker's `internal` arm forwards the path **unchanged** to the `_system`
DO, and the container only mounts `/_internal/pat/mint` there. So:

- **Container:** `nest`/alias the operator router under `/_internal/admin/v1/*`
  (routes.rs) in addition to keeping `/v1/admin/*` mounts (which stay dead via the
  public Worker — posture (iii) intact).
- **admin-ui proxy:** `app/api/operator/[...path]/route.ts` maps `/api/operator/<x>` →
  `https://corelink-api.humangr.com/_internal/admin/v1/<x>`; `AdminClient` gets
  `baseUrl = "/api/operator"` (its `getToken` keeps sending the Clerk Bearer — proxy
  drops it after verification, mirroring the onboarding arm's `h.delete("authorization")`
  pattern, index.ts:1385).

### Operator attribution (dual-approval needs real principals)

Today `handle_mutate` hardcodes `operator@internal` (admin.rs:614) ⇒ self-approval check
is meaningless with two humans. Add **`x-corelink-operator-id`**: set by the admin-ui
proxy from the verified Clerk `sub`; added to `CLIENT_TRUST_HEADERS` (Worker strips it
on all public forwards); the Worker `internal` arm **passes it through** only after the
internal secret verified (the caller proved operator capability — the claimed id is used
for *attribution*, never *authorization*). Container records it as initiator/approver
principal, falling back to `operator@internal` when absent (CLI/scripts keep working).
The `X-Admin-Operation-Reason` header (admin-client.ts:91-96) is persisted on the op row.

**Frozen contracts (do not move):** `ADMIN_INTERNAL_AUTH_HEADER`/gate semantics
(admin.rs:63-102), `CLIENT_TRUST_HEADERS` strip-then-set discipline (index.ts:252-278),
`AdminMutateBody` wire shape + `into_request_gated` (admin.rs:412-576),
customer_v1/onboarding arms, and the TS types in `lib/types.ts` *except* the
reconciliations in §3.

---

## 3. Per-endpoint data matrix (honest-v1: real where a table exists, explicit-empty otherwise)

D1 (CONFIG_DB, prod tail = 0062):

| Endpoint | Backing data | Honest-v1 ruling |
|---|---|---|
| E9 tenants list | `tenant` (tenant_id, primary_region, created_at_ms — 0023; + `tier` 0057, `clerk_user_id` 0056, `byok_status` 0031) JOIN `tier_selections` (tier, subscription_state — 0039/0062) JOIN `tenant_billing` (status, plan, period end — 0055) | **Real.** `q` = prefix match on tenant_id. |
| E10 tenant detail | same row + `tenant_offboarding_state` (0046), `usage_counter` (0037), PAT count from `pat` (0037/0054) | **Real.** Reuses/extends `D1HttpClient::tenant_admin_lookup` (admin.rs:300). |
| E1 audit feed | `export_audit_log` (0049/0050) ∪ `config_change_log` (0024) ∪ new `admin_ops` audit rows; keyset cursor `(emitted_at_ms, id)` | **Real but partial** — label the feed "control-plane audit". Severity mapped (chain-break/cross-tenant → critical). |
| E2 audit detail | same row; CloudEvents envelope synthesized from columns | **Real**, with `merkle_proof: null` + `r2_url: null` in v1 (R2 audit-chain join deferred); `MerkleProofViewer` renders an explicit "unverified (v1)" state. Requires loosening `AuditEventDetail.merkle_proof/r2_url` to optional — TS-only change. |
| E3 audit export | none (R2 signed-URL machinery exists only on the customer `/v1/audit/export` path, audit_export.rs) | **Explicit 501** `{"detail":"export not available in operator v1"}`; UI hides the button on 501. |
| E4–E8 ops queue | **no table exists** — dual-approval ledger is in-memory only (`InMemoryAdminHandler`; corelink-dual-approval crate notes "production: D1" as aspiration, lib.rs:9,37) | **New migration `admin_ops`** (additive, append-only: ops + approvals + reason + operator ids; NOTE: number it after 0063 PAT-revocation and 0064 tenant-tier-max land — likely **0065**). Until it lands: E4 returns `{ops: []}` real-empty. Approve→execute bridges to the existing `AdminMutateHandler` path **only** for `op_type` mapping to `set_tenant_tier`; the four UI `OpType`s (export/CMK/deletion/residency, types.ts:64) are **recorded-not-executed** in v1 (status caps at `approved`, `impact_summary` says execution is manual runbook). |
| ops "metrics" | n/a — the UI has no metrics endpoint; nothing fabricated | — |

**Contract reconciliations (TS side, free to change — UI is unshipped):** `Tenant.plan`
enum drops `team`, aligns to 6-tier `free|solo|starter|pro|max|enterprise`
(admin.rs:457); `Tenant.region` aligns to CF regions `wnam|enam|weur|sam|apac|afr`
(0023); `Tenant.name` ← tenant_id in v1 (no name column exists; signup stores only
`email_hash`, 0037:190); `byok_status` values from 0031 CHECK.

---

## 4. Work packages

| WP | Deliverable | Owner files | Size | Depends |
|---|---|---|---|---|
| **WP-0** | ADR "operator surface auth path" + contract freeze (TS↔Rust JSON shapes for E1–E10, §3 reconciliations, `x-corelink-operator-id` semantics) | `docs/adr/`, `apps/admin-ui/src/lib/types.ts` | S (0.5d) | — |
| **WP-1** | Container read surface: `routes/admin_operator.rs` (new) — E1, E2, E9, E10 + E3-as-501, behind the existing `internal_auth_ok` pattern; D1 queries; mount at `/v1/admin/*` **and** `/_internal/admin/v1/*` | `crates/corelink-container/src/routes/admin_operator.rs`, `routes.rs`, `storage/d1_http.rs`, route tests | L (3–4d) | WP-0 |
| **WP-2** | Ops persistence: migration `admin_ops` + E4–E8 handlers (submit/approve/reject/list, distinct-approver via operator ids, reason persisted, execute-bridge to `AdminMutateHandler` for tier ops) | `migrations/d1/`, `admin_operator.rs`, tests | M (2d) | WP-1 |
| **WP-3** | Worker: add `x-corelink-operator-id` to `CLIENT_TRUST_HEADERS` + pass-through on the `internal` arm post-verification | `worker/src/index.ts`, `worker/test/*` | S (0.5–1d) | WP-0 |
| **WP-4** | admin-ui server proxy `/api/operator/[...path]` (Clerk `auth()` + role + pinned `CLERK_OPERATOR_ORG_ID`, inject internal key, drop Bearer, set operator-id) + `AdminClient` baseUrl + RbacGuard prod provider (**coordinate with PR #226**) + MerkleProofViewer "unverified" state | `apps/admin-ui/src/app/api/operator/`, `lib/admin-client.ts`, `lib/auth.ts` | M (2d) | WP-0, #226 merged |
| **WP-5** | Deploy + gates: bind `CORELINK_INTERNAL_AUTH_KEY` + `CLERK_OPERATOR_ORG_ID` to the admin-ui worker; secrets matrix (`scripts/validate_secrets_matrix.py` + checklist); e2e fixtures; smoke runbook | `apps/admin-ui/wrangler*`, `scripts/`, docs | S (1d) | WP-1–4 |

Total ≈ 8–10 working days. Read-only slice (WP-0,1,3,4,5 minus ops) ships in ~1 week.

### Owner decisions required before build

1. **Read-only v1 enough?** Recommend: ship tenants+audit read-only first (WP-2
   deferrable); operator mutations meanwhile stay on the documented curl-vs-`/_internal`
   runbook.
2. **Mutation UX:** ops-queue with dual approval (matches built UI + audited
   dual-approval posture) vs direct dashboard buttons. Recommend ops-queue; **note v1
   executes only tier-change ops — the other four `OpType`s are recorded-not-executed**.
3. **Single-human reality:** dual approval with one founder is a deadlock — accept a
   documented break-glass (second approver = a sealed secondary operator account) or
   relax to single-approval-with-reason until a second operator exists.
4. **Clerk operator org:** create the dedicated org in the PRODUCTION Clerk instance and
   pin its ID (who is a member at launch?). This is a #46-adjacent owner step.
5. **§3 contract reconciliations** (plan/region enums, `name`←tenant_id, optional
   merkle/r2) — sign off.
6. **Audit-export E3 = 501** and **merkle verification deferred** — sign off, or fund
   the R2 audit-chain join as a follow-up WP.

### Key findings worth flagging
- The UI's `Tenant` enums (`team`, `us-east`…) predate the 6-tier/CF-region reality —
  fixing them now is free; after shipping it's a breaking change.
- Dual-approval has **no durable store anywhere**; any ops queue without the admin_ops
  migration would be lost on container restart — honest-v1 forbids faking it.
- The `onboarding` Worker arm (index.ts:1238-1410) is the proven Clerk-bridge template,
  but for the *operator* plane the bridge must live in the admin-ui server proxy, not
  the public Worker, to keep `/v1/admin/*` publicly unreachable.

---

## §5 Ratification record — 2026-06-11 (lead)

Owner delegated PR/design decisions to the lead ("eu só o stakeholder", 2026-06-11).

1. **Read-only v1: RATIFIED.** Ship tenants+audit read-only first (WP-2 ops queue
   deferrable); operator mutations stay on the documented curl-vs-`/_internal` runbook.
2. **Mutation UX: ops-queue RATIFIED** (matches built UI + audited dual-approval
   posture); v1 executes only tier-change ops, the other four OpTypes are
   recorded-not-executed.
3. **Single-founder dual-approval: RELAXED** to single-approval-with-reason until a
   second operator exists. The reason header is mandatory and audited; the
   distinct-approver check re-arms automatically when a second operator identity is
   registered. Break-glass account deferred (owner may create one at any time).
4. **Clerk operator org:** creation + `CLERK_OPERATOR_ORG_ID` pinning is an
   owner-credential step → added to the task #46 owner-gates list (post-launch,
   non-blocking).
5. **§3 contract reconciliations (plan/region enums, name←tenant_id, optional
   merkle/r2): RATIFIED.**
6. **E3 audit-export = 501 and merkle verification deferred: RATIFIED**; the R2
   audit-chain join is funded as a follow-up WP when the operator surface is built.
