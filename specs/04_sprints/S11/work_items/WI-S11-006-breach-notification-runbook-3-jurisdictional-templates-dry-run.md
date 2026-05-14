---
id: "WI-S11-006"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "AUDITED"
version: "1.2.0"
created: "2026-04-26"
updated: "2026-05-13"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-003", "FF-HR-005", "FF-HR-010"]
parent: "S-11"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "PRIVACY-MODEL"
  - "COMPLIANCE-MATRIX"
  - "INVARIANT-REGISTRY"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
tags: ["wi", "s11", "breach-notification", "rb-breach-notif", "lgpd-art-48", "gdpr-art-33", "ccpa-1798-82", "dry-run", "high-risk"]
---

# WI-S11-006 — Breach Notification Runbook RB-BREACH-NOTIF Expanded (Existing Lote 5.12 → S-11 Production-Grade) + 3 Jurisdictional Templates Drafted (LGPD ANPD PT-BR / GDPR Irish DPC EN / CCPA California AG EN; CTRL-PRIV-032 alignment) + Decision Tree Severity × Jurisdiction → Notify-Whom-When + Customer Notification Template 3 Locales + Dry-Run Tabletop Exercise Time-to-Decision-Tree-Completion ≤4h Target Privacy Officer + Legal + Security Lead + PagerDuty Escalation Matrix (privacy_model.md §11 + compliance_matrix.md §4 + §5 inheritance; templates legais drafted em `legal/breach-notification/`; dry-run semestral simulated breach scenario PII leak via log; 1 CloudEvents `dev.hugr.corelink.breach.notification_dispatched.v1` audit Object Lock 7y; Customer Notification SLA ≤72h GDPR Art. 33; ANPD ≤"tempo razoável" 72h interpretado; CCPA "most expedient time possible"; SLO interno time-to-decision ≤4h)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-11](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S11-006 |
| Título | Breach notification RB-BREACH-NOTIF expanded (existing Lote 5.12 → S-11 production-grade) + 3 jurisdictional templates LGPD ANPD PT-BR / GDPR Irish DPC EN / CCPA California AG EN drafted em `legal/breach-notification/{lgpd-anpd-template.pt-br.md,gdpr-irish-dpc-template.en.md,ccpa-state-ag-template.en.md}` + decision tree severity × jurisdiction → notify-whom-when (sprint contract §5.6 R-S11-14) + customer notification template 3 locales (PT-BR/EN/ES via WI-S11-004 mjml infra reuse) + dry-run tabletop semestral simulated breach scenario "PII leak via log" Privacy Officer + Legal + Security Lead time-to-decision ≤4h target (sprint contract R-S11-15) + PagerDuty 3 services escalation matrix (S-09 inheritance) + CTRL-PRIV-032 alignment + 1 CloudEvents canonical `dev.hugr.corelink.breach.notification_dispatched.v1` per Lote 10.9bis P0-G prefix em audit-`<region>` Object Lock 7y |
| Sprint | S-11 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-003 (PII regulatory), FF-HR-005 (CTRL-PRIV-032), FF-HR-010 (1ª regulatory full impl) |

## 1. Intent

Breach notification é o **regulatory operational readiness primary** — sem RB-BREACH-NOTIF executable + 3 jurisdictional templates pré-drafted + dry-run validated, **GDPR Art. 33** (≤72h DPA notification) + **LGPD Art. 48** (ANPD notification "tempo razoável" interpretado 72h per ANPD Res. CD/ANPD nº 15/2024 — regulamento sobre comunicação de incidente) + **CCPA §1798.82** ("most expedient time possible") ficam unfulfilled em incident real (multa LGPD 2% revenue + GDPR 4% global). Empresas que tentam draft templates durante incident → 8-16h+ time-to-notification = guaranteed SLA violation.

CoreLink existente: RB-BREACH-NOTIF stub criado Lote 5.12 (privacy_model.md §11.3 + failure_modes.md L283 ref). S-11 expand para production-grade: (a) decision tree severity × tipo × jurisdiction → notify-whom-when; (b) 3 templates legais drafted by Legal externo + Privacy Officer review; (c) customer notification template 3 locales; (d) PagerDuty 3 services escalation matrix; (e) dry-run semestral tabletop com simulated breach scenario; (f) time-to-decision ≤4h target.

```yaml
# legal/breach-notification/rb-breach-notif-decision-tree.yaml

severity_classification:
  - severity: "SEV-1"
    criteria: "PII de > 1000 titulares afetados OR sensitive data (Art. 5 II LGPD; webauthn pubkey) leaked OR cross-tenant data exposure"
    notify:
      - { authority: "ANPD", jurisdiction: "BR", deadline_hours: 72, template: "lgpd-anpd-template.pt-br.md" }
      - { authority: "Irish DPC", jurisdiction: "EU", deadline_hours: 72, template: "gdpr-irish-dpc-template.en.md" }
      - { authority: "California AG", jurisdiction: "US-CA", deadline_hours: 72, template: "ccpa-state-ag-template.en.md", caveat: "most_expedient_time_possible" }
    customer_notification:
      deadline_hours: 72
      template: "customer-breach-notification.{locale}.mjml"
      delivery: "Cloudflare Email transactional + status page banner"
    internal_escalation:
      pagerduty_service: "corelink-breach-sev1"
      who: ["Privacy Officer", "Legal", "Security Lead", "CEO interim Gustavo"]
      decision_owner: "Privacy Officer"

  - severity: "SEV-2"
    criteria: "PII de 100-1000 titulares OR availability loss > 24h (GDPR Art. 33 inclui loss of availability) OR integrity compromise affecting subset"
    notify:
      - { authority: "ANPD", jurisdiction: "BR", deadline_hours: 72, template: "lgpd-anpd-template.pt-br.md" }
      - { authority: "Irish DPC", jurisdiction: "EU", deadline_hours: 72, template: "gdpr-irish-dpc-template.en.md" }
      # CCPA SEV-2 evaluated case-by-case (threshold lower than 1000 mas avoid over-disclosure)
    customer_notification:
      deadline_hours: 72
      delivery: "Cloudflare Email transactional"
    internal_escalation:
      pagerduty_service: "corelink-breach-sev2"
      who: ["Privacy Officer", "Legal", "Security Lead"]
      decision_owner: "Privacy Officer"

  - severity: "SEV-3"
    criteria: "PII de < 100 titulares OR pseudonymized data exposure OR confirmed-no-PII near-miss"
    notify: []  # internal post-mortem only; regulatory escalation per Privacy Officer judgment
    customer_notification:
      deadline_hours: 168  # 7d if individual customer awareness needed
      delivery: "case-by-case email"
    internal_escalation:
      pagerduty_service: "corelink-breach-sev3"
      who: ["Privacy Officer"]
      decision_owner: "Privacy Officer"
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + customer cost protection + service protection discipline justification)

### 2.1 Contexto

GDPR Art. 33.1 estabelece: "In the case of a personal data breach, the controller shall **without undue delay and, where feasible, not later than 72 hours after having become aware of it**, notify the personal data breach to the supervisory authority". Art. 33.3 enumera content: nature of breach, data categories, approximate count of subjects, consequences, mitigation. LGPD Art. 48 estabelece notification a ANPD em "prazo razoável" — ANPD Resolução CD/ANPD nº 15/2024 (Regulamento sobre comunicação de incidente de segurança que possa acarretar risco ou dano relevante aos titulares) esclarece "preferencialmente em até 2 dias úteis" (≈ 48h business days; conservative interpret ≤72h calendar). CCPA §1798.82 exige notification "in the most expedient time possible and without unreasonable delay" — California courts interpretado "expedient" como ≤72h em majority of cases.

CoreLink existing state: RB-BREACH-NOTIF stub criado Lote 5.12 com basic structure (contatos + templates + checklist). Gap S-11 production-grade: decision tree formalizado, templates drafted by Legal externo, dry-run validated, PagerDuty escalation matrix, customer notification 3 locales, time-to-decision ≤4h SLO interno (operational readiness — sem isso, 72h é guaranteed miss porque triage + decision + draft + review ≥ 8-16h tipicamente).

### 2.2 Abordagem

Decision tree em YAML (legal/breach-notification/rb-breach-notif-decision-tree.yaml) classifica severity (SEV-1/2/3) × jurisdiction (BR/EU/US-CA) → notify-whom-when. 3 templates legais drafted em `legal/breach-notification/` por Legal externo + Privacy Officer review (PT-BR para ANPD, EN para Irish DPC + California AG; CCPA tem state AGs múltiplos — California é supervisory primary). Customer notification template em 3 locales (PT-BR/EN/ES via WI-S11-004 mjml infra). PagerDuty 3 services escalação matrix (S-09 inheritance: corelink-breach-sev1, sev2, sev3). Dry-run semestral tabletop com simulated breach scenario (ex.: "PII leak via log: stack trace contém email of 50 users em log Loki"); Privacy Officer + Legal + Security Lead executam decision tree → measure time-to-decision-tree-completion. Target SLO: ≤ 4h (achievable se templates pré-drafted + escalation matrix configurada).

1 CloudEvents canonical `dev.hugr.corelink.breach.notification_dispatched.v1` audit Object Lock 7y emit fail-CLOSED quando notificação despachada (regulatory evidence).

### 2.3 Valor entregue

- **GDPR Art. 33 + LGPD Art. 48 + CCPA §1798.82 alignment absoluto**: 72h notification SLA achievable.
- **CTRL-PRIV-032 satisfação**: runbook executable + dry-run semestral.
- **Time-to-decision ≤ 4h**: SLO interno garantia 72h regulatory SLA.
- **Pre-drafted templates**: incident não bloqueado em legal drafting (ANPD/DPC/AG templates pré-revisado).
- **Customer trust**: notification template 3 locales + status page banner.
- **Audit-grade**: 1 CloudEvents R2 7y + dry-run runbook execution evidence (EVT-017 + EVT-019).

### 2.4 Principais riscos & trade-offs

- **Pre-drafted vs custom-per-incident templates**: pre-drafted faster; custom mais accurate per breach specifics. **Mitigation**: pre-drafted é fill-in-the-blanks (variables marcados claramente); Legal review final pre-dispatch ainda required.
- **3 jurisdictional templates only**: cobertura limitada US (California only; outros US states tem laws specific) + ignora UK ICO + Brazil ANPD only. **Mitigation**: pre-GA template sufficient para target markets (US-CA + EU + BR); ICO + state-by-state expansion S-19+ se enterprise pedir.
- **Dry-run semestral vs trimestral**: trimestral mais frequent practice mas Legal + Privacy time-consuming. **Mitigation**: semestral cap; ad-hoc tabletop encouraged em onboarding novo team member.
- **Time-to-decision ≤ 4h aggressive**: typical industry ≥ 8h. Achievable se templates pré-drafted + decision tree clear + on-call ready. **Mitigation**: SLO interno (não regulatory); miss → tabletop review + remediate.

## 3. Customer Impact & Journey

### 3.1 Personas afetadas

- **Privacy-aware data subject**: receives notification email se affected (sev-1/2; severity-3 caso-a-caso).
- **Privacy Officer interno HuGR**: decision owner em incident real + dry-run.
- **Legal team interno + externo**: review templates pre-dispatch.
- **Security Lead**: triage + containment per RB-BREACH-NOTIF cross-reference.
- **External regulator (ANPD, Irish DPC, California AG)**: receives notification per template em jurisdiction-appropriate language.

### 3.2 Customer journey (touchpoints)

1. Detection: incident detected via SEV-1 alert (S-09 PagerDuty).
2. Declaration: oncall declares breach via `corelink-breach-sev1` PagerDuty service ≤ 1h post-detection (privacy_model.md §11.2 timeline).
3. Triage + containment: Security Lead + SRE ≤ 4h.
4. Decision tree consultation: Privacy Officer + Legal + Security Lead executam RB-BREACH-NOTIF; classify severity × jurisdiction.
5. Templates filled-in (Legal review final ≤ 2h additional buffer).
6. Notification dispatched: regulators via email/portal + customers via Cloudflare Email + status page banner.
7. `breach.notification_dispatched.v1` audit event emitted.
8. Post-mortem ≤ 14d (privacy_model.md §11.2).

### 3.3 Jornadas (User Journeys) afetadas

- **Status page** (S-09 inheritance): banner activated em sev-1/2 dispatch.
- **Customer dashboard** (S-13 admin plane): incident reference linked.
- **Privacy notice** (WI-S11-004): historical breach references appended em changelog (transparency).

### 3.4 Métricas de customer-visible

- **Time-to-notification SLA**: ≤ 72h regulatory; tracked via `corelink_breach_time_to_notification_seconds{jurisdiction}` histogram.
- **Time-to-decision-tree SLO**: ≤ 4h internal; `corelink_breach_time_to_decision_seconds`.
- **Dry-run pass rate**: 100% sustained semestrally; `corelink_breach_dry_run_completion_total{outcome}`.
- **Customer notification delivery rate**: ≥ 95% (similar to WI-S11-005 broadcast SLO).

### 3.5 Comunicação ao customer

3 locales (PT-BR/EN/ES) email transactional template `customer-breach-notification.<locale>.mjml`. Status page banner activated em sev-1 dispatch (S-09 inheritance). Privacy Officer + Comms team coordenam messaging.

### 3.6 Mitigação de fricção

- **Pre-drafted templates**: incident NOT bloqueado em legal drafting.
- **PagerDuty 3 services**: clear escalation per severity sem dispatch confusion.
- **Decision tree YAML**: deterministic logic; sem ad-hoc judgment em panic mode.
- **Tabletop semestral**: team familiar com process pre-incident.

### Anti-pattern ❌

❌ Templates drafted post-incident (timeline blow); ❌ Single jurisdiction template (multi-jurisdiction missed); ❌ Sem dry-run (process untested); ❌ Sem PagerDuty escalation (oncall confusion); ❌ Customer notification single locale (LGPD Art. 9 violation); ❌ Sem audit event (regulatory evidence gap); ❌ Time-to-decision > 8h (72h SLA at risk).

## 4. Capability Mapping / Trace

| Camada | ID | Item |
|---|---|---|
| Sprint contract | R-S11-14 + R-S11-15 | Breach notification runbook + 3 jurisdictional templates + dry-run |
| CAPs | CAP-PRIV-005 | Breach notification runbook executable |
| Invariantes | INV-AUDIT-APPEND-ONLY (CRITICAL §3.6 L116) |
| Controles | CTRL-PRIV-032 (breach notification runbook) |
| Padrões | PAT-RUNBOOK-DRILL-001 (resilience_patterns.md §3.7 canonical; semestral tabletop específico para breach notification — extends monthly drill SLA) |
| Failure modes | FM-061 (audit Object Lock vs DSR — legal hold path); FM-450 (erasure-incomplete cross-backend; can trigger breach se PII lingers) |
| Métricas | `corelink_breach_time_to_notification_seconds{jurisdiction}` + `corelink_breach_time_to_decision_seconds` + `corelink_breach_dry_run_completion_total{outcome}` + `corelink_breach_customer_notification_delivery_total{outcome}` |
| Eventos | 1 CloudEvent canonical: breach.notification_dispatched |
| Evidence | EVT-017 (RUNBOOK_EXECUTION dry-run evidence) + EVT-019 (POSTMORTEM real incident) + EVT-044 (LEGAL_REVIEW per template) + EVT-047 (AUDIT_EVENT 7y) |

## 5. Tipo e Classificação

### 5.1 Tipo primário

`feature/regulatory-baseline` — breach notification operational readiness.

### 5.2 Prioridade

**P0** — sprint contract §6 DoD bloqueante (CTRL-PRIV-032 satisfaction; sem isso 72h SLA at risk).

### 5.3 Blast radius

**Cross-tenant + cross-region + regulatory** — incident real impacta múltiplos titulares + regulators de múltiplas jurisdictions.

### 5.4 Reversibilidade

**Não-reversível** após dispatch (regulatórios não permitem un-notify). Templates podem ser updated for next incident. Dry-run reversível (simulated only).

### 5.5 Experiment? (A/B test, feature flag experiment)

Não — regulatory feature.

### 5.6 Compliance triggers

- LGPD Art. 48 (comunicação à autoridade nacional + comunicação ao titular Art. 48 § 1º).
- GDPR Art. 33 (notification to supervisory authority), Art. 34 (notification to data subjects).
- CCPA §1798.82 (notification to California AG + affected residents).
- ANPD Res. CD/ANPD nº 15/2024 — regulamento sobre comunicação de incidente de segurança (incident notification requirements).
- EDPB Guidelines 9/2022 (personal data breach notification).
- SOC 2 CC7.4..7.5 (incident response).
- ISO/IEC 27701 §5.3 (incident management).

## 6. Escopo

### 6.1 Em escopo (exaustivo)

1. `legal/breach-notification/rb-breach-notif-decision-tree.yaml` decision tree YAML (severity × jurisdiction → notify-whom-when).
2. 3 jurisdictional templates drafted (Legal externo + Privacy Officer review pre-merge):
   - `legal/breach-notification/lgpd-anpd-template.pt-br.md` — ANPD notification (LGPD Art. 48 + ANPD Res. 15/2024).
   - `legal/breach-notification/gdpr-irish-dpc-template.en.md` — Lead supervisory authority Irish DPC (GDPR Art. 33).
   - `legal/breach-notification/ccpa-state-ag-template.en.md` — California AG (CCPA §1798.82).
3. Customer notification template 3 locales:
   - `legal/breach-notification/customer-breach-notification.{pt-BR,en-US,es-MX}.mjml` (reuse WI-S11-004 mjml infra).
4. RB-BREACH-NOTIF runbook expanded em `specs/05_quality/runbooks/RB-BREACH-NOTIF.md` (existing Lote 5.12 stub → production-grade):
   - Section 1: Decision tree summary + YAML cross-ref.
   - Section 2: Severity classification criteria.
   - Section 3: Notification flow per jurisdiction (regulators + customers).
   - Section 4: Templates fill-in instructions (variables marcados).
   - Section 5: Dry-run procedure semestral.
   - Section 6: Post-mortem hooks + 14d window (privacy_model.md §11.2).
   - Section 7: Status page banner activation steps.
   - Section 8: PagerDuty 3 services escalation matrix.
5. PagerDuty 3 services configuration (S-09 inheritance):
   - `corelink-breach-sev1` (Privacy Officer + Legal + Security Lead + CEO interim).
   - `corelink-breach-sev2` (Privacy Officer + Legal + Security Lead).
   - `corelink-breach-sev3` (Privacy Officer).
6. Dry-run procedure documented:
   - Semestral cadence (Q1 + Q3).
   - Simulated breach scenarios pre-drafted (3 examples: PII leak via log, R2 cross-tenant exposure, audit chain integrity break).
   - Time-to-decision-tree-completion measurement (target ≤ 4h).
   - EVT-017 RUNBOOK_EXECUTION evidence captured (asciinema or video + timestamps).
7. 1 CloudEvent canonical `dev.hugr.corelink.breach.notification_dispatched.v1` em audit-`<region>` Object Lock 7y; emit fail-CLOSED.
8. Crate `corelink-privacy-breach-emit` (NEW small crate) — emit helper para audit event + delivery confirmation tracking.
9. 4 Prom metrics for breach observability (time-to-notification, time-to-decision, dry-run completion, customer delivery).
10. Status page banner integration (S-09 inheritance) — banner.json updates per breach severity.
11. Cross-reference RB-FM-* runbooks (e.g., RB-GDPR-ERASURE-HOLD audit Object Lock conflict; RB-DSR-ERASURE-INCOMPLETE WI-S11-002 cross-link).

#### 6.1.4 RB-BREACH-NOTIF runbook structure (excerpt)

```markdown
# RB-BREACH-NOTIF — Breach Notification Runbook

> **Status**: Production-grade (S-11 expansion of Lote 5.12 stub)
> **Owner**: Privacy Officer (Gustavo interim)
> **Reviewer**: Legal + Compliance Officer + Security Lead
> **Cadence**: Triggered by SEV-1/2/3 breach incidents; dry-run semestrally

## Section 1: Decision Tree Summary

See `legal/breach-notification/rb-breach-notif-decision-tree.yaml` for canonical YAML.
Severity classification → notify whom × when × template.

## Section 2: Severity Classification Criteria

[3 criteria tables sev-1/2/3]

## Section 3: Notification Flow

### 3.1 ANPD (BR; LGPD Art. 48)
[Steps to notify ANPD: portal URL, contact email, template path, attestations]

### 3.2 Irish DPC (EU; GDPR Art. 33; lead supervisory)
[Steps to notify Irish DPC: portal URL, contact email, template path]

### 3.3 California AG (US-CA; CCPA §1798.82)
[Steps to notify California AG: portal URL, contact email, template path]

### 3.4 Customers
[Cloudflare Email transactional + status page banner activation]

## Section 4: Templates Fill-in Instructions

Variables marcados em templates como `{{...}}`:
- `{{breach_id}}`: ULID assigned em incident declaration.
- `{{breach_detected_at}}`: ISO 8601 UTC.
- `{{data_categories_affected}}`: enum from privacy_model.md §2.
- `{{count_subjects_affected}}`: approximate integer.
- `{{containment_actions}}`: free-text bullet list.
- `{{mitigation_offered}}`: free-text bullet list (e.g., "credential rotation; 12mo identity monitoring").
- `{{contact_email}}`: privacy@hugr.dev (canonical).
- `{{breach_severity}}`: SEV-1 | SEV-2 | SEV-3.

## Section 5: Dry-Run Procedure

Semestral (Q1 + Q3):
1. Privacy Officer schedules tabletop with Legal + Security Lead + Compliance.
2. Pre-drafted scenario selected from `legal/breach-notification/dry-run-scenarios/`.
3. Decision tree consultation simulated; templates filled-in (mock data); regulator/customer notification mocked.
4. Time-to-decision-tree-completion measured (target ≤ 4h).
5. Asciinema recording archived em R2 evidence-runbooks/<date>-rb-breach-notif-dry-run.cast (EVT-017).
6. Post-dry-run review: gaps identified → action items + ADR if process changes.

## Section 6: Post-Mortem Hooks

[14d window; Privacy Officer + Security Lead lead; público se sev-1]

## Section 7: Status Page Banner

[curl + Cloudflare Pages config update steps]

## Section 8: PagerDuty Escalation Matrix

[3 services × who/when]
```

### 6.2 Componentes C4 afetados

- **PagerDuty** (S-09 inheritance): 3 services configuration.
- **Cloudflare Email** (S-13 inheritance): customer notification 3 locales transactional.
- **Cloudflare Pages status page** (S-09 inheritance): banner activation.
- **R2 audit-`<region>`**: 1 canonical CloudEvent Object Lock 7y.
- **R2 evidence-runbooks/**: dry-run asciinema + post-mortem docs (EVT-017 + EVT-019).
- **R2 evidence-legal/**: 3 jurisdictional templates Legal Review PDFs (EVT-044).

### 6.3 Arquivos do repositório

```
legal/breach-notification/
├─ rb-breach-notif-decision-tree.yaml          # canonical decision tree
├─ lgpd-anpd-template.pt-br.md                 # ANPD template
├─ gdpr-irish-dpc-template.en.md               # Irish DPC template
├─ ccpa-state-ag-template.en.md                # California AG template
├─ customer-breach-notification.pt-BR.mjml     # 3 locales customer email
├─ customer-breach-notification.en-US.mjml
├─ customer-breach-notification.es-MX.mjml
└─ dry-run-scenarios/
   ├─ scenario-1-pii-leak-via-log.md
   ├─ scenario-2-r2-cross-tenant-exposure.md
   └─ scenario-3-audit-chain-integrity-break.md

specs/05_quality/runbooks/RB-BREACH-NOTIF.md              # production-grade expansion (existing Lote 5.12)

crates/corelink-privacy-breach-emit/           # NEW small crate
├─ Cargo.toml
└─ src/
   └─ lib.rs                                   # 1 CloudEvent emitter

specs/03_architecture/adrs/
└─ ADR-S11-009-breach-notification-3-jurisdictional-coverage-rationale.md  # NEW

# 4 Prom metrics + Grafana dashboard reuse S-09 infra
```

### 6.4 Sistemas externos tocados

- **PagerDuty** (3 services).
- **Cloudflare Email** (transactional 3 locales).
- **Cloudflare Pages** (status page banner).
- **R2** (audit + evidence-runbooks + evidence-legal).

## 7. Anti-Scope

- ❌ DSR API endpoints — entregue em WI-S11-001.
- ❌ Erasure worker — entregue em WI-S11-002.
- ❌ Consent ledger — entregue em WI-S11-003.
- ❌ Privacy notice content — entregue em WI-S11-004.
- ❌ Sub-processor register — entregue em WI-S11-005.
- ❌ Residency pinning — entregue em WI-S11-007.
- ❌ DPIA + LIA + TLA+ — entregue em WI-S11-008.
- ❌ UK ICO template + state AGs (other than California) — pós-GA enterprise expansion S-19+.
- ❌ Public breach disclosure page (e.g., breach.hugr.dev) — anti-scope (status page banner suffice).
- ❌ Cyber insurance integration — Finance + Legal scope.
- ❌ Forensic incident response toolkit — Security Lead scope (this WI documents handoff to incident response process).

### Anti-pattern ❌

❌ Templates drafted post-incident; ❌ Single jurisdiction; ❌ Sem dry-run; ❌ Sem PagerDuty escalation; ❌ Single locale customer notification; ❌ Sem audit event; ❌ Time-to-decision > 8h.

## 8. Acceptance Criteria (Gherkin) — 6 scenarios

### AC-001: Decision tree YAML schema validation

```gherkin
Given legal/breach-notification/rb-breach-notif-decision-tree.yaml committed
When CI hook valida YAML schema
Then schema validates: severity_classification list com 3 entries (SEV-1/2/3)
And cada entry tem fields {severity, criteria, notify (list), customer_notification, internal_escalation}
And notify list per entry tem fields {authority, jurisdiction, deadline_hours, template, optional caveat}
And templates referenced em yaml existem em legal/breach-notification/ filesystem
And NO YAML schema violations
```

### AC-002: 3 jurisdictional templates drafted + Legal Review

```gherkin
Given 3 templates filed em legal/breach-notification/{lgpd-anpd,gdpr-irish-dpc,ccpa-state-ag}-template.{pt-br,en,en}.md
And metadata em cada template com legal_review_evt-044_path apontando para R2 evidence-legal/
When CI hook valida templates
Then 3 PDFs Legal Review existem em R2 com size > 0 (signed PDF per template)
And template variables `{{...}}` consistent entre 3 templates (8 canonical vars per RB-BREACH-NOTIF section 4)
And Privacy Officer review checkbox = true em metadata
And template content > 500 words per template (substantive content; not stub)
```

### AC-003: Dry-run tabletop happy path

```gherkin
Given Privacy Officer schedules dry-run Q1 com scenario "scenario-1-pii-leak-via-log.md"
And Privacy Officer + Legal + Security Lead disponíveis
When tabletop é executed
Then participants consultam decision tree → classify scenario as SEV-2
And templates filled-in (mock data via dry-run scenario placeholders)
And regulator/customer notification mocked (NOT actual dispatch)
And asciinema recording archived em R2 evidence-runbooks/<date>-rb-breach-notif-dry-run.cast (EVT-017)
And `corelink_breach_dry_run_completion_total{outcome='passed'}` Prom counter incrementado
And time-to-decision-tree-completion measured ≤ 4h target
And se > 4h, outcome='passed_with_concerns' + action items + Privacy Officer review
```

### AC-004: Real breach SEV-1 dispatch flow simulation

```gherkin
Given simulated SEV-1 incident: PII leak via log; 1500 titulares affected
When oncall declares breach via PagerDuty corelink-breach-sev1 service
Then PagerDuty alert escalates Privacy Officer + Legal + Security Lead + CEO interim within 15min (S-09 inheritance)
And Privacy Officer + Legal + Security Lead initiate decision tree consultation
And severity classified SEV-1 (per criteria > 1000 titulares)
And templates LGPD ANPD + GDPR Irish DPC + CCPA California AG selected
And Legal review final ≤ 2h additional buffer
And templates filled-in: {{breach_id}}, {{breach_detected_at}}, {{data_categories_affected}}, etc.
And notification dispatched a 3 regulators within 72h
And customer notification dispatched via Cloudflare Email transactional (3 locales) + status page banner
And `dev.hugr.corelink.breach.notification_dispatched.v1` emitido em audit-`<region>` Object Lock 7y com payload {breach_id, severity, jurisdictions_notified, customer_notifications_sent, ts}
And `corelink_breach_time_to_notification_seconds{jurisdiction='ANPD'}` histogram observado ≤ 72h * 3600 = 259200
```

### AC-005: Audit fail-CLOSED em emit failure

```gherkin
Given audit emit infrastructure (R2 audit-`<region>`) está indisponível
When breach notification dispatched but emit fails
Then dispatch step blocks (transaction-like behavior)
And SEV-1 alert disparado (audit emit gap = regulatory evidence gap = CRITICAL)
And rollback-safe: notification ainda dispatched (regulatory SLA priority) MAS additional out-of-band alert para SecLead + Compliance
And distinct from billing fail-OPEN; this is regulatory-grade so failure is documented but not blocking
And manual recovery: `breach.notification_dispatched.v1` re-emitted post-recovery com same payload + retry_attempt incremented
```

### AC-006: PagerDuty 3 services escalation matrix

```gherkin
Given 3 PagerDuty services configurados (corelink-breach-sev1, sev2, sev3)
When SEV-1 breach declared
Then corelink-breach-sev1 service alert
And escalation policy: Privacy Officer (15min) → Legal (30min) → Security Lead (45min) → CEO interim (60min)
And se SEV-2 instead, only corelink-breach-sev2 alerted (Privacy Officer + Legal + Security Lead, no CEO)
And se SEV-3, only Privacy Officer alerted
And property test 100 random (severity, alert_recipients) combinations: 100% match expected escalation
```

## 9. Design Decisions

### 9.1 Decisões locais

- **DD-001 Decision tree YAML vs hardcoded Rust**: YAML editable by Privacy Officer + Legal sem code review; CI validates schema. Hardcoded Rust safer (compile-time check) mas slower iteration. Decisão: YAML + schema validation (privacy + legal autonomy primary).
- **DD-002 3 jurisdictional templates only**: BR + EU + US-CA cobrem 90% target market; UK/state AGs/India/China expand pós-GA per ADR-S11-009.
- **DD-003 Decision tree YAML em legal/ vs specs/**: legal/ é monitored by Privacy Officer + Legal team (canonical home for legal artifacts). specs/ would mix concerns.
- **DD-004 Dry-run semestral (não trimestral)**: balance practice vs Legal time. Q1 + Q3 cadence aligned com fiscal year + quarterly reviews.
- **DD-005 Time-to-decision-tree-completion ≤ 4h SLO interno**: ≤ 4h achievable se templates pré-drafted; ≤ 8h industry standard. Aggressive target intencional (operational readiness).

### 9.2 Decisões que justificam ADR

- **ADR-S11-009 (NEW)**: 3 jurisdictional templates coverage rationale (BR + EU + US-CA). Rationale: target market coverage early-stage; UK ICO + state AGs (other than CA) + India DPDPA + China PIPL deferred pós-GA. Privacy Officer + Compliance Officer sign-off em decisão. Quarterly review se enterprise customer materialize com other jurisdictions.

### 9.3 Trade-offs explícitos

| Trade-off | Opção A | Opção B | Decisão | Rationale |
|---|---|---|---|---|
| Decision tree format | YAML | Hardcoded Rust | A | Privacy + Legal autonomy |
| Jurisdiction coverage | 3 (BR/EU/US-CA) | 6+ (incl. UK/state AGs) | A | Target market early-stage; ADR-S11-009 expansion plan |
| Dry-run cadence | Semestral | Trimestral | A | Legal time balance |
| Time-to-decision SLO | ≤ 4h | ≤ 8h | ≤ 4h | Operational readiness aggressive |
| Templates location | legal/ | specs/ | A | Privacy + Legal monitoring |
| Audit failure behavior | Block dispatch | Dispatch + alert | B | Regulatory SLA priority over audit completeness (with retry) |

### Anti-pattern ❌

❌ Hardcoded decision tree (legal autonomy gap); ❌ 1 jurisdiction template (multi-jurisdiction missed); ❌ Trimestral dry-run (Legal burnout); ❌ ≤ 8h SLO target (industry baseline; not aggressive enough); ❌ Templates em specs/ (location confusion); ❌ Block dispatch on audit failure (regulatory SLA violation).

## 10. Completeness Criteria SOTA

### 10.1 Code Completeness

- [ ] **C-1.1** legal/breach-notification/ directory + 3 jurisdictional templates + decision tree YAML + 3 customer notification mjml templates + 3 dry-run scenarios committed.
- [ ] **C-1.2** RB-BREACH-NOTIF.md production-grade expansion committed (sections 1-8 per §6.1.4).
- [ ] **C-1.3** Crate `corelink-privacy-breach-emit` (NEW; 1 CloudEvent emitter) compila WASM.
- [ ] **C-1.4** PagerDuty 3 services configurados via Terraform (S-09 inheritance pattern).
- [ ] **C-1.5** ADR-S11-009 jurisdictional coverage rationale committed.
- [ ] **C-1.6** Decision tree YAML schema validator script.
- [ ] **C-1.7** 3 Legal Review PDFs em R2 evidence-legal/ (per template).

### 10.2 Test Completeness

- [ ] **T-2.1** Integration test simulated breach flow (PagerDuty → decision tree → templates → audit emit) verde.
- [ ] **T-2.2** Property test escalation matrix 100k random (severity, alert_recipients) → 100% match expected.
- [ ] **T-2.3** Regression test decision tree YAML schema validation.
- [ ] **T-2.4** Regression test 3 templates variables consistency (8 canonical vars per RB-BREACH-NOTIF section 4).
- [ ] **T-2.5** Chaos test PagerDuty service offline → fallback alert via email + status page banner.
- [ ] **T-2.6** Chaos test audit emit failure → dispatch ainda procede (regulatory SLA priority); SEV-1 alert + retry.
- [ ] **T-2.7** Dry-run staging integration test: scenario execution end-to-end com mock data.

### 10.3 Documentation Completeness

- [ ] **D-3.1** RB-BREACH-NOTIF.md production-grade (sections 1-8).
- [ ] **D-3.2** ADR-S11-009 jurisdictional coverage rationale.
- [ ] **D-3.3** docs/dev/breach-notification-architecture.md.
- [ ] **D-3.4** Dry-run procedure SOP em RB-BREACH-NOTIF section 5.

### 10.4 Observability Completeness

- [ ] **O-4.1** 4 Prom metrics: time_to_notification_seconds, time_to_decision_seconds, dry_run_completion_total, customer_notification_delivery_total.
- [ ] **O-4.2** 1 dashboard `corelink-breach-notification` em Grafana (S-09 inheritance) com 6 panels.
- [ ] **O-4.3** 1 CloudEvent canonical em audit-`<region>` Object Lock 7y.
- [ ] **O-4.4** 3 PagerDuty services configurados.

### 10.5 Security & Privacy Completeness

- [ ] **S-5.1** Templates Legal Review pre-merge (3 PDFs em R2 evidence-legal/).
- [ ] **S-5.2** Customer notification 3 locales mandatory (PT-BR/EN/ES via WI-S11-004 mjml infra).
- [ ] **S-5.3** Decision tree YAML schema validation.
- [ ] **S-5.4** GDPR Art. 33/34 + LGPD Art. 48 + CCPA §1798.82 alignment Legal Review pre-merge.
- [ ] **S-5.5** Time-to-notification SLA tracked + reported to Privacy Officer + Compliance.

### 10.6 SBOM Completeness

- [ ] **B-6.1** SBOM CycloneDX 1.5+ inclui `corelink-privacy-breach-emit`.

## 11. DoD

10.x checked + sign-off matrix §30 + dry-run executed em staging com EVT-017 evidence + 3 Legal Review PDFs.

## 12. Invariants Validated

| INV | Severity | Position canonical | Cobertura WI-S11-006 |
|---|---|---|---|
| **INV-AUDIT-APPEND-ONLY** | CRITICAL | invariant_registry.md §3.6 L116 | 1 CloudEvent canonical em audit-`<region>` Object Lock 7y; emit fail-CLOSED-ish (dispatch priority over audit; retry post-recovery) |

## 13. Artifacts Produced

- `legal/breach-notification/` 7 files (1 YAML decision tree + 3 jurisdictional templates + 3 customer notification mjml).
- `legal/breach-notification/dry-run-scenarios/` 3 scenarios.
- `specs/05_quality/runbooks/RB-BREACH-NOTIF.md` production-grade expansion (sections 1-8).
- `crates/corelink-privacy-breach-emit/` (NEW; ~400 LoC).
- ADR-S11-009 jurisdictional coverage rationale.
- 1 CloudEvent schema em `schemas/cloudevents/breach-notification-dispatched.v1.json`.
- PagerDuty 3 services Terraform config.
- 4 Prom metrics + Grafana dashboard JSON.
- 3 Legal Review PDFs em R2 evidence-legal/.

## 14. Quality Standards SOTA

- **14.s11.6.1** GDPR Art. 33/34 + LGPD Art. 48 + CCPA §1798.82 Legal Review pre-merge.
- **14.s11.6.2** Time-to-decision-tree-completion ≤ 4h target em dry-run sustained.
- **14.s11.6.3** Time-to-notification ≤ 72h regulatory SLA per jurisdiction.
- **14.s11.6.4** Dry-run semestral (Q1 + Q3) com EVT-017 evidence.
- **14.s11.6.5** Templates variables consistency cross-jurisdictional (8 canonical vars).
- **14.s11.6.6** Customer notification 3 locales mandatory at GA.
- **14.s11.6.7** PagerDuty 3 services escalation matrix property test 100k.
- **14.s11.6.8** INV §3.X positions canonical verified pre-merge (Lote 10.8bis P1-13).

## 15. Chaos Experiments (5)

1. PagerDuty service offline → fallback email + status page banner; verify operational continuity.
2. Audit emit failure mid-dispatch → dispatch ainda procede; SEV-1 alert + retry post-recovery.
3. Cloudflare Email outage → status page banner ainda activated; manual customer email fallback.
4. Decision tree YAML schema bypass attempt → CI hook detects + blocks PR merge.
5. Dry-run scenario execution timeout > 4h → outcome='passed_with_concerns' + action items.

## 16. PRR

PRR HIGH_RISK 12 sign-offs + dry-run executed em staging + 3 Legal Review PDFs em R2 + Privacy + Compliance + Architect mandatory emphatic.

## 17. Sub-tasks

| ID | Descrição | PERT |
|---|---|---|
| ST-001 | legal/breach-notification/rb-breach-notif-decision-tree.yaml | 1.5h |
| ST-002 | 3 jurisdictional templates drafted (Legal externo lead; eng support template variables) | 4h |
| ST-003 | 3 customer notification mjml 3 locales | 1h |
| ST-004 | RB-BREACH-NOTIF.md production-grade expansion (sections 1-8) | 3h |
| ST-005 | corelink-privacy-breach-emit crate (1 CloudEvent) | 1h |
| ST-006 | PagerDuty 3 services Terraform config | 1h |
| ST-007 | ADR-S11-009 jurisdictional coverage rationale | 0.5h |
| ST-008 | Decision tree YAML schema validator | 0.7h |
| ST-009 | 3 dry-run scenarios pre-drafted | 1.5h |
| ST-010 | 4 Prom metrics + Grafana dashboard | 1h |
| ST-011 | Dry-run integration test em staging | 1.5h |

**PERT total**: ~20.7h (alinha com sprint contract §12).

## 18. Dependencies

- **Hard**: spec contract S-11 v1.2.0 SEALED; S-09 SEALED (PagerDuty + status page banner + audit emit + Cloudflare Email); WI-S11-004 (mjml infra reuse customer notification 3 locales).
- **Soft**: Privacy Officer + Legal externo (BR/UE/MX advogados) — staffing pre-GA for templates + dry-run participation.

## 19. Effort PERT: ~20.7h. ## 20. Time-boxing: 32h hard limit (lane HIGH_RISK +40% buffer; Legal externo coordination warrant).

## 21. Observability

4 Prom metrics + 1 Grafana dashboard 6 panels + 1 CloudEvent canonical + 3 PagerDuty services.

## 22. Cost Analysis

- **Cloudflare Email transactional**: per breach × N customers × 3 locales; sev-1 1000+ customers ≈ $1-3/breach.
- **Cloudflare Pages status page**: free tier; ≈$0.
- **R2 evidence-legal/ Legal Review PDFs**: 3 PDFs × ~5MB × 7y = ~105MB; ≈$0.003/year.
- **R2 audit-`<region>`**: 1 event/breach × ~2 breaches/year × 7y = 14 events; ≈$0.0001.
- **R2 evidence-runbooks/**: dry-run asciinema 1-2/year × ~50MB; ≈$0.001/year.
- **PagerDuty 3 services**: $20-50/month base; included em existing budget.
- **Total estimated**: ≤ $50/year + PagerDuty base (well under §14 budget).

## 23. API Contract

1 CloudEvent schema em `schemas/cloudevents/breach-notification-dispatched.v1.json`. Decision tree YAML schema em `schemas/yaml/breach-notification-decision-tree.json`.

## 24. Post-mortem Hooks

| Trigger | Severity | Owner |
|---|---|---|
| Time-to-notification > 72h regulatory SLA miss | CRITICAL | Privacy Officer + Legal escalation; ANPD/DPC potential disclosure |
| Dry-run > 4h SLO miss | HIGH | Privacy Officer + tabletop process review |
| INV-AUDIT-APPEND-ONLY violation (breach event NOT em R2 7y) | CRITICAL | Privacy Officer + SecLead |
| 3 templates Legal Review missing | HIGH | Privacy Officer + Compliance + Legal escalation |
| PagerDuty 3 services escalation miss | CRITICAL | SRE + Privacy Officer + on-call review |
| Decision tree YAML schema bypass exploit | HIGH | SecLead + Privacy + CI hardening |
| Customer notification single locale | HIGH | Privacy Officer + WI-S11-004 owner review |
| Status page banner activation miss | MEDIUM | SRE + Comms |

## 25. Rollback / Recovery

Templates revert via git; YAML decision tree revert via git. Real breach notification NOT reversible (regulatórios não permitem un-notify). Dry-run reversível (simulated only).

## 26. Security & Privacy

LINDDUN per privacy_model.md §4 + STRIDE per security_model.md §6:
- L(inkability): breach notification em audit log linkable a affected subjects (intentional regulatory evidence).
- I(dentifiability): subject names em customer notification template (intentional GDPR Art. 34 + LGPD Art. 48).
- N(on-repudiation): R2 Object Lock 7y + Legal Review per template.
- D(etectability): SEV-1 alert + status page banner intentional transparency.
- D(isclosure): regulatórios + customers receive disclosure (intentional).
- U(nawareness): customer notification 3 locales eliminate unawareness.
- N(on-compliance): **GDPR Art. 33/34 + LGPD Art. 48 + CCPA §1798.82 + SOC 2 CC7.4..7.5 + EDPB 9/2022 compliance** via runbook executable + 3 jurisdictional templates + dry-run semestral + customer notification 3 locales.

## 27. Knowledge Transfer

Tech talk (1h): "S-11 Breach Notification: 3 jurisdictional templates + decision tree YAML + dry-run semestral + PagerDuty escalation matrix"; doc `specs/05_quality/runbooks/RB-BREACH-NOTIF.md`; onboarding test 6 questões: 3 jurisdictional templates rationale (ADR-S11-009 BR/EU/US-CA target market), decision tree YAML severity classification (3 sev × 3 jurisdiction = 9 combinations), dry-run cadence semestral (DD-004), time-to-decision ≤ 4h SLO interno (DD-005 operational readiness), PagerDuty 3 services escalation, audit fail behavior (regulatory SLA priority over audit completeness).

## 28. Risk Register (12-row HIGH_RISK)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Time-to-notification > 72h | L | M | CRITICAL | M | LOW | Pre-drafted templates + decision tree YAML + PagerDuty escalation; SLO ≤ 4h decision-tree |
| R-002 | Dry-run > 4h SLO miss | M | L | LOW | L | LOW | Tabletop quarterly check + action items + Privacy Officer review |
| R-003 | Legal Review PDFs missing pre-merge | L | M | HIGH | M | LOW | CI hook strict + Privacy Officer audit |
| R-004 | PagerDuty service offline | L | L | HIGH | L | LOW | Fallback email + status page banner; SLA ≤ 99.9% per S-09 inheritance |
| R-005 | Decision tree YAML schema bypass | L | L | MEDIUM | L | LOW | CI hook strict + integration test |
| R-006 | Customer notification single locale | L | M | HIGH | M | LOW | CI gate strict 3 locales (WI-S11-004 inheritance pattern) |
| R-007 | Templates variables inconsistency | L | L | LOW | L | LOW | Cross-jurisdictional consistency CI lint (8 canonical vars) |
| R-008 | INV §3.X position drift (Lote 10.8bis P1-13) | L | L | LOW | L | LOW | INV positions §3.6 L116 verified Lote 10.11.0 |
| R-009 | Audit fail blocking dispatch (regulatory SLA violation) | L | L | CRITICAL | L | LOW | DD: dispatch priority over audit completeness; retry post-recovery |
| R-010 | Templates legal accuracy challenge (rejected by ANPD/DPC/AG) | M | M | HIGH | M | LOW | Legal externo + Privacy Officer review pre-GA; quarterly update review |
| R-011 | Dry-run scenarios stale | L | L | LOW | L | LOW | Annual scenario refresh; new scenarios per recent industry breaches |
| R-012 | Audit emit retry loop (post-recovery) | L | L | LOW | L | LOW | Idempotent retry via breach_id + retry_attempt |

## 29. Review Checkpoints

D+0 design (Architect; decision tree YAML format + audit fail trade-off); D+1 Privacy Officer (LGPD Art. 48 + EDPB 9/2022); D+2 Legal externo (BR/UE/MX template drafting); D+3 Compliance (SOC 2 CC7.4..7.5); D+4 SecLead (PagerDuty escalation + STRIDE); D+5 dry-run staging exercise; D+6 PRR.

## 30. Sign-off (HIGH_RISK 12)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver | _pending_ |
| 3 | SRE Lead | _staffing-blocked_ |
| 4 | Security Lead | _TBD; **mandatory** — incident response + STRIDE_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — dry-run integration test em staging_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory emphatic** — SOC 2 CC7.4..7.5 + GDPR Art. 33/34 + LGPD Art. 48 + CCPA §1798.82 + EDPB 9/2022_ |
| 10 | Privacy | _TBD; **mandatory emphatic** — CTRL-PRIV-032 + decision tree YAML + dry-run semestral_ |
| 11 | Architect | _TBD; **mandatory emphatic** — audit fail trade-off (regulatory SLA priority) + INV §3.X verification + ADR-S11-009_ |
| 12 | DPO interim | _TBD; **mandatory emphatic** — 3 jurisdictional coverage defensibility ANPD/DPC/AG_ |

(Legal externo sign-off via 3 Legal Review PDFs em R2 evidence-legal/; sprint-level Legal sign-off não per-WI per Lote 10.10-quaters R4 NEW-P1-4 lesson.)

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.1.0 | 2026-04-28 | Gustavo (Lote 10.11.0-bis + 10.11bis) | **Canonical fixes pós baseline review aggregate 5.18/10**: (a) **ANPD Resolução CD/ANPD nº 2/2022 → Res. CD/ANPD nº 15/2024** (Regulamento sobre comunicação de incidente de segurança que possa acarretar risco ou dano relevante aos titulares) — Res. 2/2022 tratou de procedimentos de fiscalização (citação errada no v1.0). (b) **FM-453 sub-processor breach upstream** declared canonical em failure_modes.md §3.10 (Privacy/Regulatory). (c) **CloudEvents canonical** breach.{declared,notified,dryrun}.v1 em observability_model.md §7.2. |
| 1.0.0 | 2026-04-26 | Gustavo (Lote 10.11) | Criação WI-S11-006; HIGH_RISK; SOTA pós-S-10 SEALED. Breach notification operational readiness regulatory baseline. RB-BREACH-NOTIF expanded (existing Lote 5.12 stub → S-11 production-grade) com 8 sections (decision tree summary + severity classification + notification flow + templates fill-in + dry-run procedure + post-mortem hooks + status page banner + PagerDuty escalation matrix). 3 jurisdictional templates drafted em legal/breach-notification/ (LGPD ANPD PT-BR + GDPR Irish DPC EN + CCPA California AG EN; ADR-S11-009 target market coverage rationale). Customer notification template 3 locales (PT-BR/EN/ES via WI-S11-004 mjml infra reuse). Decision tree em legal/breach-notification/rb-breach-notif-decision-tree.yaml (severity SEV-1/2/3 × jurisdiction → notify-whom-when; YAML editable by Privacy Officer + Legal sem code review per DD-001 autonomy primary). PagerDuty 3 services escalation matrix (corelink-breach-sev1/2/3 com diferentes who lists). Dry-run semestral tabletop (Q1 + Q3) com 3 pre-drafted scenarios (PII leak via log + R2 cross-tenant exposure + audit chain integrity break); time-to-decision-tree-completion ≤ 4h SLO interno target (DD-005 operational readiness aggressive vs 8h industry baseline); EVT-017 RUNBOOK_EXECUTION asciinema evidence em R2 evidence-runbooks/. 1 CloudEvent canonical `dev.hugr.corelink.breach.notification_dispatched.v1` per Lote 10.9bis P0-G prefix em audit-`<region>` Object Lock 7y. Audit fail trade-off documented (DD audit failure: dispatch ainda procede; regulatory SLA priority over audit completeness; retry post-recovery; distinct from billing fail-OPEN). 6 AC scenarios + 5 chaos + 12 risks + 8 post-mortem hooks. NEW corelink-privacy-breach-emit crate. NEW ADR-S11-009 jurisdictional coverage rationale. **Lote 10.10 lessons absorbed**: source-of-truth FIRST INV positions; typed enum (3 severities + 3 jurisdictions); sign-off cap 12; cascade discipline; corelink_time canonical helper; 3 locales mandatory at GA per sprint contract §10.s11.7; audit fail behavior trade-off canonical (regulatory > audit when conflicted). |

## 32. Anti-patterns evitados

- ❌ Templates drafted post-incident (timeline blow GDPR Art. 33 violation); ❌ Single jurisdiction template (multi-jurisdiction missed); ❌ Sem dry-run semestral (CTRL-PRIV-032 violation); ❌ Sem PagerDuty escalation matrix (oncall confusion); ❌ Customer notification single locale (LGPD Art. 9 violation); ❌ Sem audit event (regulatory evidence gap); ❌ Time-to-decision > 8h (72h SLA at risk); ❌ Hardcoded decision tree (legal autonomy gap); ❌ Block dispatch on audit failure (regulatory SLA violation); ❌ INV §3.X position TBD (Lote 10.8bis P1-13).

---
