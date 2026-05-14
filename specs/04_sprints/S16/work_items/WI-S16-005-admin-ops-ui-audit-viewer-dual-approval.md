---
id: "WI-S16-005"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "STANDARD"
parent: "S-16"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "AUTH-MODEL"
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s16", "ui", "admin-ops", "audit-viewer", "cloudevents", "dual-approval", "pat-dual-approval-001", "standard"]
---

# WI-S16-005 — Admin Operations UI: **Audit Log Viewer Self-service** Query Proxy Backend `GET /api/audit?tenant_id=X&type=Y&since=Z` (Worker Reads CloudEvents R2 S-09 Audit Bucket via Signed Query; Filters por subject (user_hash) + type (event_type Enum) + ts (since/until); Pagination Cursor-based via R2 Query; **Export JSON Button** Audit-grade Evidence Sanitized — PAT Redacted + PII Allowlist; Render Table with Columns ts + type + subject_hash + action + outcome + details_truncated) + **Admin Op Submit Dual-approval Flow** (Reuse S-13 PAT-DUAL-APPROVAL-001 Collusion-rotation Rolling 3-op Window — Lote 10.13 Canonical): Admin User Submits Destructive Op (e.g., Tenant Delete + Retention Policy Change + BYOK Rotate); Fresh MFA ≤ 30 min Required (CTRL-AUTH-010); Second Admin Approves via Separate Session (Collusion-rotation Enforcement: Same Approver Max 3 Ops within Rolling Window); Op Queued + Audit Event + Slack Notification

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-16](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S16-005 |
| Título | Admin operations UI — audit log viewer self-service + dual-approval flow destructive ops. |
| Sprint | S-16 |
| Lane | STANDARD |
| Forcing factors | none (UI consume audit API S-09 e admin API S-13 já validated; dual-approval reuse S-13 PAT-DUAL-APPROVAL-001) |

## 1. Intent

Entregar **admin operations UI** com audit log viewer self-service per-tenant + dual-approval flow para destructive ops. Audit viewer reduces compliance auditor manual queries; dual-approval enforces collusion-rotation rolling 3-op window (Lote 10.13 canonical reuse).

```typescript
// File: apps/web/src/app/admin/audit/page.tsx
export default function AuditViewer() {
  const [filters, setFilters] = useState({ tenant_id: '', type: '', since: '' });
  const [cursor, setCursor] = useState<string | null>(null);

  const { data, error } = useSWR(`/api/audit?${qs(filters, cursor)}`, fetcher);

  return (
    <AuditViewerLayout>
      <FilterBar filters={filters} setFilters={setFilters} />
      <AuditTable rows={data?.rows ?? []} />
      <Paginator cursor={data?.next_cursor} setCursor={setCursor} />
      <ExportJsonButton filters={filters} />
    </AuditViewerLayout>
  );
}
```

## 2. Narrative

Audit log viewer self-service é Stripe/Auth0 baseline; CoreLink S-16 entrega parity + sanitized JSON export audit-grade. Admin ops dual-approval reuse S-13 PAT-DUAL-APPROVAL-001 collusion-rotation rolling 3-op window (Lote 10.13 canonical) — same approver max 3 ops within rolling window prevents single-admin abuse.

Backend Worker reads CloudEvents R2 (S-09 audit bucket) via signed query; filters por subject/type/ts; pagination cursor-based. UI rendering table com sanitized fields (PAT redacted; PII allowlist).

**Risk justification STANDARD lane**:
- UI consume audit API S-09 + admin API S-13 já validated.
- Dual-approval reuse S-13 PAT-DUAL-APPROVAL-001 (não introduz novo collusion-rotation pattern).
- Audit viewer é read-only consumer; sanitized output bounded surface.
- CTRL-AUTH-010 reuse (admin destructive ops require **WebAuthn step-up canonical** — não generic MFA; reuse S-03 cycle 9 WebAuthn flow + Clerk SDK `requireMFA({ method: 'webauthn' })`; Lote 10.16 codex P1 canonical fix; password+TOTP NÃO suficiente para admin destructive ops; WebAuthn fresh ≤ 30 min).

## 3. Customer Impact & Journey

**Persona 1 — Compliance auditor (cliente)**:
- Self-service audit log viewer per-tenant; reduces manual support queries.
- Filters por subject (user_hash) + type (event_type) + ts.
- JSON export audit-grade evidence (sanitized; PAT redacted; PII allowlist).
- SOC 2 + ISO 27001 + LGPD/GDPR audit trail forensic-grade.

**Persona 2 — Tenant admin**:
- Admin destructive ops require dual-approval (tenant delete, retention policy change, BYOK rotate).
- Same approver max 3 ops within rolling window (collusion-rotation enforcement).
- Fresh MFA ≤ 30 min (CTRL-AUTH-010).
- Slack notification per op submitted + per approval.

**Persona 3 — Internal SRE/Engineer**:
- Audit log queries scriptable via JSON export.
- Dual-approval audit trail forensic-grade for post-incident review.

## 4. Capability Mapping

- **CAP-UI-003** (Audit log viewer self-service) — IMPLEMENTA primary; CloudEvents R2 query proxy + filters + JSON export.
- Trace: `_spec_contract.md §4 + §5.2 (R-S16-5)` + `S-09` (CloudEvents R2 audit bucket) + `S-13` (admin plane API + PAT-DUAL-APPROVAL-001 collusion-rotation rolling 3-op window — Lote 10.13 canonical).

## 5. Tipo

Feature WI; STANDARD lane; admin ops UI.

## 6. Escopo

### 6.1 In-scope

1. **Audit log viewer self-service** em `apps/web/src/app/admin/audit/`:
   - Route `/admin/audit` (auth-gated; SSR; MFA fresh ≤ 30 min CTRL-AUTH-010).
   - Backend Worker query proxy `GET /api/audit?tenant_id=X&type=Y&since=Z&until=W&cursor=C`:
     - Reads CloudEvents R2 (S-09 audit bucket) via signed query.
     - Filters por subject (user_hash sha256), type (event_type enum), ts (since/until ISO 8601).
     - Pagination cursor-based (≤ 100 rows per page).
     - Sanitized fields: `ts`, `type`, `subject_hash`, `action`, `outcome`, `details_truncated` (≤ 200 chars).
     - PAT redacted explicit; PII allowlist (no email, no raw tenant_id).
   - **Filter bar**:
     - Subject (user_hash input).
     - Type dropdown (multi-select; from event_type enum CloudEvents schema).
     - Since/Until date pickers.
     - Apply filters button.
   - **Audit table**:
     - Columns: ts (ISO 8601) + type (enum) + subject_hash (truncated last 8 chars) + action + outcome (success/fail) + details_truncated.
     - Sorting: ts desc default.
     - Per-row expand for full details (sanitized JSON).
   - **Pagination cursor-based**:
     - Next/Prev buttons with cursor in URL.
     - "Load more" button increments cursor.
   - **Export JSON button**:
     - Triggers backend `GET /api/audit?...&format=json&export=true`.
     - Backend streams sanitized JSON to client (≤ 100MB max; pagination if larger).
     - Audit-grade evidence (PAT redacted; PII allowlist explicit).
     - Filename: `audit-log-{tenant_id_hash}-{since}-{until}.json`.

2. **Admin op submit dual-approval flow** (reuse S-13 PAT-DUAL-APPROVAL-001 — Lote 10.13 canonical):
   - Route `/admin/ops` (auth-gated; SSR; **WebAuthn fresh ≤ 30 min CTRL-AUTH-010 canonical** — Lote 10.16 codex P1 fix; admin destructive ops require WebAuthn factor specifically, não generic MFA; Clerk SDK enforces via `requireMFA({ method: 'webauthn', max_age_minutes: 30 })`).
   - Submit form fields:
     - Op type dropdown: `tenant_delete` / `retention_policy_change` / `byok_rotate` / `region_migrate` (extensible enum).
     - Op details textarea (free-text justification; required).
     - Tenant_id target (auto-filled current tenant or manual entry for cross-tenant admin).
   - Submit → backend POST `/api/admin/op/submit` payload `{op_type, details, tenant_id}`.
   - Op queued em backend admin plane (S-13 reuse) com status `pending_approval`.
   - Slack notification per op submitted (channel `#admin-ops-coreLink`; reuse existing Slack workflow).
   - Audit event S-09 emitted `admin_op_submitted`.

3. **Approval flow**:
   - Second admin user navigates `/admin/ops/pending`; sees queued ops list.
   - Per row: op_type + submitter_hash + details + submitted_ts + approve/reject buttons.
   - Approve button:
     - Fresh MFA ≤ 30 min CTRL-AUTH-010 verified.
     - Backend POST `/api/admin/op/{id}/approve`:
       - Validates collusion-rotation: same approver cannot approve more than 3 ops within rolling 3-op window (last 3 approved ops from this approver; reject if approver_count_in_window >= 3).
       - If collusion check passes, op status `approved` → executed by backend.
     - Audit event S-09 emitted `admin_op_approved`.
     - Slack notification.
   - Reject button:
     - Backend POST `/api/admin/op/{id}/reject` payload `{reason}`.
     - Op status `rejected`; submitter notified.
     - Audit event S-09 emitted `admin_op_rejected`.

4. **Collusion-rotation enforcement** (S-13 PAT-DUAL-APPROVAL-001 reuse):
   - Backend tracks last 3 approvals per approver (rolling window).
   - If same approver attempts 4th approval within window, backend rejects: "Collusion rotation: max 3 ops per approver within rolling window. Wait for next window."
   - UI displays warning if approver near limit ("You have approved 2/3 ops in current window").
   - Audit event `admin_op_collusion_rejected` emitted.

5. **Métricas Prometheus**:
   - `corelink_admin_ui_audit_query_total{outcome}` counter (outcome ∈ ok|fail).
   - `corelink_admin_ui_audit_export_total` counter.
   - `corelink_admin_ui_admin_op_submit_total{op_type, outcome}` counter.
   - `corelink_admin_ui_admin_op_approve_total{outcome}` counter (outcome ∈ approved|rejected|collusion_rejected).
   - Cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (NUNCA per-tenant labels).

6. **Slack notifications integration**:
   - Webhook URL stored em CF Workers secret.
   - Per submission + approval + rejection event.
   - Sanitized payload (no PII; no raw tenant_id; references via hash).

### 6.2 Out-of-scope (deferred)

- Component library + a11y full (WI-S16-006).
- Closing PRR + Lighthouse + UX workshop (WI-S16-007).
- Backend admin plane API (S-13).
- Backend audit events R2 bucket (S-09).
- Internal CoreLink ops admin panel `admin.corelink.dev` separate subdomain.

## 7. Anti-Scope

- Audit viewer expose PAT raw (CTRL-CRED-001 violation).
- Audit viewer expose PII raw (CTRL-PRIV-001 violation).
- Admin op submit sem fresh **WebAuthn step-up** (CTRL-AUTH-010 canonical violation; password/TOTP NÃO suficiente — Lote 10.16 codex P1 fix).
- Dual-approval bypass (single-admin destructive op = security gap).
- Same approver bypass collusion-rotation 3-op window.
- Slack notification with raw tenant_id (PII leak).
- JSON export with non-sanitized fields.
- Cross-tenant audit access without admin scope (auth scoping gap).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Admin operations UI — audit viewer + dual-approval

  Scenario: Audit viewer query proxy reads CloudEvents R2
    Given user authenticated com admin scope + MFA fresh ≤ 30 min
    When user navigates /admin/audit
    Then backend Worker query proxy reads CloudEvents R2 S-09 audit bucket
    And filters por subject + type + ts
    And pagination cursor-based ≤ 100 rows per page

  Scenario: Audit table renders sanitized fields
    Given audit query returns rows
    When table rendered
    Then columns: ts + type + subject_hash + action + outcome + details_truncated
    And PAT redacted; PII allowlist (no email, no raw tenant_id)
    And sorting ts desc default

  Scenario: Filters subject + type + ts apply
    Given user enters filters: subject_hash="abc...123", type="dsr_submitted", since="2026-04-01"
    When apply filters clicked
    Then table re-renders with matching rows only
    And cursor reset

  Scenario: Export JSON audit-grade
    Given audit query results visible
    When user clicks Export JSON
    Then backend streams sanitized JSON to client
    And filename "audit-log-{tenant_id_hash}-{since}-{until}.json"
    And PAT redacted explicit; PII allowlist

  Scenario: Admin op submit requires fresh WebAuthn (Lote 10.16 codex P1 canonical)
    Given user > 30 min stale
    When user clicks submit op
    Then redirect to MFA re-prompt CTRL-AUTH-010
    When MFA verified, submit proceeds

  Scenario: Admin op submit creates pending approval
    Given fresh WebAuthn step-up (≤ 30 min) + admin scope
    When user submits op (e.g., tenant_delete + details)
    Then backend creates op pending_approval status
    And Slack notification emitted (sanitized; no raw tenant_id)
    And audit event admin_op_submitted emitted (S-09)

  Scenario: Approval requires second admin + fresh WebAuthn step-up
    Given op pending_approval exists
    When second admin (different from submitter) navigates /admin/ops/pending
    Then op visible em queue
    When second admin clicks approve com fresh WebAuthn step-up (≤ 30 min CTRL-AUTH-010)
    Then collusion-rotation check passes (within rolling 3-op window)
    And op status approved
    And executed by backend
    And audit event admin_op_approved emitted

  Scenario: Collusion-rotation enforcement (S-13 reuse)
    Given approver X approved 3 ops within rolling window
    When approver X attempts 4th approval
    Then backend rejects "Collusion rotation: max 3 ops per approver within rolling window"
    And UI displays error message
    And audit event admin_op_collusion_rejected emitted

  Scenario: Same submitter cannot approve own op
    Given submitter X submitted op
    When submitter X attempts approve
    Then backend rejects "Self-approval not allowed; second admin required"

  Scenario: Reject flow
    Given op pending_approval
    When second admin clicks reject + reason
    Then op status rejected
    And submitter notified
    And audit event admin_op_rejected emitted

  Scenario: Slack notification sanitized payload
    Given op submit + approval + reject events
    When Slack notification sent
    Then payload has no raw tenant_id; no email; no PAT
    And references via hash only

  Scenario: Audit query respects per-tenant scope
    Given user X authenticated em tenant A
    When user queries audit with tenant_id=B (different tenant)
    Then backend rejects "Cross-tenant audit access denied"
    Unless user has cross-tenant admin scope (separate role)
```

## 9. Design Decisions

### 9.1 Why CloudEvents R2 read via signed query (não direct DB query)

- CloudEvents R2 is canonical S-09 audit bucket; immutable + cost-efficient.
- Signed query via Worker = scoped access + audit trail of audit queries.

### 9.2 Why pagination cursor-based (não offset)

- Cursor stable across writes; no skipped/duplicated rows on R2 list ops.
- Offset-based pagination O(N) cost per page; cursor O(1).

### 9.3 Why JSON export sanitized (não raw)

- Audit-grade evidence shareable em compliance reviews.
- Sanitization (PAT redacted + PII allowlist) prevents secondary leak.
- Streaming response (≤ 100MB max; pagination if larger).

### 9.4 Why dual-approval reuse S-13 PAT-DUAL-APPROVAL-001 (não new pattern)

- S-13 Lote 10.13 canonical; collusion-rotation rolling 3-op window already validated.
- Reuse single source of truth (no per-WI drift).

### 9.5 Why Slack notification (não Email)

- Slack: real-time visibility para admin team; faster response.
- Email backup via SES if Slack down (S-09 audit event always emitted).

### 9.6 Why MFA fresh ≤ 30 min (CTRL-AUTH-010 reuse)

- Reuses S-03 baseline; consistency across destructive ops.
- 30 min balance: user convenience vs identity re-auth assurance.

### 9.7 ADR potencial?

- Não. Patterns reused (CloudEvents R2 query + S-13 dual-approval + Clerk MFA + Slack webhook). No novel architecture decision.

## 10. Completeness Criteria

- [ ] **10.s16.005.1** Audit viewer query proxy reads CloudEvents R2 S-09 (EVT-018).
- [ ] **10.s16.005.2** Filters subject + type + ts; pagination cursor-based.
- [ ] **10.s16.005.3** Audit table sanitized fields (PAT redacted; PII allowlist).
- [ ] **10.s16.005.4** Export JSON audit-grade evidence sanitized.
- [ ] **10.s16.005.5** Admin op submit requires fresh MFA CTRL-AUTH-010 (EVT-002).
- [ ] **10.s16.005.6** Dual-approval flow reuse S-13 PAT-DUAL-APPROVAL-001.
- [ ] **10.s16.005.7** Collusion-rotation rolling 3-op window enforced backend.
- [ ] **10.s16.005.8** Slack notifications sanitized payload.
- [ ] **10.s16.005.9** Audit events S-09 emitted (submit + approve + reject + collusion_rejected).
- [ ] **10.s16.005.10** Cross-tenant audit access scoped per role.

## 11. DoD

- [ ] Audit viewer route deployed staging.
- [ ] Filters + pagination + JSON export tested.
- [ ] Admin op submit + dual-approval flow tested.
- [ ] Collusion-rotation enforcement validated (4th approval rejected).
- [ ] Slack notifications integrated em staging webhook.
- [ ] Cross-tenant scope enforced.
- [ ] Tests: unit (audit query proxy + dual-approval check + collusion validator) + integration (E2E audit viewer + dual-approval flow) + 4+ negative scenarios.

## 12. Invariants Validated

- **CTRL-AUTH-010** (admin destructive ops fresh MFA ≤ 30 min) reforced em admin op submit + approve.
- **CTRL-CRED-001** (no PAT em client logs) reforced via audit viewer redaction.
- **CTRL-PRIV-001** (zero PII em client logs) reforced via audit viewer PII allowlist.
- **PAT-DUAL-APPROVAL-001** (collusion-rotation rolling 3-op window — S-13 Lote 10.13 canonical) reforced em approval flow.
- **Não introduz INVs novas** (UI é consumer das backend invariants S-09/S-13; per spec contract §8).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Audit viewer page | `apps/web/src/app/admin/audit/page.tsx` | TypeScript |
| Audit query API route | `apps/web/src/app/api/audit/route.ts` | TypeScript |
| Audit JSON export route | `apps/web/src/app/api/audit/export/route.ts` | TypeScript |
| Admin ops submit page | `apps/web/src/app/admin/ops/page.tsx` | TypeScript |
| Admin ops pending approval page | `apps/web/src/app/admin/ops/pending/page.tsx` | TypeScript |
| Admin op API routes | `apps/web/src/app/api/admin/op/*/route.ts` | TypeScript |
| Slack webhook integration | `apps/web/src/lib/slack-webhook.ts` | TypeScript |
| Filter bar component | `apps/web/src/components/audit-filters.tsx` | TypeScript |

## 14. Quality Standards

- **14.s16.005.1** Audit query p95 latency ≤ 2s on cursor pagination.
- **14.s16.005.2** Test coverage ≥ 85% (audit query + dual-approval + collusion validator + Slack integration).
- **14.s16.005.3** SAST: zero CVEs HIGH/CRITICAL em deps.
- **14.s16.005.4** Audit log viewer query proxy + filters + export tested staged 30d (per Completeness Criterion 10.s16.7).
- **14.s16.005.5** Sanitized JSON export verified (PAT regex 0 matches; PII allowlist enforced).
- **14.s16.005.6** Collusion-rotation enforcement integration test (backend rejects 4th approval).

## 15. Test Plan

### Unit tests (≥ 85% coverage)
- Audit query: filters parse + R2 signed query + pagination cursor.
- Sanitization: PAT regex redacted; PII allowlist enforced.
- Dual-approval check: same submitter cannot approve; second admin required.
- Collusion validator: approver_count_in_window check.
- Slack webhook: sanitized payload assembly.

### Integration tests (E2E staging)
- Full /admin/audit flow: query → table → filters → export.
- Full /admin/ops flow: submit → pending → approve (different admin) → execute.
- Collusion-rotation: simulate 4 approvals same approver; 4th rejected.
- Cross-tenant scope: tenant A user cannot query tenant B audit (without admin scope).

### Negative scenarios (≥ 4)
1. **Cross-tenant audit access without admin scope**: backend rejects.
2. **PAT em audit row raw**: sanitization regex catches; redacted as `<redacted>`.
3. **Self-approval attempt**: backend rejects "Self-approval not allowed".
4. **Collusion-rotation 4th approval**: backend rejects + audit event emitted.
5. **MFA stale > 30 min em admin op submit**: redirect to MFA re-prompt.
6. **JSON export > 100MB**: pagination required; backend streams chunks.

### Cross-browser
- Smoke test em Chrome + Firefox + Safari + Edge; full matrix em WI-S16-007.

## 16. Failure Modes

- **FM-150** (transient API): audit query retry com exponential backoff (3 retries default); progress indicator clear.
- **FM-160** (auth invalid): clear error UI; redirect to Clerk sign-in flow.
- **Collusion-rotation rejected**: clear error message; audit event emitted.

## 17. Controls

- **CTRL-AUTH-010** (admin destructive ops fresh MFA ≤ 30 min): admin op submit + approve.
- **CTRL-CRED-001** (no PAT em client logs): audit viewer PAT redaction sanitization.
- **CTRL-PRIV-001** (zero PII em client logs): audit viewer PII allowlist.
- **PAT-DUAL-APPROVAL-001** (S-13 reuse; collusion-rotation rolling 3-op window): approval flow.

## 18. Resilience Patterns

- Retry transient errors (FM-150): exponential backoff em audit query + admin op submit.
- Slack webhook fallback to email via SES if webhook down.
- Audit event S-09 always emitted (forensic-grade trail).

## 19. Observability

UI métricas Prometheus snake_case:
- `corelink_admin_ui_audit_query_total{outcome}` counter (outcome ∈ ok|fail).
- `corelink_admin_ui_audit_query_duration_ms` histogram (p95 ≤ 2s).
- `corelink_admin_ui_audit_export_total` counter.
- `corelink_admin_ui_admin_op_submit_total{op_type, outcome}` counter.
- `corelink_admin_ui_admin_op_approve_total{outcome}` counter (outcome ∈ approved|rejected|collusion_rejected).
- `corelink_admin_ui_collusion_rejected_total` counter (alert > 0; SEV-3).

Cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (NUNCA per-tenant labels).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: Clerk SDK + MFA fresh ≤ 30 min CTRL-AUTH-010 em admin ops; CSRF tokens.
- **Tampering**: audit events S-09 immutable R2; signed queries; JSON export sanitized.
- **Repudiation**: audit events emitted (submit + approve + reject + collusion_rejected); Slack notifications.
- **Information disclosure**: CTRL-CRED-001 (PAT redacted); CTRL-PRIV-001 (PII allowlist); cross-tenant access scoped.
- **DoS**: audit query rate-limited (≤ 100/min per user); JSON export streamed.
- **Elevation of privilege**: dual-approval enforced; collusion-rotation rolling 3-op window.

**LINDDUN delta**:
- **Linkability**: audit subject_hash (sha256); tenant_id_hash em Slack payloads.
- **Identifiability**: audit table sanitized; user_id_hash references only.
- **Non-repudiation**: audit events forensic-grade R2 immutable.
- **Detectability**: collusion-rotation alerts; admin op anomaly tracked.
- **Disclosure**: PAT NUNCA em audit table ou JSON export; PII allowlist explicit.
- **Non-compliance**: SOC 2 + ISO 27001 + LGPD/GDPR audit trail; LINDDUN review committed em `specs/_audits/2026-XX-XX-linddun-admin-ops-ui.md`.

## 21. Dependencies

### Hard blockers
- WI-S16-001 SEALED (skeleton + Clerk + CSP + i18n).
- WI-S16-002 SEALED (PAT mgmt patterns reused).
- S-09 SEALED (CloudEvents R2 audit bucket).
- S-13 SEALED (admin plane API + PAT-DUAL-APPROVAL-001 collusion-rotation rolling 3-op window — Lote 10.13 canonical).

### Soft blockers
- Slack webhook URL configured em staging.

### Outbound
- WI-S16-006 (component library reuses table + filter bar patterns).
- WI-S16-007 (E2E admin ops + closing PRR).

## 22. Effort PERT

O: 10h, M: 16h, P: 26h → PERT **16.7h** (per spec contract §12; audit viewer + filters + export + dual-approval + collusion-rotation).

## 23. Cost Analysis

- R2 audit query: ~$0.36/GB read + Class A ops; estimated $5/mês.
- Slack webhook: free.
- Worker compute: included em CF Workers free tier.
- Total: ~$5/mês incremental.

## 24. Post-mortem Hooks

- Audit viewer expose PAT raw → CRITICAL post-mortem + Security review.
- Cross-tenant audit access leak → CRITICAL post-mortem + AppSec review.
- Dual-approval bypass detected → CRITICAL post-mortem + AppSec review.
- Collusion-rotation 3-op window bypass → CRITICAL post-mortem + Security review.
- Slack notification PII leak → SEV-2 post-mortem + Privacy review.

## 25. Rollback / Recovery

Audit viewer regression detected → revert via CF Pages rollback; backend S-09 + S-13 unchanged (no data loss).

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | PAT em audit row raw | L | L | HIGH | L | LOW | Sanitization regex CI; integration test |
| R-002 | Cross-tenant audit access | L | M | CRITICAL | M | LOW | Backend auth scoping per tenant_id; integration test |
| R-003 | Dual-approval bypass | L | M | CRITICAL | M | LOW | Backend collusion-rotation check (S-13 reuse); integration test |
| R-004 | Slack PII leak | L | L | MEDIUM | L | LOW | Sanitized payload allowlist; integration test |
| R-005 | JSON export memory exhaustion | L | L | MEDIUM | L | LOW | Stream response; ≤ 100MB max |
| R-006 | Collusion-rotation rolling window state drift | L | M | HIGH | M | LOW | Backend canonical state; UI is reflection only |

## 27. Knowledge Transfer

- Tech talk (1h): "CoreLink Admin Ops UI — Audit Viewer + Dual-Approval".
- Doc `docs/internal/admin-ops-ui.md` — overview + S-13 PAT-DUAL-APPROVAL-001 reuse rationale.
- Onboarding test (3 questions): audit query sanitization + collusion-rotation rolling 3-op window + dual-approval same-submitter check.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Frontend Lead | _TBD; emphatic — audit viewer + admin ops UI + dual-approval flow_ | _pending_ |
| 4 | QA | _TBD; emphatic — E2E audit + collusion-rotation tests + cross-tenant scope_ | _pending_ |
| 5 | Product | Gustavo Schneiter | _pending_ |
| 6 | Designer/a11y advisor | _TBD; emphatic — filter bar UX + table accessibility + JSON export UX_ | _pending_ |
| 7 | Privacy officer | _TBD; emphatic — audit sanitization + PII allowlist + LINDDUN review + Slack PII gap check_ | _pending_ |

> STANDARD lane (per framework §33.5.4): 5-8 canonical sign-offs; 7 typical.

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S16-005 (cycle 12.S16.0; admin ops UI + audit viewer + dual-approval reuse S-13 PAT-DUAL-APPROVAL-001). |

## 30. Anti-patterns evitados

- Audit viewer expose PAT raw (CTRL-CRED-001 violation).
- Audit viewer expose PII raw (CTRL-PRIV-001 violation).
- Admin op submit sem fresh **WebAuthn step-up** (CTRL-AUTH-010 canonical violation; password/TOTP NÃO suficiente — Lote 10.16 codex P1 fix).
- Dual-approval bypass single-admin destructive op.
- Same approver bypass collusion-rotation 3-op window.
- Self-approval em mesmo op (submitter = approver).
- Slack notification with raw tenant_id (PII leak).
- JSON export with non-sanitized fields.
- Cross-tenant audit access without admin scope (auth scoping gap).
- Pagination offset-based (consistency drift on writes).

---

**Fim WI-S16-005.**
