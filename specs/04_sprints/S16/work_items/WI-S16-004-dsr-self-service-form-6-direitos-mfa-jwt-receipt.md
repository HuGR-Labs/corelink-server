---
id: "WI-S16-004"
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
  - "PRIVACY-MODEL"
  - "AUTH-MODEL"
  - "SECURITY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s16", "ui", "dsr", "lgpd", "gdpr", "ctrl-auth-010", "mfa", "jwt-receipt", "sla-clock", "standard"]
---

# WI-S16-004 — DSR Self-service Form com **6 Direitos** LGPD Art. 18 + GDPR Art. 15-22 (Direito #1 Access LGPD Art. 18 II + GDPR Art. 15 — User Receives All Dados Pessoais Structured; Direito #2 Correction LGPD Art. 18 III + GDPR Art. 16 — User Submits Correction Reviewed; Direito #3 Erasure LGPD Art. 18 VI + GDPR Art. 17 — User Requests Deletion com Conditional Logic Legal Hold Exceptions; Direito #4 Portability LGPD Art. 18 V + GDPR Art. 20 — JSON Export Structured; Direito #5 Objection GDPR Art. 21 — User Objects to Processing; Direito #6 Consent Revoke LGPD Art. 18 IX + GDPR Art. 7§3 — Revoke Prior Consent) + Identity Re-auth via MFA Code Re-prompted antes Submit (CTRL-AUTH-010 Fresh ≤ 30 min; Reuse S-03 WebAuthn Flow) + Submit → Confirmação em ≤ 1s + Signed JWT Receipt Displayed em UI + Emailed via SES (S-11 Alignment) + SLA Clock 30d Visível pro User (Countdown Timer; Email ao Começar/Avançar/Completar per S-11 Alignment) + DSR Status Viewer Separate Route `/settings/dsr` Lista request_id + Status + last_update + Receipt Download Link + Chaos Test Simula Slow API Response (≥ 5s API Delay; UI Shows Progress Indicator + Retry Option FM-150 Transient Handled)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-16](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S16-004 |
| Título | DSR self-service form 6 direitos LGPD/GDPR + MFA re-auth + JWT receipt + SLA clock + status viewer. |
| Sprint | S-16 |
| Lane | STANDARD |
| Forcing factors | none (UI consume DSR API S-11 já validated; MFA re-auth reuse S-03; LGPD/GDPR compliance baseline) |

## 1. Intent

Entregar **DSR self-service form compliance-grade** com 6 direitos LGPD Art. 18 + GDPR Art. 15-22, identity re-auth via MFA fresh ≤ 30 min, signed JWT receipt em ≤ 1s confirmation, SLA clock 30d visível, e status viewer route. Reduces DSR support tickets manual; baseline GA conversion S-20.

```typescript
// File: apps/web/src/app/(privacy)/dsr/page.tsx
type DsrDireito = 'access' | 'correction' | 'erasure' | 'portability' | 'objection' | 'consent_revoke';

export default function DsrForm() {
  const [direito, setDireito] = useState<DsrDireito>('access');
  const [details, setDetails] = useState('');
  const { user } = useUser();

  const submit = async () => {
    // CTRL-AUTH-010: MFA fresh ≤ 30 min required
    const mfaFresh = await requireMfaFresh(); // Clerk SDK MFA re-prompt
    if (!mfaFresh) return;

    const res = await fetch('/api/dsr/submit', {
      method: 'POST',
      body: JSON.stringify({ direito, details })
    });
    const { request_id, jwt_receipt, sla_deadline } = await res.json();
    // Display modal: Confirmation + JWT receipt + SLA clock starts
  };

  return (<DsrFormLayout direito={direito} setDireito={setDireito} details={details} setDetails={setDetails} onSubmit={submit} />);
}
```

## 2. Narrative

DSR (Data Subject Rights) self-service é compliance-critical: LGPD Art. 18 (6 direitos titular) + GDPR Art. 15-22 (rights of the data subject) exigem self-service + **SLAs per-right** (canonical S-11 _spec_contract: correction 5d úteis, access/portability/objection 15d úteis, erasure 30d calendar, consent_revoke ≤ 5min real-time — Lote 10.16 codex P0 canonical fix; previously hardcoded universal 30d era legally wrong). Stripe/Auth0 não têm DSR self-service UI; CoreLink S-16 entrega 6 direitos com identity re-auth MFA + JWT receipt + per-right SLA clock visível (zero competitors).

Backend S-11 implementa DSR API + ledger. UI implementa form 6 direitos + identity re-auth + receipt UI + status viewer. Chaos test simula slow API (≥ 5s); UI handles via progress indicator + retry (FM-150).

**Risk justification STANDARD lane**:
- UI consume DSR API S-11 já validated.
- MFA re-auth reuse S-03 cycle 9 SEAL pattern (CTRL-AUTH-010 reflection).
- JWT receipt reuse S-11 pattern.
- LGPD/GDPR compliance baseline; não introduz cripto-load-bearing controle novel.

## 3. Customer Impact & Journey

**Persona 1 — End-user (data subject)**:
- DSR form com 6 direitos clearly labeled (LGPD Art. 18 + GDPR Art. 15-22 references inline).
- Identity re-auth MFA seamless (Clerk SDK WebAuthn re-prompt).
- Submit → confirmação em ≤ 1s + JWT receipt displayed em UI + emailed.
- SLA clock 30d countdown timer visible em status viewer.
- Status viewer `/settings/dsr` lista all DSR requests + last_update + receipt download.

**Persona 2 — Privacy Officer cliente / Compliance auditor**:
- DSR fulfillment SLA 30d tracked + emailed (S-11 alignment).
- Signed JWT receipt audit trail forensic-grade.
- DSR status emailed ao começar/avançar/completar (per S-11 alignment).
- Erasure conditional logic (legal hold exceptions documented em UI).

**Persona 3 — Legal team (post-incident review)**:
- DSR receipts retrievable via JWT signature verify.
- Receipt includes direito + request_id + sla_deadline + tenant context.

## 4. Capability Mapping

- **CAP-UI-005** (DSR request form) — IMPLEMENTA primary; 6 direitos + MFA + JWT receipt.
- Trace: `_spec_contract.md §4 + §5.3 (R-S16-7)` + `privacy_model.md` (DSR 6 direitos LGPD Art. 18 + GDPR Art. 15-22) + `auth_model.md` (CTRL-AUTH-010 MFA fresh ≤ 30 min) + `S-11` (DSR API + ledger).

## 5. Tipo

Feature WI; STANDARD lane; DSR compliance-critical.

## 6. Escopo

### 6.1 In-scope

1. **DSR self-service form** em `apps/web/src/app/(privacy)/dsr/`:
   - Route `/settings/dsr` (auth-gated; SSR).
   - Form layout: direito picker (radio com 6 options + labels + LGPD/GDPR ref) + textarea details + submit button.

2. **6 direitos LGPD Art. 18 + GDPR Art. 15-22 — per-right SLA canonical (Lote 10.16 codex P0 fix; previously hardcoded universal 30d era legally wrong; S-11 _spec_contract §X é source-of-truth)**:
   - **Direito #1 Access** (LGPD Art. 18 II + GDPR Art. 15) — **SLA 15 dias úteis**:
     - Label: "Request access to all my personal data".
     - Backend POST `/api/dsr/submit` payload `{direito: "access", details}`.
     - Backend response: structured JSON export (within SLA 15 dias úteis = ~21 calendar days dependendo de feriados).
   - **Direito #2 Correction** (LGPD Art. 18 III + GDPR Art. 16) — **SLA 5 dias úteis**:
     - Label: "Correct inaccurate or incomplete data".
     - Textarea details: "What data is inaccurate? What should be corrected?".
     - Backend response: confirmation of correction within 5 dias úteis (~7 calendar days).
   - **Direito #3 Erasure** (LGPD Art. 18 VI + GDPR Art. 17) — **SLA 30 dias calendar**:
     - Label: "Delete my personal data ('right to be forgotten')".
     - Conditional logic warning: "Some data may be retained per legal hold (fiscal records 5y; audit log 7y). See /privacy for details.".
     - Confirmation modal: "Erasure is irreversible. Are you sure?".
     - Backend response: erasure attestation Ed25519-signed (S-11 R-S11-X) within SLA 30d calendar.
   - **Direito #4 Portability** (LGPD Art. 18 V + GDPR Art. 20) — **SLA 15 dias úteis**:
     - Label: "Export my data em portable JSON format".
     - Backend response: signed URL to JSON blob (within SLA 15 dias úteis).
   - **Direito #5 Objection** (GDPR Art. 21) — **SLA 15 dias úteis**:
     - Label: "Object to processing of my data".
     - Textarea details: "What processing do you object to? Why?".
     - Backend response: review + decision within 15 dias úteis.
   - **Direito #6 Consent revoke** (LGPD Art. 18 IX + GDPR Art. 7§3) — **SLA ≤ 5 minutos** (real-time):
     - Label: "Revoke prior consent".
     - Cross-link to `/consent` page (WI-S16-003) for context.
     - Backend marks consent as revoked + emits audit event S-09 within ≤ 5 min real-time (consent revoke must propagate immediately to halt processing per LGPD Art. 18 IX).
   - **Per-right SLA constants em `apps/web/src/lib/dsr-sla.ts`**: `DSR_SLA = { access: { days: 15, type: 'business' }, correction: { days: 5, type: 'business' }, erasure: { days: 30, type: 'calendar' }, portability: { days: 15, type: 'business' }, objection: { days: 15, type: 'business' }, consent_revoke: { minutes: 5, type: 'realtime' } }` — single source of truth para receipt deadline + email cadence + status viewer countdown.

3. **Identity re-auth via MFA** (CTRL-AUTH-010 fresh ≤ 30 min):
   - Before submit, check `lastVerifiedAt` claim from Clerk session.
   - If > 30 min ago, redirect to MFA re-prompt (Clerk SDK WebAuthn flow; reuse S-03).
   - After MFA verified, return to DSR form pre-filled state.
   - Property test: 100 random submit attempts > 30 min stale → all redirect to MFA.

4. **Submit → Confirmação em ≤ 1s + signed JWT receipt (per-right SLA Lote 10.16 codex P0 canonical)**:
   - Backend POST `/api/dsr/submit` returns within ≤ 1s p95 latency (SLA addendum spec contract).
   - Response payload: `{request_id, jwt_receipt, direito, sla_deadline}` — `sla_deadline` calculated per-right via `DSR_SLA[direito]` constant + business-days calculation (skip weekends + Brazilian/EU holidays para business-day rights; calendar-day for erasure; +5min para consent_revoke).
   - JWT receipt signed HS256 com payload (request_id + direito + sla_deadline + sla_type + tenant_id_hash + iat).
   - Modal "Your DSR request has been submitted. Request ID: <id>. Direito: <direito>. SLA deadline: <date> (<X dias úteis|calendar|minutos>). Receipt:".
   - JWT receipt displayed em modal (downloadable as `.jwt` file).
   - Email confirmation via SES (S-11 alignment) com receipt + per-right SLA timeline.

5. **SLA clock per-right visível (Lote 10.16 codex P0 canonical fix)**:
   - Countdown timer em status viewer route calculated from `submission_ts` to per-right `sla_deadline`.
   - Color-coded thresholds **per-right adaptive** (not universal 30d):
     - **Correction (5d úteis)**: green > 3d, yellow 1-3d, red < 1d.
     - **Access/Portability/Objection (15d úteis)**: green > 7d, yellow 3-7d, red < 3d.
     - **Erasure (30d calendar)**: green > 14d, yellow 7-14d, red < 7d.
     - **Consent revoke (5min)**: real-time progress bar; auto-refresh 30s.
   - Email triggers **per-right cadence** (Lote 10.16 codex P0 canonical):
     - **Correction**: T+0 received; T+3d (60% elapsed) status; T+5d completed/breach.
     - **Access/Portability/Objection**: T+0 received; T+8d mid-way; T+13d near-deadline; T+15d completed/breach.
     - **Erasure**: T+0 received; T+15d mid-way; T+25d near-deadline; T+30d completed/breach.
     - **Consent revoke**: T+0 received; T+5min completed (or breach if not propagated).
     - If breach (any right): backend escalation + customer notify + audit emit + DPO notification.

6. **DSR status viewer** separate route `/settings/dsr/status`:
   - Table: request_id (truncated last 4 chars) + direito + status (pending/processing/completed/expired) + submitted_ts + sla_deadline + last_update + actions.
   - Sorting: submitted_ts desc default.
   - Pagination cursor-based (≤ 50 rows per page).
   - Per row: receipt download link (re-issue JWT receipt if needed).
   - Status filter dropdown.

7. **Chaos test simula slow API** (FM-150 transient handled):
   - Test injects 5s delay em `/api/dsr/submit` response.
   - UI shows progress indicator (spinner + "Processing your request... this may take a moment").
   - Retry option button after 10s timeout ("Retry now").
   - Property test: 100 random delays 0-10s; UI never crashes; user can always retry.

8. **Plain-language form labels + Flesch-Kincaid ≤ 8** per locale (3 locales en-US + pt-BR + es-419):
   - Each direito label has plain-language explanation tooltip.
   - LGPD/GDPR references inline (small font; clickable to anchor in `/privacy`).

### 6.2 Out-of-scope (deferred)

- Audit log viewer (WI-S16-005; DSR events queryable there).
- Component library + a11y full (WI-S16-006).
- Closing PRR + Lighthouse + UX workshop (WI-S16-007).
- Backend DSR fulfillment automation (S-11; UI is capture only).
- Erasure conditional logic backend rules (S-11).

## 7. Anti-Scope

- DSR form sem identity re-auth MFA (CTRL-AUTH-010 violation).
- Submit > 1s p95 latency (UX regression baseline).
- JWT receipt sem signed signature (compliance gap).
- SLA clock missing (compliance gap; LGPD/GDPR non-compliance).
- Erasure sem conditional warning legal hold (compliance gap).
- Receipt download missing (audit trail gap).
- DSR submit without explicit direito picker (ambiguous request).
- Form labels dense legalese (UX abandonment).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: DSR self-service form 6 direitos + MFA + JWT receipt + SLA clock + status viewer

  Scenario: 6 direitos available em form
    Given user navigates /settings/dsr
    Then form picker shows 6 direitos: access, correction, erasure, portability, objection, consent_revoke
    And each direito has LGPD/GDPR reference inline
    And tooltip plain-language explanation

  Scenario: Identity re-auth MFA fresh ≤ 30 min
    Given user authenticated > 30 min ago
    When user clicks submit
    Then redirect to MFA re-prompt (Clerk SDK WebAuthn)
    When MFA verified < 30 min
    Then submit proceeds com lastVerifiedAt updated

  Scenario: Submit → confirmação em ≤ 1s
    Given user submits DSR request
    When backend POST /api/dsr/submit
    Then response em ≤ 1s p95 latency
    And payload includes {request_id, jwt_receipt, sla_deadline}
    And modal "Your DSR request submitted. Request ID: <id>" displayed

  Scenario: JWT receipt signed + downloadable
    Given DSR submitted
    When JWT receipt generated
    Then payload includes {request_id, direito, tenant_id_hash, sla_deadline, iat}
    And signature HS256 verifiable
    And receipt downloadable as .jwt file

  Scenario: Email confirmation via SES
    Given DSR submitted
    Then email sent via SES with receipt + SLA timeline
    And subject "Your DSR request has been received"

  Scenario: SLA clock visible em status viewer
    Given DSR request submitted
    When user navigates /settings/dsr/status
    Then countdown timer displayed (T+0 to T+30d)
    And color-coded green > 14d remaining

  Scenario: Email at SLA milestones
    Given DSR request at T+15d
    Then email "DSR request mid-way; expected completion in 15d"
    Given DSR at T+25d
    Then email "DSR request near deadline (5d remaining)"
    Given DSR at T+30d (completion)
    Then email "DSR request completed"

  Scenario: Erasure conditional warning legal hold
    Given user picks direito erasure
    Then warning displayed "Some data may be retained per legal hold (fiscal records 5y; audit log 7y)"
    And confirmation modal "Erasure is irreversible. Are you sure?" before submit

  Scenario: Status viewer table + filter + pagination
    Given user has 75 DSR requests
    When user navigates /settings/dsr/status
    Then table shows 50 rows page 1 + cursor next
    And sorting by submitted_ts desc default
    And status filter dropdown (pending/processing/completed/expired)
    And per-row receipt download link

  Scenario: Chaos test slow API ≥ 5s
    Given backend response delayed 5s
    When user submits DSR
    Then UI shows progress indicator + spinner
    And retry option button after 10s timeout
    And user can always retry without crash

  Scenario: SLA breach > 30d → backend escalation
    Given DSR request at T+31d (breach)
    Then backend escalation triggered + customer notify email
    And audit event S-09 emitted "dsr_sla_breach"

  Scenario: Plain-language labels Flesch-Kincaid ≤ 8 per locale
    Given form labels rendered em pt-BR
    When Flesch-Kincaid grade computed em CI
    Then grade ≤ 8
    And native speaker review per locale (em WI-S16-006)
```

## 9. Design Decisions

### 9.1 Why 6 direitos (não 5 ou 7)

- 6 = full LGPD Art. 18 + GDPR Art. 15-22 coverage (access + correction + erasure + portability + objection + consent_revoke).
- LGPD specific: also includes "info on processing" + "info on shared parties" (covered by access via JSON export).
- 7+ scope creep (e.g., complaint to authority is external — ANPD/DPA channel).

### 9.2 Why MFA fresh ≤ 30 min (não 15m ou 60m)

- 30m balance: user convenience vs identity re-auth assurance.
- Reuses CTRL-AUTH-010 baseline from S-03 cycle 9 SEAL.
- 15m too disruptive UX; 60m too lax for sensitive ops.

### 9.3 Why JWT receipt HS256 (não Ed25519)

- HS256: simple symmetric signing; reuses S-11 receipt key infrastructure.
- Ed25519 (asymmetric): would require key distribution; not justified for internal verifiable receipts.

### 9.4 Why SLA clock 30d (não 15d ou 45d)

- LGPD Art. 19: response within 15 days; extension up to 30 days possible com justificativa.
- GDPR Art. 12§3: response within 30 days.
- Conservatism: 30d default with email triggers at T+15d (LGPD optimal) + T+25d (GDPR near).

### 9.5 Why erasure conditional warning (não silent reject)

- Legal hold (fiscal records 5y; audit log 7y) prevents complete erasure.
- Silent reject = user trust loss; explicit warning + confirmation = informed user.

### 9.6 Why status viewer separate route (não inline form)

- Form is one-shot; status viewer is recurring access.
- Separate route enables bookmarking + sharing (within tenant team).

### 9.7 ADR potencial?

- Não. Patterns reused (Clerk MFA + JWT signing + SES email + Flesch-Kincaid CI). No novel architecture decision.

## 10. Completeness Criteria

- [ ] **10.s16.004.1** 6 direitos available em form com LGPD/GDPR ref + plain-language tooltips (EVT-018).
- [ ] **10.s16.004.2** Identity re-auth MFA fresh ≤ 30 min enforced (CTRL-AUTH-010) (EVT-002).
- [ ] **10.s16.004.3** Submit → confirmação em ≤ 1s p95 latency + JWT receipt (EVT-018).
- [ ] **10.s16.004.4** SLA clock 30d countdown visible em status viewer.
- [ ] **10.s16.004.5** Email triggers SES per S-11 alignment (T+0 + T+15d + T+25d + T+30d + breach).
- [ ] **10.s16.004.6** Erasure conditional warning legal hold + confirmation modal.
- [ ] **10.s16.004.7** Status viewer route com table + filter + pagination + receipt download.
- [ ] **10.s16.004.8** Chaos test slow API ≥ 5s; UI progress indicator + retry option.
- [ ] **10.s16.004.9** Form labels Flesch-Kincaid ≤ 8 per locale.
- [ ] **10.s16.004.10** Property test 100 random submit attempts > 30 min stale → all redirect MFA.

## 11. DoD

- [ ] DSR form deployed staging.
- [ ] 6 direitos integrated com S-11 backend.
- [ ] MFA re-auth flow tested (Clerk SDK WebAuthn).
- [ ] JWT receipt signed + verifiable; downloadable as .jwt.
- [ ] SLA clock countdown visible + color-coded.
- [ ] Email triggers tested (SES sandbox em staging).
- [ ] Status viewer route accessible + functional.
- [ ] Chaos test slow API validated.
- [ ] Tests: unit (DSR submit handler + JWT receipt sign + SLA computation) + integration (E2E /settings/dsr → backend persist → email triggered) + 4+ negative scenarios.

## 12. Invariants Validated

- **CTRL-AUTH-010** (admin destructive ops fresh MFA ≤ 30 min) reforced em DSR submit.
- **CTRL-PRIV-CONSENT-001..006** consumed (consent_revoke direito #6 marks consent as revoked).
- **CTRL-PRIV-001** (zero PII em client-side logs) reforced via safeLog wrapper.
- **Não introduz INVs novas** (UI é consumer; per spec contract §8).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| DSR form | `apps/web/src/app/(privacy)/dsr/page.tsx` | TypeScript |
| DSR submit API route | `apps/web/src/app/api/dsr/submit/route.ts` | TypeScript |
| DSR status viewer | `apps/web/src/app/settings/dsr/status/page.tsx` | TypeScript |
| MFA re-auth helper | `apps/web/src/lib/mfa-fresh.ts` | TypeScript |
| JWT receipt lib | `apps/web/src/lib/dsr-receipt.ts` | TypeScript |
| SLA clock component | `apps/web/src/components/sla-clock.tsx` | TypeScript |
| Email templates | `apps/web/src/email-templates/dsr-{received,midway,near,completed,breach}.html` | HTML |

## 14. Quality Standards

- **14.s16.004.1** DSR submit p95 latency ≤ 1s.
- **14.s16.004.2** Test coverage ≥ 85% (DSR handler + JWT receipt + SLA + MFA helper).
- **14.s16.004.3** SAST: jsonwebtoken + Clerk SDK zero CVEs HIGH/CRITICAL.
- **14.s16.004.4** Form labels Flesch-Kincaid ≤ 8 per locale CI gate.
- **14.s16.004.5** Property test 100 random delays + MFA stale scenarios.
- **14.s16.004.6** Email triggers SES sandbox em staging verified.

## 15. Test Plan

### Unit tests (≥ 85% coverage)
- DSR submit handler: 6 direitos validated; payload schema zod.
- JWT receipt sign: HS256 + verifiable; payload includes required fields.
- SLA computation: 30d from submission_ts; color-coded breakpoints.
- MFA helper: lastVerifiedAt check; redirect logic.

### Integration tests (E2E staging)
- Full /settings/dsr flow: pick direito → MFA re-auth → submit → backend persist → modal + email.
- Status viewer: table + filter + pagination + receipt download.
- Erasure conditional warning + confirmation modal.

### Negative scenarios (≥ 4)
1. **MFA stale > 30 min**: redirect to MFA re-prompt antes submit.
2. **Submit > 1s latency**: UI shows progress indicator + retry.
3. **JWT receipt tampered**: signature verify fails; receipt rejected.
4. **Erasure without confirmation**: form rejects submit (modal mandatory).
5. **SLA breach > 30d**: backend escalation + customer notify email.
6. **Status viewer permission**: user X cannot see user Y's DSR requests (auth scoping).

### Chaos test
- Inject 5s + 10s + 30s API delays; verify UI handles gracefully.

### Cross-browser
- Smoke test em Chrome + Firefox + Safari + Edge; full matrix em WI-S16-007.

## 16. Failure Modes

- **FM-150** (transient API): DSR submit retry com exponential backoff (3 retries default); progress indicator clear; chaos test handled.
- **FM-160** (auth invalid): clear error UI; redirect to Clerk sign-in flow.
- **DSR SLA breach**: backend escalation + customer notify (S-11 alignment).

## 17. Controls

- **CTRL-AUTH-010** (admin destructive ops fresh MFA ≤ 30 min): DSR submit requires MFA re-auth.
- **CTRL-PRIV-CONSENT-001..006** consumed (consent_revoke direito #6).
- **CTRL-PRIV-001** (zero PII em client logs) reforced via safeLog wrapper.

## 18. Resilience Patterns

- Retry transient errors (FM-150): exponential backoff em DSR submit; progress indicator + retry button.
- Optimistic UI confirmation com rollback em backend reject.
- Email retry via SES (S-11 alignment).
- SLA breach escalation backend (S-11).

## 19. Observability

UI métricas Prometheus snake_case:
- `corelink_admin_ui_dsr_submit_total{direito, outcome}` counter (direito ∈ access|correction|erasure|portability|objection|consent_revoke).
- `corelink_admin_ui_dsr_submit_duration_ms` histogram (p95 ≤ 1s).
- `corelink_admin_ui_dsr_mfa_reprompt_total{outcome}` counter (outcome ∈ verified|failed).
- `corelink_admin_ui_dsr_sla_breach_total` counter (alert > 0).
- `corelink_admin_ui_dsr_receipt_download_total` counter.

Cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (NUNCA per-tenant labels).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: identity re-auth MFA fresh ≤ 30 min CTRL-AUTH-010; Clerk WebAuthn passkey.
- **Tampering**: JWT receipt signed HS256 + verifiable; backend persist + audit event S-09.
- **Repudiation**: signed JWT receipt + emailed via SES + audit event S-09 forensic-grade trail.
- **Information disclosure**: PAT NUNCA em DSR payload; safeLog allowlist; receipt downloadable only by request originator.
- **DoS**: DSR submit rate-limited (≤ 5/dia per user; via backend); MFA re-auth bounded.
- **Elevation of privilege**: status viewer scoped per user; cross-user access prevented.

**LINDDUN delta**:
- **Linkability**: request_id UUID v7; tenant_id_hash em JWT receipt.
- **Identifiability**: DSR payload references user_id_hash; original DSR records retained per S-11 retention policy 5y legal evidence.
- **Non-repudiation**: signed JWT receipt + emailed + audit event forensic-grade.
- **Detectability**: SLA breach alerts; MFA reprompt tracked.
- **Disclosure**: PAT NUNCA em DSR payload ou receipt.
- **Unawareness**: form plain-language Flesch-Kincaid ≤ 8 + LGPD/GDPR refs inline.
- **Non-compliance**: LGPD Art. 18 (6 direitos) + GDPR Art. 15-22 (data subject rights) + GDPR Art. 25 (data protection by design); LINDDUN review committed em `specs/_audits/2026-XX-XX-linddun-dsr-ui.md`.

## 21. Dependencies

### Hard blockers
- WI-S16-001 SEALED (skeleton + Clerk + CSP + i18n).
- WI-S16-003 SEALED (consent_revoke flow integrated; cross-link).
- S-11 SEALED (DSR API + ledger + email triggers).
- S-03 SEALED (Clerk MFA + WebAuthn flow).

### Soft blockers
- S-09 SEALED (audit events for DSR submit).
- SES sandbox setup em staging.

### Outbound
- WI-S16-005 (audit log viewer queries DSR events).
- WI-S16-007 (E2E DSR submit + MFA flow + closing PRR).

## 22. Effort PERT

O: 14h, M: 22h, P: 36h → PERT **23.0h** (per spec contract §12; DSR form + 6 direitos + MFA + JWT receipt + SLA clock + status viewer + chaos test).

## 23. Cost Analysis

- SES emails (DSR triggers): ~$0.10 / 1000 emails (≤ 5 emails per DSR; ≤ 100 DSRs/mês = $0.05/mês).
- Backend DSR API (S-11): infra costed em S-11.
- Total: ~$1/mês incremental.

## 24. Post-mortem Hooks

- DSR form failure (user can't submit) → post-mortem (compliance impact).
- MFA re-auth bypass detected → CRITICAL post-mortem + AppSec review.
- JWT receipt tampered (verify fails em audit) → CRITICAL post-mortem + crypto review.
- DSR SLA breach > 30d sustained → compliance post-mortem + Legal review.
- Email trigger failure sustained > 1d → SEV-2 post-mortem.

## 25. Rollback / Recovery

DSR form regression detected → revert via CF Pages rollback; backend S-11 retains existing DSR records (no data loss).

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | DSR form abandonment UX miss | M | L | LOW | L | LOW | UX research; iterate; clear progress indicator + plain-language |
| R-002 | MFA re-auth disrupts UX flow | M | L | LOW | L | LOW | Pre-prompt expectation; reuse Clerk SDK WebAuthn passkey (≤ 5s) |
| R-003 | JWT receipt verify failure em audit | L | M | HIGH | M | LOW | Comprehensive verify tests; key rotation plan; audit event S-09 |
| R-004 | SLA breach > 30d (compliance gap) | L | L | HIGH | M | LOW | Backend S-11 escalation; customer notify; audit S-09 |
| R-005 | Erasure conditional logic misunderstood | M | L | MEDIUM | M | LOW | Plain-language warning + confirmation modal + privacy notice cross-link |
| R-006 | Status viewer cross-user access leak | L | L | CRITICAL | L | LOW | Backend auth scoping per user_id; integration test |

## 27. Knowledge Transfer

- Tech talk (1h): "CoreLink DSR Self-service Form — 6 Direitos + MFA + JWT Receipt + SLA".
- Doc `docs/internal/dsr-self-service.md` — overview + LGPD/GDPR mapping.
- Onboarding test (3 questions): 6 direitos LGPD/GDPR ref + MFA fresh ≤ 30 min + SLA clock 30d.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Frontend Lead | _TBD; emphatic — DSR form + status viewer + JWT receipt + SLA clock_ | _pending_ |
| 4 | QA | _TBD; emphatic — E2E DSR + MFA stale scenarios + chaos test slow API_ | _pending_ |
| 5 | Product | Gustavo Schneiter | _pending_ |
| 6 | Designer/a11y advisor | _TBD; emphatic — form UX + SLA clock UX + plain-language labels per locale_ | _pending_ |
| 7 | Privacy officer | _TBD; emphatic — LGPD Art. 18 + GDPR Art. 15-22 mapping + erasure conditional logic + LINDDUN review_ | _pending_ |

> STANDARD lane (per framework §33.5.4): 5-8 canonical sign-offs; 7 typical. Privacy officer canonical em S-16-004 (DSR compliance-critical).

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S16-004 (cycle 12.S16.0; DSR self-service form 6 direitos + MFA + JWT receipt + SLA clock + status viewer). |

## 30. Anti-patterns evitados

- DSR form sem identity re-auth MFA (CTRL-AUTH-010 violation).
- Submit > 1s p95 latency (UX regression baseline).
- JWT receipt sem signed signature (compliance gap).
- SLA clock missing (compliance gap; LGPD/GDPR non-compliance).
- Erasure sem conditional warning legal hold (compliance gap).
- Receipt download missing (audit trail gap).
- DSR submit without explicit direito picker (ambiguous request).
- Form labels dense legalese (UX abandonment).
- Cross-user status viewer access (auth scoping gap).
- Email trigger missing (SLA milestones gap).

---

**Fim WI-S16-004.**
