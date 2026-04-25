---
id: "SPEC-CONTRACT-S19"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s19", "onboarding", "dpa", "self-service", "enterprise-handoff", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-19: Customer Onboarding (Self-Service Signup → DPA → Billing → Enterprise Handoff)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-19 |
| Nome | Customer Onboarding |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-009 (DPA + Terms customer-facing contract) — onboarding flow registra DPA click-through e ativa billing; bug = legal exposure |
| Duração estimada | 2.5 semanas |
| WIs antecipados | 6 |
| SOTA target | Self-service signup ≤ 3 min + DPA click-through evidence-grade + Stripe Checkout 99.9% reliability + enterprise handoff orchestrated |

## 1. Objetivo

**Operacionalizar fluxo de customer onboarding end-to-end** com qualidade enterprise-ready: self-service signup → email verify → tenant provisioning → DPA click-through (com evidence-grade EVT-049) → tier selection → Stripe Checkout subscription activation → first PAT created → quickstart link. Enterprise inquiry form com white-glove handoff (Slack notification + CRM entry + Sales engagement). Reduz friction de conversão; sem isso GA scaling impossible.

**Por que SOTA:** competitors entregam onboarding com DPA click-through advisory ou DPA acceptance via support email manual. CoreLink S-19 entrega: (a) DPA click-through com 6-field consent capture (CTRL-PRIV-CONSENT-001..006); (b) cryptographic proof of DPA acceptance (signed JWT receipt); (c) signup ≤ 3 min start-to-finish; (d) abandon rate per-step instrumented; (e) enterprise handoff Slack→CRM atomic. Reference: **Stripe Atlas onboarding** (gold standard), **Auth0 self-service signup**, **Linear onboarding flow**.

**Codex finding lane upgrade:** STANDARD → HIGH_RISK porque onboarding registra DPA + ativa billing = **FF-HR-009** (customer-facing contract); bug = legal exposure inaceitável.

**Codex finding ownership clash:** S-16 owns frontend UI (`CAP-UI-001` "Tenant onboarding flow") — S-19 owns **business logic** of onboarding (DPA acceptance + tier provisioning + enterprise handoff); UI surface delivered S-16, business orchestration delivered S-19. Separation explicit em §10 anti-scope.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK (10–12 sign-offs).
- **FF-HR-009**: DPA + Terms click-through é customer-facing contract; bug → legal exposure (LGPD + GDPR + CCPA enforcement risk).
- **Forcing factor**: billing activation depende de DPA signed; race condition = customer billed sem DPA = breach.

## 3. Inherits_from

```yaml
inherits_from:
  - "COMPLIANCE-MATRIX"         # SOC 2 + LGPD + GDPR + CCPA
  - "PRIVACY-MODEL"             # CTRL-PRIV-CONSENT-001..006 (DPA é consent type)
  - "AUTH-MODEL"                # Clerk email verify + MFA setup
  - "OBSERVABILITY-MODEL"       # conversion funnel + abandon rate métricas
  - "FAILURE-MODES"             # FM-160 (auth invalid), FM-151 (Stripe outage)
  - "INVARIANT-REGISTRY"        # INV-CONSENT-PROOF-VERIFIABLE, INV-DATA-RESIDENCY
```

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-ONBOARD-001** | Self-service signup business logic | Email verify orchestration; tenant provisioning atomic; first PAT generation. |
| **CAP-ONBOARD-002** | DPA click-through | Version-aware DPA acceptance com 6-field consent capture (CTRL-PRIV-CONSENT-001..006); EVT-049 evidence; signed JWT receipt. |
| **CAP-ONBOARD-003** | Tier selection + Stripe subscription | Tier picker + Stripe Checkout integration (S-10 reuse); subscription activation depends on DPA signed. |
| **CAP-ONBOARD-004** | First-run experience | First PAT + CLI install command + quickstart link; in-app guided tour (S-16 UI consume). |
| **CAP-ONBOARD-005** | Enterprise inquiry form + white-glove handoff | Form em UI; backend triggers Slack notification + CRM entry (HubSpot/equivalent); Sales engagement orchestrated. |
| **CAP-ONBOARD-006** | Conversion funnel instrumentation | Per-step métricas (start, email verify, DPA, tier, Stripe, first PAT); abandon rate per-step + cohort analysis. |
| **CAP-ONBOARD-007** | DPA versioning + re-acceptance flow | Major DPA version bump → existing tenants prompted re-accept; grace period 30d; failure → degrade read-only. |

## 5. Requirements específicos

### 5.1 Signup Business Logic (CAP-ONBOARD-001)

- **R-S19-1**: Signup orchestration (frontend em S-16; backend em S-19):
  - User submits email → Clerk email verify.
  - Email verified → tenant provisioning atomic em D1 (`tenant`, `tenant_metadata`, `usage_counter` initialized).
  - Region pinned per Accept-Language ou explicit selection (US default; EU detected via locale).
  - First PAT created automaticamente com scope read-write + 90d expiry.
- **R-S19-2**: Signup atomicity: tenant provisioning + DPA acceptance + Stripe customer ID em single transaction; failure rollback all.
- **R-S19-3**: Signup ≤ 3 min start-to-finish (measured via dev workshop; 5-dev sample).

### 5.2 DPA Click-Through (CAP-ONBOARD-002 + CAP-ONBOARD-007)

- **R-S19-4**: DPA template v1 em `legal/dpa/v1.md` + click-through implementation:
  - Render DPA full text em UI (S-16).
  - User must scroll para bottom (anti-pattern dismiss without read).
  - Calcular `notice_text_hash = sha256(dpa_full_text)` em frontend; envia ao backend.
  - 6-field consent payload (CTRL-PRIV-CONSENT-001..006).
  - **EVT-049 evidence captured**: full payload + signed JWT receipt.
  - Backend signs receipt JWT com per-region key.
- **R-S19-5**: DPA versioning:
  - Semver (major bump = material change requires re-acceptance).
  - Major bump triggers existing tenants email broadcast + 30d grace period to re-accept.
  - Re-acceptance failure após grace period → tenant degrade read-only com email + in-app warning.
- **R-S19-6**: DPA available em 3 locales (en + pt-BR + es); Legal local review per locale.

### 5.3 Tier + Stripe (CAP-ONBOARD-003)

- **R-S19-7**: Tier selection UI (S-16 frontend) integrado Stripe Checkout (S-10 reuse):
  - 5 tiers visible (free/starter/team/pro/enterprise — enterprise = "Contact us").
  - Stripe Checkout redirect com tenant_id mapped to Stripe customer.
  - Webhook handler (S-10 reuse) atualiza local subscription state.
- **R-S19-8**: **Atomicity invariant**: subscription activation requires DPA signed first; race condition prevented via D1 lock + INV-ONBOARD-DPA-FIRST.

### 5.4 First-Run + Enterprise (CAP-ONBOARD-004 + CAP-ONBOARD-005)

- **R-S19-9**: First-run experience:
  - First PAT shown (only-once per S-16 R-S16-8).
  - CLI install command rendered (e.g., `curl -fsSL https://corelink.dev/cli | sh`).
  - Quickstart link to `docs.corelink.dev/quickstart` (S-18).
  - Optional in-app tour (S-16 frontend consume).
- **R-S19-10**: Enterprise inquiry form:
  - Form fields: company, role, expected GB/mo, BYOK requirements, residency requirements.
  - Submit → Slack notification (`#sales-leads` channel) + CRM entry (HubSpot or equivalent) — atomic.
  - Auto-reply email com white-glove timeline (24h response SLA).
  - Sales engagement workflow documented em `docs/internal/sales-handoff.md`.

### 5.5 Funnel Instrumentation (CAP-ONBOARD-006)

- **R-S19-11**: Conversion funnel métricas (S-09 observability stack):
  - `corelink.onboarding.step_started_total{step}`.
  - `corelink.onboarding.step_completed_total{step}`.
  - `corelink.onboarding.step_abandoned_total{step, reason}`.
  - Steps: signup_start → email_verified → dpa_signed → tier_selected → stripe_activated → first_pat_created → first_cas_put.
- **R-S19-12**: Cohort analysis dashboard (S-09 dashboards-as-code).

## 6. Definition of Done

- [ ] **WIs SEALED**: 6/6.
- [ ] **E2E signup flow** ≤ 3 min start-to-finish (5-dev sample) (EVT-018).
- [ ] **DPA click-through** gera evidence imutável (EVT-049) + signed JWT receipt + verifiable backend (EVT-049 + EVT-024).
- [ ] **Stripe subscription** activa em team tier após DPA signed (E2E test) (EVT-018).
- [ ] **Atomicity test**: simulate Stripe outage during signup → tenant rolls back consistently (chaos test) (EVT-023).
- [ ] **Enterprise inquiry flow** testado com 1 fake lead → Slack + CRM atomic → 24h auto-reply email (EVT-018).
- [ ] **Conversion funnel** dashboard live em Grafana (S-09); 7 steps instrumentado (EVT-021).
- [ ] **DPA versioning re-acceptance** test: bump v1 → v2 → tenant prompted; refused → degrade read-only após 30d grace (EVT-018).
- [ ] **DPA 3 locales** native speaker + Legal local reviewed (en + pt-BR + es) (EVT-044).
- [ ] **Signup form a11y** AA (axe-core 0 violations) (EVT-018).
- [ ] **Zero secrets em URL** (no PAT, no consent token) — fuzz test 1k random params (EVT-002).
- [ ] **PRR HIGH_RISK**: Privacy Officer + Legal + Engineer + QA + Product + SRE + Compliance officer + Sales lead + 2 peers + Privacy/UX advisor.
- [ ] **Runbook**: cria RB-FM-SIGNUP-FAILED stub se signup atomicity falha em prod.

## 7. Completeness Criteria (delta local)

- [ ] **10.s19.1** Conversion funnel métrica (signup start → first usage) instrumentada (EVT-021).
- [ ] **10.s19.2** Abandon rate per-step visível em dashboard (S-09 alignment) (EVT-021).
- [ ] **10.s19.3** **Signup ≤ 3 min** sustained 30d staging (5-dev sample weekly).
- [ ] **10.s19.4** **DPA evidence verifiable** post-facto (consent verify endpoint S-11 retorna match).
- [ ] **10.s19.5** **DPA re-acceptance flow** sustained 1 cycle (v1 → v2 mock).
- [ ] **10.s19.6** **Enterprise handoff atomicity** Slack + CRM (atomic; bug em either rolls back).
- [ ] **10.s19.7** **5 real signups em closed beta** sucesso (EVT-018).

## 8. Invariants

### Mantidas

- **CTRL-PRIV-CONSENT-001..006** (consent capture com proof) — DPA é consent type.
- **INV-CONSENT-PROOF-VERIFIABLE** (HIGH — herdada S-11): DPA hash verifiable.

### Novas (introduzidas por S-19 — adicionar a invariant_registry.md §3.12)

- **INV-ONBOARD-DPA-FIRST** (HIGH — novo): subscription activation **requires** DPA signed primeiro; race condition prevented; nenhum customer billed sem DPA. **Why:** billing sem DPA = legal exposure. **How to apply:** D1 lock + transactional check + property test 10k concurrent attempts.
- **INV-ONBOARD-ATOMIC-PROVISIONING** (HIGH — novo): tenant provisioning atomic — tenant + DPA + Stripe customer ID em single tx; failure rollback all. **Why:** partial state = orphan tenant + billing inconsistency. **How to apply:** D1 transaction + chaos test Stripe outage.

## 9. Quality Standards (delta local)

- **14.s19.1 Signup form a11y AA** (WCAG 2.2 AA via axe-core CI 0 violations).
- **14.s19.2 Zero secrets em URL** (nem PAT, nem consent token, nem session token); fuzz test verifica.
- **14.s19.3 Signup ≤ 3 min** measured via dev workshop weekly.
- **14.s19.4 DPA quality**: 3 locales native speaker + Legal local review.
- **14.s19.5 Atomicity discipline**: signup é all-or-nothing; chaos test Stripe outage verifica rollback.
- **14.s19.6 Conversion funnel cardinality budget**: max 7 steps × 3 regions × 5 tiers = 105 series; respeita INV-OBS-CARDINALITY-BUDGET.
- **14.s19.7 Enterprise handoff SLA**: Slack notification ≤ 1 min; auto-reply ≤ 5 min; Sales engagement ≤ 24h.
- **14.s19.8 DPA evidence retention**: 7y em R2 audit bucket (S-09 alignment).

## 10. Anti-scope

- ❌ Enterprise MSA negotiation automation (manual com Legal; S-19 entrega inquiry form + handoff).
- ❌ Trial period logic (tier `free` já serve; sem timed trial complications).
- ❌ Frontend UI (S-16 owns UI; S-19 owns business logic + orchestration).
- ❌ Customer success automation (CSM tier — pós-GA).
- ❌ Email marketing automation (Sales engagement manual via CRM).
- ❌ Multi-step manual approval flow (auto-approve all signups; abuse handled separately via S-08 rate limit + S-11 DSR).
- ❌ Custom DPA per customer (single DPA template at GA; enterprise custom é S-14 + manual Legal).
- ❌ Referral program / affiliate tracking (pós-GA Q1).

## 11. Dependencies

### Hard blockers

- **S-03 SEALED** (auth Clerk + email verify).
- **S-10 SEALED** (billing Stripe + subscription).
- **S-11 SEALED** (DPA é consent type; consent ledger reuse).
- **S-16 SEALED** (frontend UI surfaces).
- **S-18 SEALED** (docs quickstart link).

### Soft blockers

- **S-09 SEALED** (conversion funnel instrumented).
- **S-13 SEALED** (admin plane support para enterprise inquiry handoff config).
- **S-14 SEALED** (BYOK + DPA enterprise; lighthouse customer enterprise tier consome BYOK setup wizard + DPA amendment Schrems II TIA do S-14).

### Outbound

- **S-20** (GA exige 5 real signups em closed beta + 1 enterprise inquiry tested).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S19-001** | Signup orchestration backend (Clerk + tenant provisioning + atomic) + INV-ONBOARD-ATOMIC-PROVISIONING | Clerk webhook; tenant provisioning D1 tx; first PAT; rollback logic; chaos test | 14h | 22h | 36h | **23.0h** |
| **WI-S19-002** | DPA click-through + 6-field consent + JWT receipt + 3 locales + Legal review | DPA template v1 + 3 locales; click-through endpoint; consent ledger reuse; JWT receipt; Legal review | 14h | 22h | 36h | **23.0h** |
| **WI-S19-003** | Tier selection + Stripe Checkout + INV-ONBOARD-DPA-FIRST + atomicity | Stripe Checkout redirect; subscription activation; D1 lock DPA-first; property test 10k | 12h | 18h | 30h | **19.0h** |
| **WI-S19-004** | First-run experience + CLI install command + quickstart link + sample tenant | first PAT shown; CLI install rendering; quickstart link logic; 5-dev UX research | 8h | 12h | 20h | **12.7h** |
| **WI-S19-005** | Enterprise inquiry form + Slack + CRM atomic + 24h auto-reply | form backend; Slack webhook; CRM API (HubSpot or equiv); auto-reply; sales handoff doc | 10h | 16h | 26h | **16.7h** |
| **WI-S19-006** | DPA versioning + re-acceptance flow + degrade read-only + conversion funnel + PRR | semver bump logic; re-acceptance email broadcast; 30d grace; degrade read-only; funnel instrument | 12h | 18h | 30h | **19.0h** |

**Total PERT:** ~113h ≈ 14 dias work × 1 eng. Buffer 3 dias confere com 2.5 semanas.

## 13. Duração + Timeline

- **Duração:** 2.5 semanas (12 dias úteis) + buffer 3 dias.
- **Marcos:**
  - **D+4:** WI-001 SEALED (signup orchestration).
  - **D+7:** WI-002 SEALED (DPA + Legal review).
  - **D+9:** WI-003 SEALED (Stripe + DPA-first).
  - **D+11:** WI-004 + WI-005 SEALED (first-run + enterprise).
  - **D+13:** WI-006 SEALED (versioning + funnel + PRR).
  - **D+15:** Sprint review + sign-offs.

## 14. Critérios de promoção

- DoD complete + 5 real signups em closed beta successful.
- DPA Legal sign-off em 3 locales.
- Atomicity property test 10k 0 violations.
- Conversion funnel sustained 30d.
- PRR HIGH_RISK aprovado.

## 15. Riscos (registry expandido)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **DPA legal review atrasa** (Legal externo) | M | M | MEDIUM | M | LOW | Engage Legal externo Q3 antes do sprint; iterate during sprint. |
| **Stripe Checkout quirks** em certas regiões | M | L | LOW | L | LOW | Test EU + LATAM regions; documented; fallback manual checkout para regions edge cases. |
| **Email deliverability** (Clerk) | L | L | LOW | L | LOW | Clerk SLA + monitor + fallback resend; SPF/DKIM/DMARC configured. |
| **Atomicity bug** signup (partial state) | L | M | HIGH | L | LOW | INV-ONBOARD-ATOMIC-PROVISIONING + chaos test Stripe outage + property test. |
| **DPA-first race condition** | M | M | HIGH (legal) | M | LOW | INV-ONBOARD-DPA-FIRST + D1 lock + property test 10k concurrent + audit. |
| **DPA re-acceptance flow** breaks existing tenants | L | M | HIGH | L | LOW | 30d grace + email broadcast + degrade read-only graceful + customer support escalation path. |
| **Enterprise handoff** Slack + CRM partial failure | M | L | MEDIUM | L | LOW | Atomic flag + retry queue + monitoring; manual fallback documented. |
| **Conversion funnel** false signal (bot signups inflate) | M | L | LOW | L | LOW | reCAPTCHA + email verify + abandon-rate sanity check + bot detection. |
| **Translation legal terms** incorrect (pt-BR/es) | M | M | HIGH (legal) | M | LOW | Native speaker + Legal local review per locale; quarterly review. |
| **Signup > 3 min** (UX miss) | M | L | LOW | L | LOW | Iterate post-launch; 5-dev sample weekly. |
| **Enterprise inquiry abuse** (spam) | M | L | LOW | L | LOW | reCAPTCHA + rate limit + Sales triage. |
| **DPA semver bump trigger** missed for material change | L | M | HIGH (legal) | L | LOW | Legal review on every PR touching DPA; semver discipline; quarterly audit. |

## 16. Benchmarks SOTA externos

| Critério | Stripe Atlas | Auth0 self-service | Linear onboarding | **CoreLink target S-19** |
|---|---|---|---|---|
| Self-service signup ≤ 3 min | Yes | Yes | Yes | **Yes — measured via 5-dev workshop** |
| DPA click-through evidence-grade | Yes | Yes | Limited | **Yes — 6-field consent + JWT receipt + verifiable** |
| Atomic provisioning (transactional) | Yes | Yes | Yes | **Yes — INV-ONBOARD-ATOMIC-PROVISIONING + property test 10k** |
| Enterprise handoff orchestrated | Yes | Yes | Sales-led | **Yes — Slack + CRM atomic + 24h SLA auto-reply** |
| Conversion funnel instrumented | Yes | Yes | Yes | **Yes — 7 steps + cohort analysis** |
| DPA versioning + re-acceptance | Manual | Manual | Manual | **Yes — semver + 30d grace + degrade read-only** |
| 3+ locales DPA | Limited | Limited | Limited | **Yes — en + pt-BR + es Legal-reviewed** |
| Signup form a11y AA | Yes | Yes | Yes | **Yes — WCAG 2.2 AA axe-core 0 violations** |
| First-run experience guided | Yes | Yes | Yes | **Yes — first PAT + CLI install + quickstart link** |

**Veredito SOTA:** S-19 v1.1 atinge feature parity com Stripe Atlas em 9/9 dimensões; vantagem em DPA versioning + re-acceptance flow + INV-ONBOARD-DPA-FIRST.

## 17. References (RFCs, papers, standards)

- **GDPR Art. 28** — Processor obligations (DPA basis).
- **LGPD Art. 39** — Operator (processor) responsibilities.
- **CCPA §1798.140(v)** — service provider definition.
- **EDPB Standard Contractual Clauses (SCCs)** for international transfers.
- **NIST Privacy Framework 1.0** — onboarding privacy considerations.
- **Stripe Atlas Documentation** <https://stripe.com/atlas>.
- **Auth0 Universal Login Best Practices**.
- **System Usability Scale (SUS)** for onboarding measurement.
- **WCAG 2.2 AA** — accessibility.
- **CloudEvents v1.0.2** — DPA acceptance event.

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- DPA-first race condition (subscription activated sem DPA) → CRITICAL post-mortem + Legal review + customer notification.
- Atomicity bug signup (orphan tenant) → CRITICAL post-mortem + INV-ONBOARD-ATOMIC-PROVISIONING review.
- DPA re-acceptance flow breaks existing tenant → 5-Why + customer success outreach.
- Enterprise handoff Slack + CRM partial failure (silent) → post-mortem + retry queue strengthen.
- Conversion funnel anomaly (signup spike + 0% activation) → post-mortem (bot abuse) + bot detection strengthen.
- DPA semver bump missed for material change → CRITICAL post-mortem + Legal + retroactive re-acceptance broadcast.

## 19. Waiver policy

S-19 **NÃO PODE** promover via waiver dos seguintes itens:

- ❌ INV-ONBOARD-DPA-FIRST property test verde — legal baseline.
- ❌ INV-ONBOARD-ATOMIC-PROVISIONING — data consistency baseline.
- ❌ DPA Legal sign-off em 3 locales — regulatory baseline.
- ❌ Atomic enterprise handoff Slack + CRM — sales operational baseline.

Itens waivable com Legal + Privacy Officer + Sales lead + ADR:

- ⚠️ 3 DPA locales → 2 locales GA (en + pt-BR); es no Q1 pós-GA.
- ⚠️ Signup ≤ 3 min → ≤ 5 min com plan to reduce next sprint.
- ⚠️ Enterprise auto-reply 5 min → 30 min (com Sales SLA 24h still hard).

---

**Fim spec contract S-19 v1.1.0 SOTA.**
