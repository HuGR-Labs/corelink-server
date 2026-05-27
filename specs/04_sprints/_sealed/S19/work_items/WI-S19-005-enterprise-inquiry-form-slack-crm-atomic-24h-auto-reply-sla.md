---
id: "WI-S19-005"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-009"]
parent: "S-19"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
  - "SECURITY-MODEL"
tags: ["wi", "s19", "onboarding", "enterprise-inquiry", "slack-crm-atomic", "saga", "24h-auto-reply", "sales-handoff", "high-risk"]
---

# WI-S19-005 — Enterprise Inquiry Form Backend (`/api/onboarding/enterprise-inquiry`) com Form Fields (company + role + expected GB/mo + BYOK Requirements + Residency Requirements + reCAPTCHA + Email + Phone Optional) + Submit Triggers Slack Notification (`#sales-leads` Channel via Webhook) + CRM Entry (HubSpot or Equivalent via API) **Atomic** (Both Succeed or Both Rollback via Saga Pattern PAT-SAGA-001 Reuse S-10 + Retry Queue + Compensating Transaction) + Auto-Reply Email com White-Glove Timeline (24h Response SLA Hard) ≤ 5 min via SES + Sales Engagement Workflow Documented em `docs/internal/sales-handoff.md` (Lead Triage Decision Tree + Qualification Checklist + Follow-Up Cadence) + Rate Limit + reCAPTCHA + Bot Detection Mitigates Spam Abuse + Métricas Prometheus snake_case (enterprise_inquiry_total{outcome} + auto_reply_duration_seconds_bucket p99 ≤ 5min + slack_crm_atomicity_violations_total alert > 0) + RB-FM-ENTERPRISE-HANDOFF-PARTIAL Stub

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-19](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S19-005 |
| Título | Enterprise inquiry form + Slack + CRM atomic saga + 24h auto-reply SLA + Sales workflow doc + reCAPTCHA |
| Sprint | S-19 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-009 (enterprise inquiry é first-touch sales contract; partial failure = lost lead = customer trust loss) |

## 1. Intent

Enterprise inquiry form é o **white-glove entry point** for enterprise customer prospects (FedRAMP-ready, EU customers, financial services, BYOK requirements, residency requirements). Spec contract S-19 §5.4 R-S19-10 estabelece: form fields (company + role + expected GB/mo + BYOK + residency); submit → Slack notification (`#sales-leads`) + CRM entry (HubSpot or equivalent) **atomic**; auto-reply email com 24h response SLA; Sales engagement workflow documented em `docs/internal/sales-handoff.md`. Este WI implementa todos + reCAPTCHA + bot detection + RB-FM-ENTERPRISE-HANDOFF-PARTIAL stub.

```rust
// File: crates/corelink-onboarding-enterprise/src/inquiry_handler.rs

#![forbid(unsafe_code)]

#[async_trait]
pub trait EnterpriseInquiryHandler: Send + Sync {
    /// POST /api/onboarding/enterprise-inquiry — form submit handler.
    /// Saga: Slack notification + CRM entry atomic; both succeed or both rollback.
    /// Auto-reply email ≤ 5 min via SES.
    /// PAT-SAGA-001 reuse S-10.
    async fn submit_inquiry(
        &self,
        form: EnterpriseInquiryForm,
        recaptcha_token: RecaptchaToken,
    ) -> Result<InquiryReceipt, EnterpriseInquiryError>;
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct EnterpriseInquiryForm {
    pub company: String,                                 // 1-200 chars
    pub role: String,                                    // CISO|CTO|CIO|VPEng|other
    pub email: String,                                   // validated RFC 5322
    pub phone_optional: Option<String>,                  // E.164 format
    pub expected_gb_per_month: u64,                      // 0..=u64::MAX
    pub byok_requirements: BYOKRequirementsKind,         // none|aws_kms|gcp_kms|azure_kv|vault
    pub residency_requirements: ResidencyKind,           // none|us|eu|sam|apac|specific
    pub additional_notes: Option<String>,                // 0-2000 chars
}
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Enterprise inquiry form é o **first-touch sales contract** for high-value enterprise prospects. Sem atomicity Slack + CRM, partial failure = lost lead (Slack notification arrived but CRM entry missing → Sales rep reaches out but no CRM context → unprofessional impression → customer trust loss; OR CRM entry created but Slack silent → Sales rep doesn't notice for hours → 24h SLA missed → lost deal).

Spec contract S-19 §5.4 R-S19-10 estabelece atomicity: Slack + CRM both succeed or both rollback. Este WI implementa via saga pattern PAT-SAGA-001 (reuse S-10):
1. Slack webhook POST first (idempotent via `idempotency_key`).
2. CRM API POST (HubSpot or equivalent).
3. If CRM fails → reverse compensation via Slack webhook delete (or mark "rollback" message).
4. If both succeed → emit audit `corelink.onboarding.enterprise_inquiry_atomic_ok`.
5. Auto-reply email via SES ≤ 5 min.

**Risk justification HIGH_RISK**:

- **FF-HR-009**: enterprise inquiry é first-touch customer-facing contract (white-glove SLA promise); partial failure = lost lead = customer trust loss = revenue loss permanent.
- **Reversibility**: bug detected post-deploy = catastrophic (lost deals attributable to bug; competitive disadvantage; reputation damage).
- **Blast radius**: 100% enterprise prospects; potentially 10s-100s/mês at GA scale; each lost lead = $50k-500k ARR loss.

**Bugs catastróficos que este WI deve catch**:

- **Slack webhook fail silently**: webhook returns 200 OK but message not posted (Slack rate limit ou channel permission); CRM proceeds; Sales doesn't see Slack; 24h SLA missed.
- **CRM API timeout**: HubSpot API timeout 30s; Slack already posted; saga compensation not triggered; Sales sees Slack mas CRM context missing.
- **Auto-reply email fail**: SES bounce; customer doesn't see confirmation; doubt about submission; resubmits; 2 inquiries em CRM.
- **reCAPTCHA bypass**: pentester bypasses reCAPTCHA via headless browser; spam abuse.
- **Idempotency bug**: same form submit retried 3x; 3 inquiries em CRM + 3 Slack messages + 3 auto-reply emails; embarrassing.
- **Race condition saga**: Slack succeeds at t1; CRM POST in-flight at t1+1s; Slack compensation triggered at t1+2s (CRM failure detected); CRM eventually succeeds at t1+3s; resulting state: CRM entry exists + Slack rollback message posted; confusing.

**Atacante adversarial scenarios validated**:

- **Spam abuse**: pentester submits 1000 forms in 1 min; verify rate limit + reCAPTCHA + bot detection.
- **Slack webhook forge**: pentester captures webhook URL + posts forged messages; verify Slack signature secret rotation.
- **CRM API key exfiltration**: pentester tries to leak HubSpot API key; verify key never em logs + rotation cadence.
- **Auto-reply phishing exploit**: pentester crafts email payload to phish customer; verify email template static + SES sandbox.
- **Saga state corruption**: pentester injects Slack webhook timeout to leave saga in partial state; verify reverse compensation triggered + RB-FM-ENTERPRISE-HANDOFF-PARTIAL invoked.

**Mitigation**: saga pattern PAT-SAGA-001 reuse S-10; idempotency key per form submit; reCAPTCHA + rate limit + bot detection; reverse compensation Slack delete message; CRM API timeout 30s + retry queue; auto-reply email static template + SES sandbox; RB-FM-ENTERPRISE-HANDOFF-PARTIAL stub for ops escalation.

## 3. Customer Impact & Journey

**Persona 1 — Enterprise prospect (CISO / CTO / VPEng)**:
- Navigate `/contact-sales` ou tier picker enterprise = "Contact us".
- Fill form: company + role + email + phone (optional) + expected GB/mo + BYOK requirements + residency requirements + additional notes.
- Submit → instant confirmation modal "Thank you. Our Sales team will respond within 24 hours."
- Auto-reply email ≤ 5 min com white-glove timeline + Sales contact info.
- Sales engagement ≤ 24h SLA (manual follow-up by Sales rep with CRM context).

**Persona 2 — Internal Sales lead**:
- Slack `#sales-leads` channel notification: "New enterprise inquiry from <company> (<role>); BYOK: <kind>; residency: <kind>; expected GB: <num>."
- CRM entry created with full form context + lead score + qualification checklist.
- Sales workflow doc `docs/internal/sales-handoff.md`: lead triage decision tree (qualified vs unqualified) + qualification checklist + follow-up cadence (24h response SLA).

**Persona 3 — Internal SRE on-call**:
- RB-FM-ENTERPRISE-HANDOFF-PARTIAL stub em `specs/05_runbooks/`; dry-run quarterly.
- Métrica `corelink_onboarding_slack_crm_atomicity_violations_total` alert > 0 = SEV-2.

**SLA addendum**:
- Slack notification ≤ 1 min after form submit.
- CRM entry ≤ 1 min after form submit.
- Saga atomicity: both succeed or both rollback.
- Auto-reply email ≤ 5 min via SES.
- Sales engagement ≤ 24h SLA.
- Spam blocked via reCAPTCHA + rate limit (≤ 3/hora per IP) + bot detection.

## 4. Capability Mapping

- **CAP-ONBOARD-005** (Enterprise inquiry form + white-glove handoff) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §5.4 R-S19-10` + `resilience_patterns.md PAT-SAGA-001` + `S-10 saga pattern reuse`.

## 5. Tipo

Feature WI; HIGH_RISK; FF-HR-009; sales-load-bearing.

## 6. Escopo

### 6.1 In-scope

1. **Enterprise inquiry form backend** em `crates/corelink-onboarding-enterprise/`:
   - `POST /api/onboarding/enterprise-inquiry` handler.
   - Form fields: company (1-200) + role (enum CISO|CTO|CIO|VPEng|other) + email (validated RFC 5322) + phone (optional E.164) + expected_gb_per_month (u64) + byok_requirements (enum none|aws_kms|gcp_kms|azure_kv|vault) + residency_requirements (enum none|us|eu|sam|apac|specific) + additional_notes (0-2000).
   - Schema validated via zod (frontend) + serde (backend); rejects malformed.

2. **Slack notification** (`#sales-leads` channel via webhook):
   - Slack webhook URL stored em DO config-singleton (S-13 secret rotation 24h cadence).
   - Message format: "New enterprise inquiry from <company> (<role>); BYOK: <kind>; residency: <kind>; expected GB: <num>; <CRM link>; <inquiry_id>."
   - Idempotency via `idempotency_key` per inquiry_id.
   - Slack signature verify on webhook outbound (HMAC-SHA256 with shared secret).

3. **CRM entry** (HubSpot or equivalent via API):
   - HubSpot API (or equivalent) POST `/crm/v3/objects/contacts` + `/crm/v3/objects/deals`.
   - API key stored em DO config-singleton (S-13 secret rotation 30d cadence).
   - Lead score auto-computed: BYOK enterprise + residency EU + expected GB > 1TB = high score.
   - Qualification checklist auto-populated: company size + industry + decision-maker role.

4. **Saga pattern PAT-SAGA-001 atomicity** (reuse S-10):
   - Step 1: Slack webhook POST (idempotent via key).
   - Step 2: CRM API POST.
   - If CRM fails → reverse compensation: Slack webhook delete message (or mark "rollback" message com prefix `[ROLLBACK]`).
   - If both succeed → emit audit `corelink.onboarding.enterprise_inquiry_atomic_ok`.
   - If saga in partial state > 5 min → invoke RB-FM-ENTERPRISE-HANDOFF-PARTIAL.

5. **Auto-reply email** ≤ 5 min via SES:
   - Static template: "Thank you for your interest in CoreLink. Our Sales team will respond within 24 hours. Inquiry ID: <id>. White-glove timeline below: ... Contact: sales@humangr.com."
   - SES sandbox + DKIM/SPF/DMARC configured.
   - Bounce handling: alert > 5% bounce rate.

6. **Sales engagement workflow** documented em `docs/internal/sales-handoff.md`:
   - Lead triage decision tree (qualified vs unqualified).
   - Qualification checklist (company size + industry + decision-maker + budget + timeline).
   - Follow-up cadence (24h response SLA + 48h deeper qualification + 1 week proposal).
   - Escalation path (failed handoff → SRE on-call + Sales lead).

7. **reCAPTCHA + rate limit + bot detection**:
   - reCAPTCHA v3 (invisible) on form submit; threshold ≥ 0.5.
   - Rate limit ≤ 3/hora per IP (via Cloudflare WAF + S-08 reuse).
   - Bot detection via User-Agent + form interaction pattern + honeypot field.
   - Spam blocked → 422 + audit emit `corelink.onboarding.enterprise_inquiry_spam_blocked`.

8. **RB-FM-ENTERPRISE-HANDOFF-PARTIAL stub** em `specs/05_runbooks/`:
   - Scenario: Slack succeeded but CRM failed (or vice versa); saga in partial state > 5 min.
   - Decision tree: manual reconcile (post Slack mark + create CRM entry manual) + customer notify + Sales escalation.
   - Stub-only em S-19; full runbook deferred S-20.

9. **Métricas Prometheus snake_case underscored**:
   - `corelink_onboarding_enterprise_inquiry_total{outcome, plan}` (outcome ∈ slack_crm_atomic_ok|slack_crm_partial_rolled_back|spam_blocked|recaptcha_failed).
   - `corelink_onboarding_enterprise_auto_reply_duration_seconds_bucket{plan}` (histogram p99 ≤ 5 min).
   - `corelink_onboarding_slack_crm_atomicity_violations_total{plan}` (counter; alert > 0 = SEV-2).
   - `corelink_onboarding_enterprise_recaptcha_failed_total{plan}` (counter).
   - `corelink_onboarding_enterprise_rate_limit_blocked_total{plan}` (counter).

### 6.2 Out-of-scope (deferred)

- Signup orchestration backend (WI-S19-001).
- DPA click-through (WI-S19-002).
- Tier selection + Stripe Checkout (WI-S19-004; enterprise tier routes here).
- Conversion funnel (WI-S19-006).
- Multi-DPO escalation workflow (single DPO em S-19).
- CRM webhook bidirectional sync (HubSpot → CoreLink updates; deferred pós-GA).
- Lead nurturing automation (Marketing Email; deferred pós-GA).
- Salesforce as alternative CRM (HubSpot canonical at GA; multi-CRM pós-GA).

## 7. Anti-Scope

- Saga without atomic compensation (partial state tolerated).
- Slack webhook signature verify skipped.
- CRM API key em logs.
- Auto-reply email > 5 min.
- reCAPTCHA bypass.
- Spam abuse tolerated (rate limit required).
- Sales handoff doc skipped (operational readiness gap).
- RB-FM-ENTERPRISE-HANDOFF-PARTIAL stub skipped.

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Enterprise inquiry form Slack+CRM atomic + 24h auto-reply

  Scenario: Form submit triggers Slack + CRM atomic
    Given user submits form with company="Acme" + role="CISO" + email + BYOK="aws_kms" + residency="eu"
    And reCAPTCHA token valid
    When backend processes
    Then Slack notification posted to #sales-leads
    And CRM entry created in HubSpot
    And both atomic (saga pattern PAT-SAGA-001)
    And audit emit "corelink.onboarding.enterprise_inquiry_atomic_ok"

  Scenario: Auto-reply email ≤ 5 min via SES
    Given inquiry submitted successful
    When SES sends auto-reply
    Then email delivered ≤ 5 min p99
    And template static (no PII leak)

  Scenario: CRM fails → Slack rollback compensation
    Given Slack webhook posted successful
    When CRM API POST fails (timeout 30s)
    Then saga reverse compensation triggered
    And Slack message marked [ROLLBACK] (or deleted)
    And audit emit "corelink.onboarding.enterprise_inquiry_atomic_rollback"

  Scenario: Slack fails → CRM not posted
    Given Slack webhook fails (rate limit 429)
    When backend processes
    Then CRM POST not initiated
    And inquiry queued for retry
    And audit emit "corelink.onboarding.enterprise_inquiry_slack_failed_retry"

  Scenario: Saga partial state > 5 min → RB invoked
    Given Slack succeeded + CRM in-flight > 5 min
    When monitor detects
    Then RB-FM-ENTERPRISE-HANDOFF-PARTIAL invoked
    And SEV-2 alert fired
    And on-call paged

  Scenario: Spam abuse blocked via reCAPTCHA
    Given reCAPTCHA token < 0.5 score
    When backend validates
    Then 422 returned + audit emit "corelink.onboarding.enterprise_inquiry_spam_blocked"

  Scenario: Rate limit blocked > 3/hora per IP
    Given 4 inquiries submitted from same IP in 1 hour
    When 4th submitted
    Then 429 returned + audit emit "corelink.onboarding.enterprise_inquiry_rate_limit_blocked"

  Scenario: Form schema validation rejects malformed
    Given form with company="" (empty)
    When backend validates zod
    Then 422 returned + error "company_required"

  Scenario: Idempotency same form submit retried 3x
    Given idempotency_key per inquiry
    When submitted 3x
    Then 1 Slack message + 1 CRM entry + 1 auto-reply
    And dedup via idempotency_key

  Scenario: Sales workflow doc committed
    Given docs/internal/sales-handoff.md
    When committed em PR
    Then lead triage decision tree
    And qualification checklist
    And follow-up cadence 24h SLA documented

  Scenario: Slack webhook signature verify outbound
    Given Slack webhook URL leaked
    When pentester forges message
    Then HMAC-SHA256 signature verify rejects

  Scenario: CRM API key never em logs
    Given safeLog allowlist enforced
    When inquiry processed
    Then CRM API key NEVER em logs (verified via grep audit)
```

## 9. Design Decisions

### 9.1 Why saga pattern PAT-SAGA-001 (não 2PC)

- 2PC requires distributed transaction coordinator; Slack + CRM not coordinator-aware.
- Saga = sequence of compensable transactions; reverse compensation on failure.
- PAT-SAGA-001 reuse S-10 (Stripe webhook saga pattern canonical).

### 9.2 Why HubSpot canonical (não Salesforce)

- HubSpot tier "Free" sufficient at GA; Salesforce baseline ~$25/user/mês.
- HubSpot API documentation excellent; rate limits generous.
- Multi-CRM support deferred pós-GA.

### 9.3 Why auto-reply ≤ 5 min (não instant)

- Instant = SES rate limit risk; bounce handling weak.
- 5 min = balance between UX expectation + SES reliability.
- Customer expectation set in confirmation modal "within 24 hours".

### 9.4 Why reCAPTCHA v3 (não v2 checkbox)

- v3 invisible; better UX (no checkbox interruption).
- Threshold ≥ 0.5 conservative; tunable via DO config.

### 9.5 Why Slack webhook signature verify outbound

- Slack webhook URL leaked = pentester can post forged messages to #sales-leads.
- HMAC-SHA256 signature verify outbound prevents impersonation.
- S-13 secret rotation 24h cadence for Slack webhook secret.

### 9.6 Why RB-FM-ENTERPRISE-HANDOFF-PARTIAL stub (não full runbook)

- S-19 timeline 2.5 weeks; full runbook deferred S-20.
- Stub covers expected scenario + decision tree skeleton.

### 9.7 ADR potencial?

- Não. Patterns reused (saga PAT-SAGA-001 S-10 + reCAPTCHA v3 + HubSpot CRM standard). No novel architecture decision.

## 10. Completeness Criteria SOTA

- [ ] **10.s19.005.1** Form schema zod + serde validated.
- [ ] **10.s19.005.2** Slack webhook posted to #sales-leads.
- [ ] **10.s19.005.3** CRM entry HubSpot API.
- [ ] **10.s19.005.4** Saga atomicity Slack + CRM (both succeed or both rollback).
- [ ] **10.s19.005.5** Auto-reply email ≤ 5 min via SES.
- [ ] **10.s19.005.6** Sales workflow doc `docs/internal/sales-handoff.md` committed.
- [ ] **10.s19.005.7** reCAPTCHA v3 + rate limit + bot detection.
- [ ] **10.s19.005.8** Idempotency key per inquiry.
- [ ] **10.s19.005.9** RB-FM-ENTERPRISE-HANDOFF-PARTIAL stub committed.
- [ ] **10.s19.005.10** Slack webhook signature verify outbound.
- [ ] **10.s19.005.11** CRM API key never em logs (safeLog allowlist).
- [ ] **10.s19.005.12** Métricas Prometheus snake_case (5+ métricas).

## 11. DoD

- [ ] Form submit tested staging.
- [ ] Saga atomicity verified.
- [ ] Auto-reply email ≤ 5 min verified.
- [ ] Sales handoff doc committed.
- [ ] reCAPTCHA + rate limit verified.
- [ ] Tests: unit (form validation + saga + auto-reply) + integration (E2E inquiry → Slack + CRM + email) + 4+ negative scenarios.

## 12. Invariants Validated

- **CTRL-AUDIT-002** (rich events) reforced via inquiry audit emit.
- **PAT-SAGA-001** (saga pattern) reused.
- Não introduz INV nova (consumer + reuso S-10 saga).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Enterprise inquiry crate | `crates/corelink-onboarding-enterprise/` | Rust |
| Inquiry handler | `crates/corelink-onboarding-enterprise/src/inquiry_handler.rs` | Rust |
| Saga logic | `crates/corelink-onboarding-enterprise/src/saga.rs` | Rust |
| Slack webhook client | `crates/corelink-onboarding-enterprise/src/slack_client.rs` | Rust |
| CRM API client | `crates/corelink-onboarding-enterprise/src/crm_client.rs` | Rust |
| Auto-reply SES | `crates/corelink-onboarding-enterprise/src/auto_reply.rs` | Rust |
| Form UI | `apps/web/src/app/contact-sales/page.tsx` | TypeScript |
| Sales handoff doc | `docs/internal/sales-handoff.md` | Markdown |
| RB stub | `specs/05_runbooks/RB-FM-ENTERPRISE-HANDOFF-PARTIAL.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s19.005.1** Test coverage ≥ 90% (handler + saga + auto-reply).
- **14.s19.005.2** SAST: cargo-audit + cargo-deny clean.
- **14.s19.005.3** SES email deliverability ≥ 95%; bounce rate < 5%.
- **14.s19.005.4** Slack webhook signature verify outbound HMAC-SHA256.
- **14.s19.005.5** CRM API key rotation 30d cadence (S-13).
- **14.s19.005.6** Saga atomicity 100% (zero partial state em chaos drill).
- **14.s19.005.7** Auto-reply email ≤ 5 min p99.

## 15. Chaos Experiments

1. **CRM API timeout 30s**: verify saga reverse compensation.
2. **Slack webhook fail silently**: verify retry queue + monitor.
3. **Auto-reply SES bounce**: verify alert > 5% bounce rate.
4. **reCAPTCHA bypass attempt**: verify threshold ≥ 0.5 enforced.
5. **Rate limit exhaustion**: 4 inquiries from same IP; verify 429.
6. **Idempotency replay**: same form submit 3x; verify 1 Slack + 1 CRM + 1 email.
7. **Slack webhook signature forge**: pentester forges message; verify rejected.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (sprint S-19 §14):

- [ ] All Gherkin green.
- [ ] Saga atomicity verified.
- [ ] Sales handoff doc committed.
- [ ] reCAPTCHA + rate limit verified.
- [ ] 11 sign-offs canonical documented.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Form backend + zod + serde validation | 2h |
| ST-002 | Slack webhook client + signature verify outbound | 2h |
| ST-003 | CRM API client (HubSpot or equivalent) | 3h |
| ST-004 | Saga pattern PAT-SAGA-001 reuse + reverse compensation | 3h |
| ST-005 | Auto-reply email SES + DKIM/SPF/DMARC | 2h |
| ST-006 | reCAPTCHA v3 + rate limit + bot detection | 2h |
| ST-007 | Sales handoff doc | 1h |
| ST-008 | RB stub | 1h |

**Total Optimistic**: ~16h. **PERT** (O=10h, M=16h, P=26h per spec contract §12): **16.7h**.

## 18. Dependencies

### Hard blockers
- WI-S19-001 SEALED (signup orchestration foundation).
- S-13 SEALED (admin plane secret rotation Slack + CRM API key).
- S-08 SEALED (rate limit foundation).

### Soft blockers
- S-10 SEALED (saga pattern PAT-SAGA-001 canonical reuse).
- S-09 SEALED (audit chain R2 EVT emit).

### Outbound
- WI-S19-006 (closing PRR).
- S-20 (GA exige 1 enterprise inquiry tested).

## 19. Effort PERT

O: 10h, M: 16h, P: 26h → PERT **16.7h** (per spec contract §12; form + saga + auto-reply + handoff doc).

## 20. Time-boxing

**20h hard limit owner**. Saga compensation **3h dedicated**. Se exceder: split em sub-WI (form + Slack vs saga + auto-reply).

## 21. Observability

Métricas Prometheus snake_case underscored:

- `corelink_onboarding_enterprise_inquiry_total{outcome, plan}` (outcome ∈ slack_crm_atomic_ok|slack_crm_partial_rolled_back|spam_blocked|recaptcha_failed|rate_limit_blocked).
- `corelink_onboarding_enterprise_auto_reply_duration_seconds_bucket{plan}` (histogram p99 ≤ 5 min).
- `corelink_onboarding_slack_crm_atomicity_violations_total{plan}` (alert > 0 = SEV-2).
- `corelink_onboarding_enterprise_recaptcha_failed_total{plan}` (counter).
- `corelink_onboarding_enterprise_rate_limit_blocked_total{plan}` (counter).
- `corelink_onboarding_slack_webhook_invalid_signature_total{plan}` (counter; alert > 0 indicates impersonation).

Dashboard DASH-ONBOARDING painel "Enterprise Inquiry Handoff" (5-panel: inquiry rate + saga atomicity + auto-reply latency + reCAPTCHA failures + rate limit blocked).

## 22. Cost Analysis

- Slack webhook: free (Slack workspace).
- HubSpot Free tier: free at low volume; ~$50/user/mês paid tier as scaling.
- SES emails: ~$0.10 / 1000 emails = $1/mês incremental.
- Total: ~$1-50/mês incremental depending on HubSpot tier.

## 23. API Contract

- `POST /api/onboarding/enterprise-inquiry` — form submit; returns 200 (success + inquiry_id) | 422 (recaptcha_failed | malformed) | 429 (rate_limit) | 503 (saga_partial_retry).

## 24. Post-mortem Hooks

- Saga atomicity violation detected → SEV-2 + reverse compensation review.
- Auto-reply email > 5 min sustained → SEV-3 + SES configuration review.
- Spam abuse detected (reCAPTCHA bypassed) → SEV-2 + bot detection strengthen.
- Slack webhook signature forge detected → CRITICAL + Security incident.
- CRM API key leaked em logs → CRITICAL + rotation immediate.
- Sales handoff workflow drift > 30 days → SEV-3 + doc review.

## 25. Rollback / Recovery

Enterprise inquiry regression → revert via CF Workers rollback; existing inquiries retained em CRM + Slack history; Sales escalation manual fallback.

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: Slack webhook signature verify outbound HMAC-SHA256; reCAPTCHA + bot detection.
- **Tampering**: form schema validated zod + serde; saga atomicity prevents partial state.
- **Repudiation**: inquiry audit emit + CRM entry + Slack message; forensic trail.
- **Information disclosure**: CRM API key + Slack webhook secret never em logs; safeLog allowlist.
- **DoS**: rate limit ≤ 3/hora per IP; reCAPTCHA + bot detection.
- **Elevation of privilege**: anonymous submission acceptable (no auth required); no privilege escalation possible.

**LINDDUN delta**:
- **Linkability**: company + email em CRM (intentional sales engagement; LGPD Art. 7º legitimate interest).
- **Identifiability**: form fields stored em CRM (intentional).
- **Non-repudiation**: inquiry audit trail.
- **Detectability**: spam blocked alerted; reCAPTCHA failed alerted; rate limit alerted.
- **Disclosure**: form template static; no PII leak em auto-reply.
- **Unawareness**: customer notified per submission via auto-reply ≤ 5 min.
- **Non-compliance**: LGPD Art. 7º + GDPR Art. 6(1)(f) legitimate interest sales satisfied.

## 27. Knowledge Transfer

- Tech talk (1h): "CoreLink Enterprise Inquiry — Saga Slack+CRM + 24h SLA".
- Doc `docs/internal/sales-handoff.md` (this WI deliverable).
- Onboarding test (3 questions): saga atomicity + reCAPTCHA threshold + handoff workflow.

## 28. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Status |
|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ |
| 3 | Architect (Privacy + Legal SME) | _TBD_ | _pending_ |
| 4 | Privacy Officer | _TBD_ | _pending_ |
| 5 | Legal Counsel | _TBD_ | _pending_ |
| 6 | Engineer (S-19 lead) | _TBD_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ |
| 9 | SRE Lead | _TBD_ | _pending_ |
| 10 | Compliance Officer | _TBD_ | _pending_ |
| 11 | Sales lead | _TBD_ | _pending_ |

## 29. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S19-005 (cycle 12.S19.0; enterprise inquiry form + saga Slack+CRM atomic + 24h SLA + sales handoff doc + reCAPTCHA). |

## 30. Anti-patterns evitados

- Saga without atomic compensation.
- Slack webhook signature verify skipped.
- CRM API key em logs.
- Auto-reply > 5 min.
- reCAPTCHA bypass.
- Spam abuse tolerated.
- Sales handoff doc skipped.

---

**Fim WI-S19-005.**
