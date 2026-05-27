---
id: "WI-S16-002"
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
tags: ["wi", "s16", "ui", "onboarding", "dpa", "stripe", "pat-management", "self-service", "standard"]
---

# WI-S16-002 — Tenant Onboarding Flow Self-service Completo (Signup Wizard Step-by-step com Progress Indicator Clear → DPA Acceptance UI com Texto Versionado `legal/dpa/v<M.m>.md` Rendered via @next/mdx + Capture Timestamp + Acceptance Signed JWT Receipt → Stripe Checkout Integration via Stripe SDK + Stripe Customer Portal Embed → First PAT Created via PAT Management Widget [Scope Picker read-only/read-write/admin com Explanation per Scope; Expiry Picker 30d/90d/1y/never; PAT Exibido **Apenas Uma Vez** + Warning Copy-protection + Masked após Copy + Button "I've Copied It"; List Active id+name+scope+last_used+expires; Revoke Single + Revoke All]) + Tenant Self-service Settings UI (Config + Retention Policy Picker + Flag Toggle pra Dev/Test Scenarios) + URL-shareable Invite Link para Co-developers (Signed JWT com tenant_id + role; Reusa Clerk SDK Invite Flow) + Time-to-First-PAT ≤ 5 min Measured via UX Workshop 5 Devs (PRR Gate em WI-S16-007)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-16](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S16-002 |
| Título | Tenant onboarding self-service flow + DPA + Stripe + first PAT + settings UI + invite link. |
| Sprint | S-16 |
| Lane | STANDARD |
| Forcing factors | none (consume APIs S-03/S-10 já validated; Stripe Checkout + Customer Portal são Stripe-managed surfaces) |

## 1. Intent

Entregar **tenant onboarding flow completo self-service**: signup wizard → DPA acceptance → Stripe billing → first PAT created em ≤ 5 min end-to-end. Reduces support tickets manual onboarding to zero baseline; baseline para GA conversion S-20.

```typescript
// File: apps/web/src/app/(onboarding)/wizard/page.tsx
export default function OnboardingWizard() {
  const [step, setStep] = useState<'signup' | 'dpa' | 'billing' | 'first-pat' | 'done'>('signup');
  const { user } = useUser(); // Clerk SDK

  return (
    <WizardLayout step={step} totalSteps={4}>
      {step === 'signup' && <SignupStep onNext={() => setStep('dpa')} />}
      {step === 'dpa' && <DpaAcceptanceStep onAccept={(receipt) => { trackDpa(receipt); setStep('billing'); }} />}
      {step === 'billing' && <StripeCheckoutStep onComplete={() => setStep('first-pat')} />}
      {step === 'first-pat' && <FirstPatStep onCreated={() => setStep('done')} />}
      {step === 'done' && <OnboardingComplete tenantId={user.publicMetadata.tenantId} />}
    </WizardLayout>
  );
}
```

## 2. Narrative

Self-service onboarding é tabela rasa para GA conversion. Stripe Dashboard + Auth0 Dashboard são gold-standard em smooth onboarding; CoreLink S-16 entrega parity + diferenciador em time-to-first-PAT ≤ 5 min measured via UX workshop 5 devs externos rotated per sprint.

Este WI implementa o wizard step-by-step (signup → DPA → billing → first PAT), tenant self-service settings UI, e URL-shareable invite link para co-developers. PAT management widget (scope picker + expiry picker + once-display + masked após copy) é shared component reused em settings (subsequent PAT creations) e onboarding (first PAT).

**Risk justification STANDARD lane**:
- Consome APIs S-03 (PAT create + Clerk auth) + S-10 (billing + Stripe).
- Stripe Checkout + Customer Portal = Stripe-managed surfaces (no novel cripto-load-bearing controle).
- DPA acceptance é capture frontend; backend persist em S-10/S-13.
- PAT once-display + masked após copy = CTRL-CRED-001 reflection (não introduz novo control).

## 3. Customer Impact & Journey

**Persona 1 — Build Engineer prospect novo**:
- Wizard step-by-step com progress indicator clear (4 steps: signup → DPA → billing → first PAT).
- Time-to-first-PAT ≤ 5 min measured via UX workshop 5 devs externos.
- DPA acceptance versionado mdx; texto plain-language Flesch-Kincaid grade ≤ 8.
- Stripe Checkout seamless + Customer Portal embed for invoice history.
- First PAT exibido apenas uma vez + masked após copy + button "I've copied it" (CTRL-CRED-001 reflection).
- URL-shareable invite link para co-developers (signed JWT com tenant_id + role).

**Persona 2 — Privacy/Compliance officer**:
- DPA acceptance signed JWT receipt audit trail forensic-grade.
- DPA versionado per semver; capture timestamp em backend (S-10/S-13 alignment).
- Tenant self-service settings UI: retention policy picker + flag toggle pra dev/test scenarios.

## 4. Capability Mapping

- **CAP-UI-001** (Tenant onboarding flow) — IMPLEMENTA primary; signup → DPA → billing → first PAT.
- **CAP-UI-006** (PAT management) — IMPLEMENTA primary widget reused; scope picker + expiry + once-display + revoke.
- **CAP-UI-008** (Billing overview) — IMPLEMENTA via Stripe Customer Portal embed.
- Trace: `_spec_contract.md §4 + §5.1` + `auth_model.md` (Clerk + PAT format hybrid S-03) + `security_model.md` (CTRL-CRED-001 PAT once-display + masked).

## 5. Tipo

Feature WI; STANDARD lane; onboarding + PAT mgmt + settings.

## 6. Escopo

### 6.1 In-scope

1. **Onboarding wizard step-by-step** em `apps/web/src/app/(onboarding)/wizard/`:
   - **Step 1 — Signup**: Clerk SDK `<SignUp />` flow (SSO providers + email + WebAuthn passkey enrolment); on completion, tenant_id auto-provisioned via Clerk webhook (S-03 reuse).
   - **Step 2 — DPA acceptance**:
     - Render mdx `legal/dpa/v<M.m>.md` (latest semver) via @next/mdx; texto plain-language Flesch-Kincaid grade ≤ 8.
     - Checkbox "I have read and accept the Data Processing Agreement v<M.m>".
     - Capture `dpa_version` + `acceptance_ts` (browser ts) + `submission_ts` (server ts) em backend persist via POST `/api/dpa-accept`.
     - Backend signs JWT receipt (HS256 + key from S-03); receipt displayed em UI + emailed via SES.
   - **Step 3 — Billing setup**:
     - Stripe SDK `@stripe/stripe-js` + `@stripe/react-stripe-js`; Checkout session created via backend POST `/api/billing/checkout`.
     - Embed Stripe Checkout (hosted by Stripe; no PCI scope client-side).
     - Plan picker: free / starter / growth / enterprise (matching S-10 plan tiers).
     - On completion, Stripe webhook → backend persist tenant.plan_tier (S-10 reuse).
   - **Step 4 — First PAT created**:
     - PAT management widget (vide §6.2) embedded com pre-selected scope `read-write` + expiry `90d` defaults.
     - User confirma → PAT created via backend POST `/api/pat/create` (S-03 reuse).
     - **PAT exibido apenas uma vez** em modal com `<code>` rendered + "Copy to clipboard" button + warning "This is the only time you'll see this PAT. Store it securely.".
     - Após copy click, PAT masked como `corelink_prod_abc***`; modal close button enabled.
     - Suggested next-action: link to `examples/bazel-starter` ou `examples/buck2-starter` (S-15 reuse) com pre-filled CORELINK_PAT em copy-paste snippet.

2. **PAT management widget** (shared component em `apps/web/src/components/pat-mgmt.tsx`; reused em settings + onboarding):
   - **Create PAT form**:
     - **Scope picker**: radio com 3 options:
       - `read-only` (CAS read + AC read; safe for CI cache hit).
       - `read-write` (CAS write + read; default for build engineer).
       - `admin` (admin plane API; warning "Use only for ops; create separate PAT for build pipelines").
     - **Expiry picker**: dropdown `30d` / `90d` (default) / `1y` / `never` (warning "Never-expiring PATs require manual rotation").
     - **Optional name**: free-text label for PAT (e.g., "ci-pipeline").
     - Submit → backend POST `/api/pat/create` (S-03 reuse).
   - **Once-display modal** (vide §6.1 step 4 narrative).
   - **List active PATs** table:
     - Columns: id (truncated last 4 chars), name, scope, last_used, expires, actions.
     - Sorting: last_used desc default.
     - Pagination cursor-based (≤ 50 rows per page).
   - **Revoke single**: button per row → confirmation modal "Revoke PAT '<name>'? This action cannot be undone." → backend POST `/api/pat/{id}/revoke`.
   - **Revoke all**: button → confirmation modal + MFA re-auth (CTRL-AUTH-010 fresh ≤ 30 min) → backend POST `/api/pat/revoke-all`.
   - **Rotation hint**: warning banner se any PAT > 80% lifetime ("PAT '<name>' expires in N days; consider rotating").

3. **Tenant self-service settings UI** em `apps/web/src/app/settings/`:
   - `/settings` (overview): name, plan tier, region, created_at, billing email.
   - `/settings/security` (PAT mgmt + MFA factors).
   - `/settings/billing` (Stripe Customer Portal embed + invoice history).
   - `/settings/retention` (retention policy picker: 30d / 90d / 1y / forever; aligns with S-08 quota).
   - `/settings/flags` (feature flag toggle pra dev/test scenarios; e.g., `enable_byok_setup_wizard`, `enable_telemetry_opt_in`).
   - `/settings/team` (URL-shareable invite link generator).

4. **URL-shareable invite link para co-developers**:
   - Generate signed JWT com `tenant_id` + `role` (member/admin) + `expires_at` (default 7d).
   - URL pattern `https://app.corelink.humangr.com/invite?token=<jwt>`.
   - On open, Clerk SDK `<SignUp />` flow pre-filled com tenant context; invite redeemed em first auth.
   - Reusa Clerk SDK invite flow (S-03).

5. **Time-to-first-PAT ≤ 5 min instrumentation**:
   - Métrica `corelink_admin_ui_onboarding_step_duration_ms{step}` histogram (step ∈ signup|dpa|billing|first_pat).
   - Métrica `corelink_admin_ui_onboarding_completion_total{outcome}` counter (outcome ∈ completed|abandoned).
   - UX workshop 5 devs externos rotated per sprint (measured em WI-S16-007 closing PRR).

### 6.2 Out-of-scope (deferred)

- Consent management UI (WI-S16-003).
- DSR self-service form (WI-S16-004).
- Admin operations UI + audit viewer (WI-S16-005).
- Component library + a11y full + privacy/sub-processors pages (WI-S16-006).
- Closing PRR + Lighthouse + UX workshop sample (WI-S16-007).
- Enterprise sales-led onboarding (S-19).

## 7. Anti-Scope

- PAT exibido > 1x (CTRL-CRED-001 violation).
- DPA acceptance sem versioning (compliance gap).
- Stripe Checkout custom (PCI scope creep; use Stripe-hosted Checkout).
- Invite link sem signed JWT expiry (security gap).
- Onboarding wizard sem progress indicator (UX gap).
- Time-to-first-PAT > 5 min sustained (DX regression baseline).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Tenant onboarding flow + PAT mgmt + settings UI

  Scenario: Onboarding wizard completes em ≤ 5 min
    Given new user navigates /onboarding/wizard
    When user completes signup → DPA → billing → first PAT
    Then total elapsed time ≤ 5 min em ≥ 80% UX workshop trials
    And tenant_id provisioned em backend
    And first PAT created scoped read-write expiry 90d default

  Scenario: DPA acceptance versioned + JWT receipt
    Given DPA mdx v1.2.0 rendered
    When user clicks "I accept"
    Then backend POST /api/dpa-accept payload includes dpa_version=1.2.0 + acceptance_ts
    And backend signs JWT receipt
    And receipt displayed em UI + emailed via SES

  Scenario: Stripe Checkout integrates seamless
    Given user em billing step
    When user clicks "Continue to Checkout"
    Then Stripe Checkout session created via backend
    And user redirected to Stripe-hosted Checkout
    And after payment, webhook → tenant.plan_tier persisted

  Scenario: First PAT exibido apenas uma vez + masked após copy
    Given user em first-pat step
    When user submits PAT create form
    Then PAT exibido em modal com "Copy to clipboard" button
    And warning "This is the only time you'll see this PAT" visible
    When user clicks "I've copied it"
    Then PAT masked como "corelink_prod_abc***"
    And modal close button enabled

  Scenario: PAT scope picker com explanation
    Given user em PAT create form
    When user opens scope picker
    Then 3 options visible: read-only, read-write, admin
    And each option has explanation text
    And admin scope shows warning "Use only for ops"

  Scenario: PAT expiry picker default 90d
    Given user em PAT create form
    When form initialized
    Then expiry picker default = "90d"
    And "never" option shows warning "Never-expiring PATs require manual rotation"

  Scenario: List active PATs com pagination
    Given tenant has 75 active PATs
    When user navigates /settings/security
    Then table shows 50 rows page 1 + cursor next
    And sorting by last_used desc default
    And revoke button per row + revoke all button

  Scenario: Revoke all PATs requires MFA re-auth
    Given user authenticated > 30 min ago
    When user clicks revoke all
    Then MFA re-prompt CTRL-AUTH-010
    And after MFA verified, all PATs revoked

  Scenario: URL-shareable invite link signed JWT
    Given tenant admin generates invite link
    When invite token created
    Then signed JWT com tenant_id + role + expires_at (7d default)
    And URL https://app.corelink.humangr.com/invite?token=<jwt>
    When invitee opens URL, Clerk SignUp flow pre-filled tenant context

  Scenario: Onboarding wizard progress indicator clear
    Given user em wizard
    Then progress bar shows current step / total (4 steps)
    And labels per step (Signup / DPA / Billing / First PAT)
    And keyboard navigation tab order logical (a11y baseline)

  Scenario: Onboarding abandonment tracked
    Given user starts wizard but exits before completion
    When user closes browser tab
    Then métrica corelink_admin_ui_onboarding_completion_total{outcome=abandoned} incremented
    And step at abandonment captured

  Scenario: Settings retention policy picker
    Given tenant admin navigates /settings/retention
    When user selects "90d"
    Then backend POST /api/tenant/retention payload {policy: "90d"}
    And confirmation toast displayed
    And audit event emitted (S-09)
```

## 9. Design Decisions

### 9.1 Why 4-step wizard (não 1-page form)

- 4-step = clear progress indicator + reduces cognitive load.
- 1-page form = abandonment risk em billing step; wizard isolates per concern.
- Stripe/Auth0 baseline: 3-5 step wizards onboarding.

### 9.2 Why Stripe Checkout hosted (não custom embed Elements)

- PCI scope: Stripe-hosted Checkout = SAQ A (lowest scope).
- Custom Elements = SAQ A-EP (higher scope; not justified for onboarding).
- Stripe Customer Portal embed em settings/billing (subsequent invoice history).

### 9.3 Why PAT once-display + masked após copy (CTRL-CRED-001)

- PAT em URL params ou logs = security incident baseline.
- Once-display + masked + button "I've copied it" = forces user action + reduces accidental exposure.

### 9.4 Why invite link signed JWT 7d expiry default

- 7d balance: long enough for invitee to redeem; short enough to limit replay window.
- Custom expiry (configurable em settings/team) up to 30d max.

### 9.5 Why retention policy picker em settings (não onboarding)

- Onboarding focus: signup → DPA → billing → first PAT (minimal cognitive load).
- Retention policy is post-onboarding tweak; default global policy applies until overridden.

### 9.6 ADR potencial?

- Não. Patterns reused (Stripe Checkout hosted + Clerk invite + JWT signed). No novel architecture decision.

## 10. Completeness Criteria

- [ ] **10.s16.002.1** Onboarding wizard 4 steps deployed staging (EVT-018).
- [ ] **10.s16.002.2** DPA acceptance versionado + JWT receipt + emailed (EVT-018).
- [ ] **10.s16.002.3** Stripe Checkout integrated + plan tier persisted backend (EVT-018).
- [ ] **10.s16.002.4** First PAT created via widget + once-display modal + masked após copy (EVT-002 CTRL-CRED-001).
- [ ] **10.s16.002.5** PAT mgmt widget shared (create + list + revoke single + revoke all + MFA re-auth) (EVT-002).
- [ ] **10.s16.002.6** Tenant self-service settings UI (overview + security + billing + retention + flags + team).
- [ ] **10.s16.002.7** URL-shareable invite link signed JWT 7d default expiry.
- [ ] **10.s16.002.8** Time-to-first-PAT instrumentation métricas emitting staging (EVT-013).
- [ ] **10.s16.002.9** Onboarding abandonment tracked per step.
- [ ] **10.s16.002.10** Plain-language DPA + scope explanations Flesch-Kincaid grade ≤ 8.

## 11. DoD

- [ ] Onboarding wizard tested staging (4 steps; UX walkthrough OK).
- [ ] PAT mgmt widget unit + integration tests; revoke-all MFA re-auth flow verified.
- [ ] Settings UI 6 routes accessible + functional.
- [ ] Invite link signed JWT verified (sign + verify; expiry enforce).
- [ ] Time-to-first-PAT instrumented; staging telemetry confirmed.
- [ ] Tests: unit (PAT widget + invite JWT + DPA acceptance handler) + integration (wizard E2E staging) + 4+ negative scenarios.

## 12. Invariants Validated

- **CTRL-CRED-001** (no PAT em client logs) reforced via PAT once-display + masked após copy + Sentry redaction allowlist.
- **CTRL-AUTH-010** (admin destructive ops fresh MFA ≤ 30 min) reforced em revoke-all flow.
- **Não introduz INVs novas** (UI é consumer; per spec contract §8 mantidas only).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Onboarding wizard | `apps/web/src/app/(onboarding)/wizard/` | TypeScript |
| DPA acceptance handler | `apps/web/src/app/api/dpa-accept/route.ts` | TypeScript |
| Stripe Checkout integration | `apps/web/src/app/api/billing/checkout/route.ts` | TypeScript |
| PAT mgmt widget | `apps/web/src/components/pat-mgmt.tsx` | TypeScript |
| PAT API routes | `apps/web/src/app/api/pat/*/route.ts` | TypeScript |
| Settings UI | `apps/web/src/app/settings/` | TypeScript |
| Invite link generator | `apps/web/src/lib/invite-jwt.ts` | TypeScript |
| DPA mdx | `legal/dpa/v1.0.md` | Markdown+JSX |

## 14. Quality Standards

- **14.s16.002.1** PAT once-display enforced via UI lifecycle assertion + integration test.
- **14.s16.002.2** Test coverage ≥ 80% (PAT widget + invite JWT + DPA acceptance + Stripe webhook handler).
- **14.s16.002.3** SAST: npm audit + Dependabot zero CVEs HIGH/CRITICAL em Stripe SDK + JWT lib.
- **14.s16.002.4** UX workshop 5 devs externos sample em WI-S16-007 measures time-to-first-PAT ≤ 5 min em ≥ 80% trials.
- **14.s16.002.5** DPA + scope explanations Flesch-Kincaid grade ≤ 8 verified em CI.
- **14.s16.002.6** Stripe Checkout PCI scope SAQ A only (hosted; no custom Elements).

## 15. Test Plan

### Unit tests (≥ 80% coverage)
- PAT widget: scope picker validates options; expiry picker default 90d; once-display lifecycle.
- Invite JWT: sign + verify + expiry enforce; replay rejected.
- DPA acceptance handler: backend persist + JWT receipt sign.
- Stripe webhook handler: tenant.plan_tier update on `checkout.session.completed`.

### Integration tests (E2E staging)
- Wizard 4 steps complete em staging; tenant + DPA + plan + first PAT persisted.
- PAT create + list + revoke single + revoke all (MFA re-auth flow).
- Settings UI 6 routes accessible + functional.
- Invite link redemption via Clerk SignUp pre-filled context.

### Negative scenarios (≥ 4)
1. **PAT once-display violation**: re-render attempt → assertion fails (PAT not stored client-side).
2. **DPA acceptance sem versioning**: backend rejects payload sem `dpa_version`.
3. **Stripe webhook replay**: idempotency key check rejects duplicate.
4. **Invite link expired**: redemption rejected com clear error "Invite link expired; request new one".
5. **Revoke all sem MFA fresh**: redirect to MFA re-prompt antes revoke proceeds.
6. **Onboarding abandonment**: métrica incremented; user can resume from saved step.

### Cross-browser
- Smoke test em Chrome + Firefox + Safari + Edge; full matrix em WI-S16-007.

## 16. Failure Modes

- **FM-150** (transient API): Stripe Checkout + PAT create retry com exponential backoff (3 retries default).
- **FM-160** (auth invalid): clear error UI; redirect to Clerk sign-in flow; never echoes PAT em error message.

## 17. Controls

- **CTRL-CRED-001** (no PAT em client logs): PAT once-display + masked após copy + Sentry redaction allowlist + safeLog wrapper.
- **CTRL-AUTH-010** (admin destructive ops fresh MFA ≤ 30 min): revoke-all PAT requires MFA re-auth.
- **CTRL-PRIV-CONSENT-001..006** consumed (DPA acceptance é consent-adjacent; full consent capture em WI-S16-003).

## 18. Resilience Patterns

- Retry transient errors (FM-150): exponential backoff em mutations (PAT create + Stripe Checkout + DPA accept).
- Optimistic UI updates com rollback em falha (PAT create lifecycle).
- Skeleton loaders em SSR auth-gated routes.
- Wizard step persistence (LocalStorage até backend persist; reduces abandonment).

## 19. Observability

UI métricas Prometheus snake_case com `plan` label canonical:
- `corelink_admin_ui_onboarding_step_duration_ms{step}` histogram.
- `corelink_admin_ui_onboarding_completion_total{outcome}` counter (outcome ∈ completed|abandoned).
- `corelink_admin_ui_pat_create_total{scope, outcome}` counter.
- `corelink_admin_ui_pat_revoke_total{type}` counter (type ∈ single|all).
- `corelink_admin_ui_dpa_acceptance_total{dpa_version}` counter.

Cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (NUNCA per-tenant labels).

## 20. Security & Privacy

**STRIDE delta**:
- **Spoofing**: Clerk SSO + WebAuthn + MFA fresh ≤ 30 min em revoke-all; invite link signed JWT prevents impersonation.
- **Tampering**: PAT once-display + masked após copy; DPA acceptance signed JWT receipt verifiable.
- **Repudiation**: DPA acceptance signed JWT receipt + emailed; Stripe webhook idempotency + audit events S-09.
- **Information disclosure**: CTRL-CRED-001 (PAT once-display + masked); Stripe Checkout hosted (no PCI scope client-side).
- **DoS**: PAT revoke-all requires MFA (rate-limited via Clerk).
- **Elevation of privilege**: invite link role validation backend; admin scope warning em PAT picker.

**LINDDUN delta**:
- Linkability: telemetry pageview anonymized; tenant_id never em client logs.
- Identifiability: DPA + Stripe email handled via Clerk; safeLog allowlist.
- Disclosure: PAT NUNCA em URL params ou logs; once-display + masked.

## 21. Dependencies

### Hard blockers
- WI-S16-001 SEALED (skeleton + Clerk + CSP + i18n).
- S-03 SEALED (Clerk auth + PAT format hybrid).
- S-10 SEALED (billing + Stripe webhook handler).

### Soft blockers
- S-13 SEALED (admin plane API for retention policy update).

### Outbound
- WI-S16-003 (consent UI may share lifecycle patterns).
- WI-S16-004 (DSR form may share JWT receipt pattern).
- WI-S16-005 (admin ops UI shares PAT mgmt widget patterns).
- WI-S16-007 (E2E + UX workshop measures time-to-first-PAT).

## 22. Effort PERT

O: 14h, M: 22h, P: 36h → PERT **23.0h** (per spec contract §12; wizard + DPA + Stripe + PAT widget + settings + invite link).

## 23. Cost Analysis

- Stripe Checkout: 2.9% + $0.30 per transaction (variable; not infra cost).
- SES emails: $0.10 / 1000 emails (DPA + JWT receipts; ≤ 1k emails/mês onboarding).
- CF Pages incremental: free tier sufficient.
- Total: ~$10/mês incremental (DPA emails + invite emails).

## 24. Post-mortem Hooks

- PAT exibido > 1x detected → CRITICAL post-mortem + Security review.
- DPA acceptance sem versioning detected → compliance post-mortem + privacy gap fix.
- Stripe webhook idempotency violation → SEV-2 post-mortem.
- Time-to-first-PAT > 10 min sustained > 7d → DX regression post-mortem.
- Invite link replay attack → CRITICAL post-mortem + AppSec review.

## 25. Rollback / Recovery

Onboarding wizard regression detected → revert via CF Pages rollback (previous deploy accessible); Stripe webhook replay protection prevents double-billing.

## 26. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Stripe Checkout integration edge cases | M | L | LOW | L | LOW | Test matrix; Stripe support tier; idempotency keys |
| R-002 | PAT exposure browser dev tools | L | L | HIGH | L | LOW | once-display + masked + Sentry redaction allowlist |
| R-003 | DPA acceptance abandonment | M | M | MEDIUM | M | LOW | Plain-language Flesch-Kincaid ≤ 8; UX workshop iterate |
| R-004 | Invite link replay | L | L | HIGH | L | LOW | signed JWT expiry + nonce + Clerk redemption tracking |
| R-005 | Time-to-first-PAT > 5 min UX miss | M | L | LOW | L | LOW | UX workshop weekly; iterate wizard steps; onboarding video |
| R-006 | Settings retention policy update inconsistency | L | M | MEDIUM | L | LOW | Audit event S-09 + backend transaction; UI confirmation |

## 27. Knowledge Transfer

- Tech talk (1h): "CoreLink Onboarding Wizard + PAT Mgmt Widget".
- Doc `docs/internal/admin-ui-onboarding.md` — overview.
- Onboarding test (3 questions): PAT once-display lifecycle + DPA versioning + invite JWT expiry.

## 28. Sign-off (STANDARD 5-8 canonical; 7 typical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Frontend Lead | _TBD; emphatic — wizard + PAT widget + settings UI_ | _pending_ |
| 4 | QA | _TBD; emphatic — E2E onboarding + revoke-all MFA flow_ | _pending_ |
| 5 | Product | Gustavo Schneiter | _pending_ |
| 6 | Designer/a11y advisor | _TBD; emphatic — wizard UX + plain-language DPA + scope picker UX_ | _pending_ |
| 7 | Privacy officer | _TBD; emphatic — DPA versioning + JWT receipt + invite link audit trail_ | _pending_ |

> STANDARD lane (per framework §33.5.4): 5-8 canonical sign-offs; 7 typical.

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S16-002 (cycle 12.S16.0; tenant onboarding flow + DPA + Stripe + first PAT + settings + invite link). |

## 30. Anti-patterns evitados

- PAT exibido > 1x (CTRL-CRED-001 violation).
- DPA acceptance sem versioning (compliance gap).
- Stripe Checkout custom embed (PCI scope creep).
- Invite link sem signed JWT expiry (security gap).
- Onboarding wizard sem progress indicator (UX gap).
- Time-to-first-PAT > 5 min sustained (DX regression baseline).
- Revoke-all sem MFA re-auth (CTRL-AUTH-010 violation).
- DPA + scope explanations dense legalese (UX abandonment + compliance opaque).

---

**Fim WI-S16-002.**
