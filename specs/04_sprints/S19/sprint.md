---
id: "S-19"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-009"]
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "COMPLIANCE-MATRIX"
  - "PRIVACY-MODEL"
  - "AUTH-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["sprint", "s19", "onboarding", "dpa", "self-service", "enterprise-handoff", "high-risk"]
---

# Sprint S-19 — Customer Onboarding (Self-Service Signup ≤ 3 min + DPA Click-through 6-Field Consent EVT-049 + 3 Locales Legal-Reviewed + Tier Selection + Stripe Subscription Activation INV-ONBOARD-DPA-FIRST + Enterprise Inquiry Form Slack+CRM Atomic + 24h Auto-Reply SLA + DPA Versioning Re-acceptance 30d Grace + Conversion Funnel Cohort Dashboard + INV-ONBOARD-ATOMIC-PROVISIONING + 11 Sign-Offs Canonical Two-Phase SEAL D+15/D+45)

> **doc_status:** DRAFT · **lane:** HIGH_RISK · **Versão:** 1.0.0 · **2026-04-29**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (cycle 9.4 SOTA elevation; codex lane upgrade STANDARD → HIGH_RISK ratificado)

> **Phase boundary:** Fase 5 — Customer onboarding business logic + DPA evidence-grade contract pré-GA scaling.
> **FF-HR-009 specific**: onboarding flow registra DPA click-through (customer-facing legal contract) + ativa billing (subscription state + Stripe customer ID); bug = LGPD/GDPR/CCPA legal exposure + breach notification mandatory + customer trust loss permanent + contract challenge legitimate em court. DPA-first race condition (customer billed sem DPA) ou atomicity bug (orphan tenant + inconsistent billing) = catastrophic legal posture; DPA semver bump missed para material change = retroactive consent invalidation risk.

---

## 1. Objetivo

Operacionalizar **customer onboarding business logic end-to-end** com qualidade enterprise-ready: self-service signup orchestration (Clerk email verify integration; tenant + DPA + Stripe customer ID em single D1 transaction com rollback consistency under chaos) → DPA click-through 6-field consent capture (CTRL-PRIV-CONSENT-001..006 reflection per S-11/S-16 pattern; locale from rendered `corelink_locale` cookie pós Lote 10.16 fix; EVT-049 evidence retention 7y; cryptographic JWT receipt signed per-region key) → tier selection com Stripe Checkout subscription activation (S-10 reuse; race condition prevention via D1 lock + INV-ONBOARD-DPA-FIRST canonical) → first-run experience (first PAT + CLI install command + quickstart link + sample tenant) → enterprise inquiry form + white-glove handoff (Slack `#sales-leads` + CRM atomic; 24h auto-reply SLA; Sales engagement workflow doc) → DPA versioning + re-acceptance flow (semver; major bump = email broadcast + 30d grace; failure post-grace → tenant degrade read-only) → conversion funnel instrumentation (7 steps × 3 regions × 5 tiers = 105 series respeitando INV-OBS-CARDINALITY-BUDGET).

Decomposição cumulativa em 6 WIs: (1) **WI-S19-001** signup orchestration backend + atomic provisioning (Clerk webhook + tenant + DPA + Stripe customer em single D1 tx; first PAT auto-gen scope rw 90d expiry; rollback logic; chaos test Stripe outage during signup → 0 orphan tenant; INV-ONBOARD-ATOMIC-PROVISIONING canonical); (2) **WI-S19-002** DPA click-through 6-field + JWT receipt + 3 locales (en-US/pt-BR/es-419) Legal local review + EVT-049 evidence + scroll-gate anti-dismiss UX; (3) **WI-S19-003** DPA versioning + re-acceptance flow (semver; major bump email broadcast + 30d grace + degrade read-only post-grace; v1→v2 simulated cycle); (4) **WI-S19-004** tier selection + Stripe Checkout activation + INV-ONBOARD-DPA-FIRST (D1 lock; property test 10k concurrent signup 0 customer billed sem DPA); (5) **WI-S19-005** enterprise inquiry form + Slack + CRM atomic handoff + 24h auto-reply SLA + Sales workflow doc `docs/internal/sales-handoff.md`; (6) **WI-S19-006** closing — conversion funnel cohort dashboard + property tests 10k DPA-first/atomicity aggregated + RB-FM-SIGNUP-FAILED stub + adversarial summary + PRR 11 sign-offs canonical. Mitiga **FM-160** (auth invalid no signup), **FM-151** (Stripe outage during checkout), **FM-X-DPA-LEGAL-CHALLENGE** (DPA template contestado em court). Implementa CTRL-PRIV-CONSENT-001..006 reuso S-11/S-16 + ratifica INV-ONBOARD-DPA-FIRST (HIGH §3.12) + INV-ONBOARD-ATOMIC-PROVISIONING (HIGH §3.12) + reforça INV-CONSENT-PROOF-VERIFIABLE (CRITICAL §3.12 herdada S-11) + respeita INV-OBS-CARDINALITY-BUDGET (§3.12 herdada S-09).

**Por que SOTA**: competitors (Stripe Atlas, Auth0 self-service, Linear onboarding) entregam onboarding com DPA via support email manual ou DPA click-through advisory only. CoreLink S-19 entrega: (a) **DPA click-through com 6-field consent capture** (CTRL-PRIV-CONSENT-001..006) + cryptographic JWT receipt verifiable post-facto via S-11 verify endpoint; (b) **signup ≤ 3 min start-to-finish** measured via 5-dev workshop weekly + 30d observation window; (c) **abandon rate per-step** instrumented com cohort analysis dashboard; (d) **enterprise handoff Slack→CRM atomic** com retry queue + 24h auto-reply; (e) **DPA versioning re-acceptance** semver-based com 30d grace + degrade read-only graceful (zero competitors); (f) **3 locales Legal local review** per locale (NÃO automated translation). Reference: **Stripe Atlas onboarding** (gold standard), **Auth0 self-service**, **Linear onboarding flow**, **GDPR Art. 28** (Processor obligations), **LGPD Art. 39** (Operator), **EDPB SCCs**, **NIST Privacy Framework 1.0**, **WCAG 2.2 AA**, **CloudEvents v1.0.2** (DPA acceptance event).

**Codex finding lane upgrade ratificado** (spec contract §1): STANDARD → HIGH_RISK porque onboarding é customer-facing legal contract (DPA + Terms) + billing activation = legal exposure inaceitável. **FF-HR-009** ativo. **Codex finding ownership clash ratificado** (spec contract §1): S-16 owns frontend UI (`CAP-UI-001` "Tenant onboarding flow"); S-19 owns business logic + orchestration; separation explicit em §10 anti-scope.

## 2. Escopo

### 2.1 In-scope

- **WI-S19-001**: Signup orchestration backend (`crates/corelink-onboarding/`) — Clerk webhook handler email verify; tenant provisioning atomic em D1 single transaction (tenant + tenant_metadata + usage_counter + dpa_acceptance_pending + stripe_customer_id placeholder); first PAT auto-generated scope read-write 90d expiry shown only-once; region pinned per `corelink_locale` cookie ou explicit selection (US default; EU detected via locale); rollback logic on Clerk failure ou D1 tx failure; chaos test Stripe outage during signup → tenant rollback consistent (no orphan tenant + no inconsistent billing); INV-ONBOARD-ATOMIC-PROVISIONING ratificada via property test 10k atomicity violations.
- **WI-S19-002**: DPA click-through implementation (`crates/corelink-onboarding-dpa/` + `apps/web/src/app/onboarding/dpa/`) — DPA template v1 em `legal/dpa/v1.0.0.{en-US,pt-BR,es-419}.md` (3 locales) + Legal local review per locale (NÃO automated translation; mandatory Legal sign-off per locale before publish em `specs/_audits/2026-XX-XX-legal-review-dpa-v1.md`); UI render full text + scroll-gate (button disabled until user scrolls to bottom; anti-dismiss UX); 6-field consent payload (`notice_text_hash` sha256(dpa_full_text) calculado em frontend Web Crypto API + `notice_version` semver from frontmatter + `locale` from **rendered `corelink_locale` cookie** Lote 10.16 fix + `wording_id` deterministic UUID v7 from notice_version + `ui_capture_ts` browser Date.now() + `submission_ts` server-assigned); EVT-049 evidence captured imutável em R2 audit bucket retention 7y; signed JWT receipt HS256 per-region key (separate signing key + `key_id` index per data_model.md §4.1; NÃO confundir com PAT format S-03 hybrid HMAC + Argon2id); reuse S-11 consent ledger + verify endpoint pattern.
- **WI-S19-003**: DPA versioning + re-acceptance flow (`crates/corelink-onboarding-dpa-versioning/`) — Semver discipline (major bump = material change requires re-acceptance; minor/patch = no re-acceptance); Legal review on every PR touching DPA + quarterly audit; major bump triggers existing tenants email broadcast + 30d grace period to re-accept; in-app banner persistent during grace; re-acceptance failure post-grace → tenant degrade read-only com email + in-app warning + customer support escalation path; v1→v2 simulated cycle test em staging; PAT-DEGRADE-001 reuse pra tenant graceful degrade.
- **WI-S19-004**: Tier selection + Stripe Checkout subscription activation (`crates/corelink-onboarding-billing/`) — Tier picker UI (S-16 frontend) integrado Stripe Checkout (S-10 reuse) com 5 tiers visible (free/starter/team/pro/enterprise — enterprise = "Contact us"); Stripe Checkout redirect com tenant_id mapped to Stripe customer; webhook handler (S-10 reuse) atualiza local subscription state; **INV-ONBOARD-DPA-FIRST canonical**: subscription activation requires DPA signed primeiro; race condition prevented via D1 lock + transactional check (`SELECT dpa_signed_ts FROM tenant WHERE tenant_id = ? FOR UPDATE`); property test 10k concurrent signup attempts → 0 customer billed sem DPA.
- **WI-S19-005**: Enterprise inquiry form + Slack + CRM atomic handoff (`crates/corelink-onboarding-enterprise/`) — Form fields: company + role + expected GB/mo + BYOK requirements + residency requirements + reCAPTCHA; submit → Slack notification (`#sales-leads` channel via webhook) + CRM entry (HubSpot or equivalent via API) **atomic** (both succeed or both rollback via retry queue + saga pattern); auto-reply email com white-glove timeline (24h response SLA hard) ≤ 5 min via SES; Sales engagement workflow documented em `docs/internal/sales-handoff.md` (lead triage decision tree + qualification checklist + follow-up cadence); rate limit + reCAPTCHA + bot detection mitigates spam abuse.
- **WI-S19-006**: Closing WI — conversion funnel instrumentation (Prometheus snake_case underscore métricas: `corelink_onboarding_step_started_total{step,region,plan}` + `corelink_onboarding_step_completed_total{step,region,plan}` + `corelink_onboarding_step_abandoned_total{step,reason,region,plan}` + 4 mais; cardinality budget 7 steps × 3 regions × 5 tiers = 105 séries respeitando INV-OBS-CARDINALITY-BUDGET; **NUNCA per-tenant labels**); cohort analysis dashboard DASH-ONBOARDING (S-09 alignment dashboards-as-code); property tests 10k aggregated (DPA-first INV-ONBOARD-DPA-FIRST + atomicity INV-ONBOARD-ATOMIC-PROVISIONING + cross-WI integration); RB-FM-SIGNUP-FAILED stub em `specs/05_runbooks/RB-FM-SIGNUP-FAILED.md` se signup atomicity falha em prod; adversarial summary aggregation 25+ scenarios; PRR HIGH_RISK 11 sign-offs canonical doc `specs/04_sprints/S19/PRR-S19.md`.

### 2.2 Anti-scope

- Frontend UI (S-16 owns UI surfaces; S-19 owns business logic + orchestration backend).
- Enterprise MSA negotiation automation (manual com Legal; S-19 entrega inquiry form + handoff workflow).
- Trial period logic (tier `free` já serve; sem timed trial complications).
- Customer success automation (CSM tier — pós-GA Q1).
- Email marketing automation (Sales engagement manual via CRM).
- Multi-step manual approval flow (auto-approve all signups; abuse handled separately via S-08 rate limit + S-11 DSR).
- Custom DPA per customer (single DPA template at GA; enterprise custom é S-14 BYOK + manual Legal).
- Referral program / affiliate tracking (pós-GA Q1).
- DPA template auto-translation (Legal local review per locale mandatory; manual translation only).
- Multi-DPO escalation workflow (single DPO em S-19; enterprise multi-DPO pós-GA).
- Customer-facing onboarding analytics dashboard (internal-only em S-19; customer-facing pós-GA).

## 3. Customer Impact & Journey

**JTBD:** "Como customer prospect signing up CoreLink, preciso experiência onboarding ≤ 3 min start-to-finish (de email submit até first CAS PUT), com **DPA click-through evidence-grade** que posso re-verificar post-facto via verify endpoint público (CTRL-PRIV-CONSENT-001..006 reflection); **billing activation atomic** com tenant provisioning (no orphan tenant; no inconsistent billing); **3 locales nativos** (en-US/pt-BR/es-419) Legal-reviewed; **first PAT** + CLI install command + quickstart link rendered em first-run; **enterprise inquiry** com white-glove 24h SLA auto-reply + Sales engagement orchestrated. Como Privacy Officer cliente / Compliance auditor, preciso evidence pack 6-field consent + JWT receipt verifiable + EVT-049 retention 7y + DPA versioning semver discipline + re-acceptance flow 30d grace evidence-grade. Como Sales lead, preciso enterprise inquiry handoff Slack + CRM atomic com retry queue + 24h SLA auto-reply email + workflow doc qualifying leads."

**CAPs entregues:** CAP-ONBOARD-001 (signup business logic + atomicity) + CAP-ONBOARD-002 (DPA click-through 6-field consent) + CAP-ONBOARD-003 (tier + Stripe subscription) + CAP-ONBOARD-004 (first-run experience) + CAP-ONBOARD-005 (enterprise inquiry + handoff) + CAP-ONBOARD-006 (conversion funnel instrumentation) + CAP-ONBOARD-007 (DPA versioning + re-acceptance).

**Persona 1 — Customer prospect signing up self-service**:
- Signup ≤ 3 min sustained measured via 5-dev workshop weekly + 30d observation window GA Evidence Gate D+45.
- DPA click-through: full text rendered em locale ativa (cookie corelink_locale set by Next.js middleware); scroll-gate anti-dismiss; 6-field consent capture; JWT receipt downloadable + emailed.
- Tier selection: 5 tiers visible; free tier instant activation; paid tiers Stripe Checkout redirect.
- First-run: first PAT shown only-once; CLI install command rendered (`curl -fsSL https://corelink.dev/cli | sh`); quickstart link `docs.corelink.dev/quickstart` (S-18); optional in-app guided tour.

**Persona 2 — Privacy Officer cliente / Compliance auditor**:
- 6-field consent payload audit trail forensic-grade; backend S-11 consent verify endpoint R-S11-9 retorna match.
- EVT-049 evidence retention 7y em R2 audit bucket.
- DPA versioning semver discipline + Legal review on every PR touching DPA + quarterly audit.
- DPA re-acceptance flow 30d grace + degrade read-only post-grace evidence-grade.
- Compliance Matrix mapping: GDPR Art. 28 (Processor obligations DPA basis) + LGPD Art. 39 (Operator) + CCPA §1798.140(v) (service provider) + EDPB SCCs + NIST Privacy Framework 1.0 + WCAG 2.2 AA (a11y).

**Persona 3 — Sales lead (enterprise inquiry handoff)**:
- Inquiry form submit triggers Slack `#sales-leads` notification ≤ 1 min + CRM entry (HubSpot/equivalent) atomic; failure either side rolls back via retry queue.
- Auto-reply email ≤ 5 min via SES com white-glove timeline (24h response SLA hard).
- Sales engagement workflow documented em `docs/internal/sales-handoff.md` (lead triage + qualification + follow-up cadence).

**SLA addendum**:
- Signup ≤ 3 min start-to-finish (5-dev sample weekly + 30d observation).
- Conversion funnel sustained 30d staging (abandon rate per-step visible).
- DPA evidence verifiable post-facto via consent verify endpoint S-11 retorna match.
- DPA re-acceptance flow sustained 1 cycle (v1 → v2 mock).
- Enterprise handoff atomicity: Slack + CRM both succeed or both rollback.
- 5 real signups em closed beta successful (DoD GA Evidence Gate D+45).
- Enterprise auto-reply ≤ 5 min; Sales engagement ≤ 24h SLA.
- 3 DPA locales Legal local review per locale (en-US + pt-BR + es-419).

## 4. Capability Mapping (trace)

Ver `_spec_contract.md §4`. Foundation: `privacy_model.md §5.6` (CTRL-PRIV-CONSENT-001..006 + DPA é consent type) + `auth_model.md` (Clerk SSO email verify + MFA setup) + `observability_model.md §3.1` (Prometheus snake_case + cardinality budget INV-OBS-CARDINALITY-BUDGET; NUNCA per-tenant labels) + `failure_modes.md` (FM-160 + FM-151 + novo FM-X-DPA-LEGAL-CHALLENGE stub) + `invariant_registry.md §3.12` (INV-CONSENT-PROOF-VERIFIABLE herdada S-11 + INV-ONBOARD-DPA-FIRST + INV-ONBOARD-ATOMIC-PROVISIONING novas; INV-OBS-CARDINALITY-BUDGET herdada S-09) + `data_model.md §4.1` (DPA receipt JWT signing key separate + key_id index; NÃO confundir com PAT format S-03) + `compliance_matrix.md` (SOC 2 + LGPD + GDPR + CCPA).

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S19-D1 | Signup orchestration backend + atomic provisioning + first PAT + chaos test | `crates/corelink-onboarding/` + `tests/onboarding_atomicity_proptest.rs` + `migrations/0XX_signup_atomic.sql` | Clerk webhook handler; tenant + DPA + Stripe customer single D1 tx; first PAT auto-gen 90d; rollback logic; chaos test Stripe outage 0 orphan tenant; property test 10k atomicity 0 violations; INV-ONBOARD-ATOMIC-PROVISIONING ratificada |
| S19-D2 | DPA click-through + 6-field consent + JWT receipt + 3 locales + Legal review | `crates/corelink-onboarding-dpa/` + `apps/web/src/app/onboarding/dpa/` + `legal/dpa/v1.0.0.{en-US,pt-BR,es-419}.md` + `specs/_audits/2026-XX-XX-legal-review-dpa-v1.md` | DPA template 3 locales Legal local-reviewed (NÃO automated); UI render + scroll-gate; 6-field consent capture (locale from rendered cookie pós Lote 10.16); JWT receipt HS256 per-region key; EVT-049 R2 retention 7y; reuse S-11 verify endpoint pattern |
| S19-D3 | DPA versioning + re-acceptance flow + 30d grace + degrade read-only | `crates/corelink-onboarding-dpa-versioning/` + `apps/web/src/app/dpa-banner/` | Semver bump logic; major bump email broadcast existing tenants + 30d grace + in-app banner persistent; re-acceptance failure post-grace → tenant degrade read-only PAT-DEGRADE-001 reuse; v1→v2 simulated cycle staging; Legal review every PR + quarterly audit |
| S19-D4 | Tier selection + Stripe Checkout + INV-ONBOARD-DPA-FIRST atomicity | `crates/corelink-onboarding-billing/` + `tests/dpa_first_proptest.rs` | Tier picker (5 tiers); Stripe Checkout redirect (S-10 reuse); subscription activation; D1 lock DPA-first transactional check; property test 10k concurrent signup 0 customer billed sem DPA; INV-ONBOARD-DPA-FIRST ratificada |
| S19-D5 | Enterprise inquiry form + Slack + CRM atomic + 24h auto-reply + Sales workflow doc | `crates/corelink-onboarding-enterprise/` + `apps/web/src/app/contact-sales/` + `docs/internal/sales-handoff.md` | Form backend; Slack webhook `#sales-leads`; CRM API atomic via saga + retry queue; auto-reply email ≤ 5 min via SES; 24h Sales SLA; reCAPTCHA + rate limit; sales handoff doc lead triage + qualification + follow-up cadence |
| S19-D6 | Conversion funnel + cohort dashboard + property tests aggregated + RB stub + PRR | `crates/corelink-onboarding-funnel/` + `infra/grafana/dashboards/dash-onboarding.json` + `specs/05_runbooks/RB-FM-SIGNUP-FAILED.md` + `specs/04_sprints/S19/PRR-S19.md` | 7 steps × 3 regions × 5 tiers = 105 séries cardinality safe; cohort dashboard DASH-ONBOARDING; property tests 10k aggregated DPA-first + atomicity + cross-WI; RB-FM-SIGNUP-FAILED stub; adversarial summary 25+ scenarios; PRR 11 sign-offs canonical; sustained 30d observation window GA Evidence Gate D+45 |

## 6. Escopo técnico por camada (inherits_from)

### 6.1 Auth Model (herda `auth_model.md`)

- Clerk email verify orchestration (Clerk webhook handler em `crates/corelink-onboarding/`; tenant provisioning triggered apenas após email verified event).
- TenantCtx propagation: signup creates tenant + initial admin user role; subsequent operations carry `tenant_id` em JWT claims (RS256 verified).
- First PAT auto-generated com scope read-write + 90d expiry; shown only-once UI surface (S-16 R-S16-8 herdada); INV-AUTH-PII-ENCRYPTED herdada S-03 — email cipher BYTEA via pgcrypto.
- DPA acceptance requires user explicit click (NÃO auto-submit; LGPD Art. 8º + GDPR Art. 7§2 alignment); WebAuthn UV=1 step-up NOT required at signup (deferred to DSR/admin flows).
- Enterprise inquiry form auth: anonymous submission acceptable (no Clerk session required at this stage; reCAPTCHA + rate limit + email confirm only).

### 6.2 Privacy Model (herda `privacy_model.md §5.6`)

- **CTRL-PRIV-CONSENT-001** (opt-in proof of informed) — IMPLEMENTA primary via DPA click-through 6-field consent capture + JWT receipt verifiable.
- **CTRL-PRIV-CONSENT-002** (revogação imediata ≤ 5min cascade) — herdada S-11; DPA revoke flow integrated via DSR (S-11 alignment).
- **CTRL-PRIV-CONSENT-003** (audit immutable) — herdada S-09; EVT-049 emit em R2 audit bucket Object Lock 7y retention.
- **CTRL-PRIV-CONSENT-004** (LIA legitimate interest assessment) — N/A para DPA (basis = consent obrigatório DPA contract).
- **CTRL-PRIV-CONSENT-005** (notice versioning) — IMPLEMENTA primary via DPA semver discipline + Legal review every PR + 3 locales Legal local-reviewed; locale match canonical from rendered `corelink_locale` cookie pós Lote 10.16 fix (NÃO Accept-Language header).
- **CTRL-PRIV-CONSENT-006** (proof of display screenshot opcional) — IMPLEMENTA via reuse S-16 WI-S16-003 screenshot evidence pattern (html2canvas client-side primary + server-side headless Chrome fallback; R2 retention 7y).
- LGPD Art. 8º (consentimento) + GDPR Art. 7 (conditions for consent) + GDPR Art. 28 (Processor obligations DPA basis) + EDPB SCCs alignment.

### 6.3 Observability Model (herda `observability_model.md §3.1`)

Métricas underscored snake_case com label `plan` aplicável; **NUNCA per-tenant labels** (INV-OBS-CARDINALITY-BUDGET); cardinality budget 7 steps × 3 regions × 5 tiers = 105 séries por métrica; ≤ 20k séries totais respeitado:

- `corelink_onboarding_step_started_total{step, region, plan}` (counter; step ∈ signup_start|email_verified|dpa_signed|tier_selected|stripe_activated|first_pat_created|first_cas_put).
- `corelink_onboarding_step_completed_total{step, region, plan}` (counter).
- `corelink_onboarding_step_abandoned_total{step, reason, region, plan}` (counter; reason ∈ timeout|user_canceled|error|browser_close).
- `corelink_onboarding_step_duration_seconds_bucket{step, region, plan}` (histogram p50/p95/p99 per step).
- `corelink_onboarding_signup_total_duration_seconds_bucket{outcome, region, plan}` (histogram; outcome ∈ completed|abandoned).
- `corelink_onboarding_dpa_acceptance_total{locale, version, plan}` (counter).
- `corelink_onboarding_dpa_re_acceptance_total{from_version, to_version, outcome, plan}` (outcome ∈ accepted|grace_expired_degraded|grace_expired_blocked).
- `corelink_onboarding_atomicity_rollback_total{reason, plan}` (reason ∈ stripe_outage|d1_failure|clerk_failure; alert > 5/dia).
- `corelink_onboarding_dpa_first_violation_total{plan}` (counter; alert > 0 — should never happen).
- `corelink_onboarding_enterprise_inquiry_total{outcome, plan}` (outcome ∈ slack_crm_atomic_ok|slack_crm_partial_rolled_back|spam_blocked).
- `corelink_onboarding_enterprise_auto_reply_duration_seconds_bucket{plan}` (histogram p99 ≤ 5 min).
- `corelink_onboarding_consent_locale_mismatch_total{plan}` (counter; alert > 5/dia indica cookie drift).

### 6.4 SLOs

- **SLO-ONBOARD-SIGNUP-DURATION** (novo): signup p99 ≤ 3 min start-to-finish sustained 30d observation window.
- **SLO-ONBOARD-DPA-RECEIPT-VERIFIABILITY** (novo): consent verify endpoint round-trip integrity ≥ 99.9%.
- **SLO-ONBOARD-ENTERPRISE-AUTO-REPLY** (novo): auto-reply email p99 ≤ 5 min.
- **SLO-ONBOARD-ATOMICITY** (novo): rollback consistency 100% (zero orphan tenants em chaos drill weekly).

### 6.5 Failure Modes (herda `failure_modes.md`)

- **FM-160** (auth invalid no signup) — Clerk email verify failure → clear error UI + retry path; audit emit `corelink.onboarding.auth_invalid`.
- **FM-151** (Stripe outage during checkout) — RB-FM-151 reuse S-10; chaos test verifica rollback consistency; tenant marked dpa_acceptance_pending mas billing pending; retry queue + customer notify.
- **FM-X-DPA-LEGAL-CHALLENGE** (novo stub) — DPA template contestado em court; runbook stub `RB-FM-DPA-LEGAL-CHALLENGE` em `specs/05_runbooks/`; Legal externo escalation + customer notify + retroactive re-acceptance broadcast se invalidated.

### 6.6 Resilience Patterns (herda `resilience_patterns.md`)

- **PAT-CORRELATION-ID-001**: signup flow correlation_id propagation across Clerk webhook + D1 tx + Stripe Checkout + first PAT + first CAS PUT (debug + audit chain integrity).
- **PAT-DEGRADE-001**: DPA re-acceptance grace expired → tenant degrade read-only graceful (no hard 503; explicit warning + customer support escalation).
- **PAT-SAGA-001** (reuse S-10): enterprise inquiry Slack + CRM atomic via saga + retry queue.

## 7. Definition of Done (lane HIGH_RISK two-phase SEAL)

> **Two-phase SEAL** (per Timeline §9): items verificáveis instantaneamente fecham em **Implementation SEAL D+15**; items requerendo "30d sustained 5 dev signups + DPA re-acceptance flow 1 cycle + signup ≤ 3 min sustained" janela fecham em **GA Evidence Gate D+45**. Ambos SEALs canônicos; sprint considerado concluído apenas após GA Evidence Gate D+45.

- [ ] **WIs SEALED**: 6/6 (EVT-031).
- [ ] **E2E signup flow** ≤ 3 min start-to-finish (5-dev sample) (EVT-018).
- [ ] **DPA click-through** gera evidence imutável (EVT-049) + signed JWT receipt + verifiable backend (EVT-049 + EVT-024).
- [ ] **Stripe subscription** activa em team tier após DPA signed (E2E test) (EVT-018).
- [ ] **Atomicity test**: simulate Stripe outage during signup → tenant rolls back consistently (chaos test) (EVT-023) — INV-ONBOARD-ATOMIC-PROVISIONING ratificada.
- [ ] **DPA-first race condition test**: 10k concurrent signup attempts → 0 customer billed sem DPA (EVT-002) — INV-ONBOARD-DPA-FIRST ratificada.
- [ ] **Enterprise inquiry flow** testado com 1 fake lead → Slack + CRM atomic → 24h auto-reply email (EVT-018).
- [ ] **Conversion funnel** dashboard live em Grafana (S-09); 7 steps instrumentado (EVT-021).
- [ ] **DPA versioning re-acceptance** test: bump v1 → v2 → tenant prompted; refused → degrade read-only após 30d grace (EVT-018).
- [ ] **DPA 3 locales** native speaker + Legal local reviewed (en-US + pt-BR + es-419) (EVT-044).
- [ ] **Signup form a11y** AA (axe-core 0 violations) (EVT-018).
- [ ] **Zero secrets em URL** (no PAT, no consent token, no session token) — fuzz test 1k random params (EVT-002).
- [ ] **PRR HIGH_RISK 11 sign-offs canonical**: Owner + Final Approver + Architect + Privacy Officer + Legal + Engineer + QA + Product + SRE Lead + Compliance Officer + Sales lead + Privacy/UX advisor (Crypto SME folds into Architect; per spec contract §6 + framework §33.5.4.3 + ADR-0034 solo-tier waiver).
- [ ] **Runbook**: cria RB-FM-SIGNUP-FAILED stub se signup atomicity falha em prod (EVT-017).
- [ ] **CTRL-PRIV-CONSENT-001..006** reflected (UI capture per S-16 WI-S16-003 pattern; backend S-11 enforce).
- [ ] **INV-CONSENT-PROOF-VERIFIABLE** (CRITICAL — registry §3.12 herdada S-11) reforced via DPA verify endpoint round-trip.
- [ ] **INV-ONBOARD-DPA-FIRST** (HIGH — registry §3.12 nova) ratificada — D1 lock + transactional check + property test 10k concurrent.
- [ ] **INV-ONBOARD-ATOMIC-PROVISIONING** (HIGH — registry §3.12 nova) ratificada — D1 transaction + chaos test Stripe outage.
- [ ] **INV-OBS-CARDINALITY-BUDGET** (registry §3.12 herdada S-09) — funnel métricas 7 steps × 3 regions × 5 tiers = 105 séries respeitado; NUNCA per-tenant labels.
- [ ] **10.s19.1** Conversion funnel métrica (signup start → first usage) instrumentada (EVT-021).
- [ ] **10.s19.2** Abandon rate per-step visível em dashboard DASH-ONBOARDING (S-09 alignment) (EVT-021).
- [ ] **10.s19.3** Signup ≤ 3 min sustained 30d staging (5-dev sample weekly) *(GA Evidence Gate D+45)*.
- [ ] **10.s19.4** DPA evidence verifiable post-facto (consent verify endpoint S-11 retorna match) (EVT-024).
- [ ] **10.s19.5** DPA re-acceptance flow sustained 1 cycle (v1 → v2 mock) *(GA Evidence Gate D+45)*.
- [ ] **10.s19.6** Enterprise handoff atomicity Slack + CRM (atomic; bug em either rolls back) (EVT-023).
- [ ] **10.s19.7** 5 real signups em closed beta sucesso *(GA Evidence Gate D+45)* (EVT-018).
- [ ] **Cost regression gate**: onboarding adds ≤ 5% overhead em signup path; CRM API + Slack webhook + SES auto-reply ≤ $50/mês total.
- [ ] **Métricas underscored Prometheus**: 12+ funnel + DPA + enterprise metrics emitting em staging com label `plan` aplicável (NUNCA per-tenant labels per INV-OBS-CARDINALITY-BUDGET) (EVT-013).

## 8. Dependencies

### Hard blockers

- **S-03 SEALED** (auth Clerk + email verify orchestration foundation).
- **S-10 SEALED** (billing Stripe + subscription activation foundation).
- **S-11 SEALED** (DPA é consent type; consent ledger D1 + verify endpoint R-S11-9 reuse; 6-field schema canonical).
- **S-16 SEALED** (frontend UI surfaces; consent UI 6-field pattern WI-S16-003 reuse pós Lote 10.16 fix).
- **S-18 SEALED** (docs quickstart link `docs.corelink.dev/quickstart` referenced em first-run).

### Soft blockers

- **S-09 SEALED** (conversion funnel instrumented; DASH-ONBOARDING grafana; audit chain R2 EVT-049 retention).
- **S-13 SEALED** (admin plane support para enterprise inquiry handoff config flags + secret rotation Slack webhook + CRM API key).
- **S-14 SEALED** (BYOK + DPA enterprise; lighthouse customer enterprise tier consome BYOK setup wizard + DPA amendment Schrems II TIA do S-14 — opcional para 1 enterprise lighthouse).

### Outbound

- **S-20** (GA exige 5 real signups em closed beta successful + 1 enterprise inquiry tested + DPA Legal sign-off em 3 locales + atomicity property test 10k 0 violations + conversion funnel sustained 30d).

## 9. Timeline

- **Sprint kick-off**: D+0 (após S-11 + S-16 + S-18 SEALED).
- **D+4**: WI-S19-001 SEALED (signup orchestration + atomicity).
- **D+7**: WI-S19-002 SEALED (DPA + Legal review 3 locales).
- **D+9**: WI-S19-003 SEALED (DPA versioning + re-acceptance).
- **D+11**: WI-S19-004 + WI-S19-005 SEALED (Stripe DPA-first + enterprise handoff).
- **D+13**: WI-S19-006 SEALED (funnel + property tests + RB stub + PRR draft).
- **D+15**: Sprint review + 11 sign-offs canonical PRR coletados + **Implementation SEAL ceremony** (todos WIs entregues + tooling em produção + zero P0/P1 abertos + property tests verde).
- **D+15..D+45**: **Observation window (30d sustained evidence)** — signup ≤ 3 min sustained 5-dev workshop weekly + DPA re-acceptance flow 1 cycle simulado v1→v2 + 5 real signups closed beta successful + conversion funnel sustained 30d staging + enterprise handoff atomicity weekly chaos drill verde. Métricas coletadas continuously; nenhum WI re-aberto exceto fix-critical.
- **D+45**: **GA Evidence Gate SEAL** (sprint sign-off final) — DoD ship-gate criteria validados com janela 30d real (não-simulada); signup duration sustained, DPA re-acceptance cycle comprovado, 5 real signups closed beta, funnel sustained, enterprise handoff atomicity sustained todos comprovados via DASH-ONBOARDING. Implementation já SEALED em D+15; este gate libera S-20 GA dependencies.
- **Total**: 2.5 semanas implementação (12 dias úteis) + 30d observation window + GA Evidence Gate D+45 + buffer 3 dias.

## 10. Risk Register

Ver `_spec_contract.md §15` (12 riscos 6-col com Owner per item: DPA legal review atrasa, Stripe Checkout quirks regiões, Email deliverability Clerk, Atomicity bug signup partial state, DPA-first race condition, DPA re-acceptance flow breaks tenants, Enterprise handoff Slack+CRM partial failure, Conversion funnel false signal bot signups, Translation legal terms incorrect pt-BR/es, Signup > 3 min UX miss, Enterprise inquiry abuse spam, DPA semver bump missed material change).

## 11. Observability Plan

DASH-ONBOARDING (novo dashboard):

- Conversion funnel 7-step waterfall (start → first_cas_put) com abandon rate per-step.
- Signup duration p50/p95/p99 per region (histogram).
- DPA acceptance rate per locale (en-US/pt-BR/es-419 stacked).
- DPA re-acceptance flow status (current version + grace period countdown + degraded tenants count).
- Atomicity rollback events timeline (chaos drill + production incidents).
- DPA-first violation alert > 0 (should never happen; SEV-1 if triggered).
- Enterprise inquiry funnel (form submit → Slack + CRM atomic → auto-reply ≤ 5 min → Sales engagement ≤ 24h).
- Cohort analysis: weekly cohort signup → activation rate (first CAS PUT within 7d).

Métricas listadas em §6.3 (12+); todas com label `plan` aplicável; cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (≤ 20k séries únicas per métrica; ≤ 100k total; **NUNCA per-tenant labels** — funnel é region-tier-step labeled cardinality safe).

## 12. Security & Privacy

**STRIDE delta** (vs S-16 baseline):
- **Spoofing**: Clerk email verify mandatory before tenant provisioning; reCAPTCHA on enterprise inquiry; bot detection mitigates fake signups; rate limit per-IP.
- **Tampering**: 6-field consent payload tamper-detected via backend re-compute notice_text_hash + reject mismatch; JWT receipt HS256 signed per-region key; D1 transaction atomicity prevents partial state injection.
- **Repudiation**: 6-field consent payload + JWT receipt + EVT-049 evidence + R2 retention 7y forensic-grade trail; DPA verify endpoint público stateless via S-11 verify pattern.
- **Information disclosure**: zero secrets em URL (no PAT, no consent token, no session token; fuzz test 1k random params); safeLog allowlist; consent payload references user_id_hash (sha256 server-side); enterprise inquiry form rate-limited.
- **DoS**: signup rate-limited (≤ 10/min per IP); enterprise inquiry rate-limited (≤ 3/hora per IP); Stripe Checkout rate respeitando Stripe API limits; D1 lock contention bounded.
- **Elevation of privilege**: signup creates tenant + initial admin user role; subsequent role changes require admin step-up (S-13); DPA acceptance requires explicit user click (no auto-submit dark patterns; LGPD Art. 8º + GDPR Art. 7§2 alignment).

**LINDDUN delta** (vs S-11/S-16 baseline):
- **Linkability**: tenant_id em audit é necessário (compliance accountability); region em métrica é tier-labeled NÃO tenant-labeled (cardinality budget); cohort analysis via aggregated weekly cohorts (k-anon).
- **Identifiability**: signup payload references user_id_hash (sha256 server-side); email cipher BYTEA via pgcrypto (S-03); enterprise inquiry stores company + role + email (intentional; LGPD Art. 7º legitimate interest sales engagement).
- **Non-repudiation**: 6-field consent payload + JWT receipt verifiable post-facto via S-11 verify endpoint; EVT-049 retention 7y; chain integrity preserved across DPA semver versioning (legal review every PR + quarterly audit).
- **Detectability**: DPA-first violation alert > 0 (SEV-1; should never happen); locale mismatch alerted > 5/dia; atomicity rollback alerted > 5/dia.
- **Disclosure of information**: DPA template Legal-reviewed em 3 locales; SOC 2 + LGPD + GDPR + CCPA evidence pack assembled; transparency page renders DPA full text per locale.
- **Unawareness**: customer notified per DPA major version bump via email broadcast + 30d grace period + in-app banner; enterprise inquiry auto-reply ≤ 5 min; consent revoke flow integrated DSR (S-11 alignment).
- **Non-compliance**: GDPR Art. 7 + Art. 28 + LGPD Art. 8º + Art. 39 + CCPA §1798.140(v) + EDPB SCCs + NIST Privacy Framework 1.0 + WCAG 2.2 AA satisfied.

## 13. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc** (per `_spec_contract.md §18`):

- DPA-first race condition (subscription activated sem DPA) → CRITICAL post-mortem + Legal review + customer notification.
- Atomicity bug signup (orphan tenant) → CRITICAL post-mortem + INV-ONBOARD-ATOMIC-PROVISIONING review.
- DPA re-acceptance flow breaks existing tenant → 5-Why + customer success outreach.
- Enterprise handoff Slack + CRM partial failure (silent) → post-mortem + retry queue strengthen.
- Conversion funnel anomaly (signup spike + 0% activation) → post-mortem (bot abuse) + bot detection strengthen.
- DPA semver bump missed for material change → CRITICAL post-mortem + Legal + retroactive re-acceptance broadcast.
- DPA legal challenge from customer em court → post-mortem com Legal externo + customer trust review.
- Signup p99 > 3 min sustained 7d → review UX + iterate flow.
- Enterprise auto-reply > 5 min sustained 7d → review SES configuration + Sales SLA review.

## 14. Sign-off (HIGH_RISK 11 canonical)

11 roles per spec contract §6 + framework §33.5.4.3 + ADR-0034 solo-tier waiver: Owner + Final Approver + Architect (com **Privacy + Legal SME specialization MANDATORY** para S-19 — DPA click-through 6-field consent + JWT receipt verifiable + 3 locales Legal local review + DPA versioning + re-acceptance flow + atomicity D1 transaction + DPA-first invariant) + Privacy Officer + Legal Counsel + Engineer (S-19 lead) + QA Lead + Product + SRE Lead + Compliance Officer + Sales lead + Privacy/UX advisor.

Privacy SME + Legal SME folds into Architect role specialization (precedent: S-13 Crypto SME + S-14 Crypto SME). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect per framework §33.5.4.3 + ADR-0034). Sales lead canonical em S-19 (enterprise handoff orchestrated business logic).

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-19 (cycle 12.S19.0; spec contract v1.1.0 base; lane upgrade STANDARD → HIGH_RISK ratificado FF-HR-009; two-phase SEAL D+15/D+45 canonical; INV-ONBOARD-DPA-FIRST + INV-ONBOARD-ATOMIC-PROVISIONING novas §3.12). |

---

**Fim de S-19 sprint contract.**
