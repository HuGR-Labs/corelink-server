---
id: "DPO-APPOINTMENT-2026-05-15"
type: "compliance_appointment"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "GAP-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "LGPD-FULL-AUDIT-2026-05-15"
  - "SOC2-EVIDENCE-ROLLUP-2026-05-15"
  - "COMPLIANCE-MATRIX"
  - "PRIVACY-MODEL"
tags:
  - "lgpd"
  - "lgpd-art-38"
  - "lgpd-art-41"
  - "gdpr-art-37"
  - "gdpr-art-38"
  - "gdpr-art-39"
  - "dpo"
  - "encarregado"
  - "appointment"
  - "gap-01"
  - "interim"
  - "anpd"
  - "governance"
---

# Interim DPO Appointment (2026-05-15)

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **purpose:** formal designation of CoreLink's interim Data Protection Officer (`encarregado pelo tratamento de dados pessoais`) pending permanent hire in Q3-2026, with reporting line, independence safeguards, term, and ANPD-template registration form.
>
> **Regulatory anchors:**
> - **LGPD Art. 41 §1º** (Lei 13.709/2018) — controller must publish DPO identity and contact in clear, objective, easily accessible form.
> - **LGPD Art. 41 §2º** — DPO duties: receive subject complaints, receive ANPD communications, orient employees, execute other tasks defined by controller.
> - **LGPD Art. 41 §3º** — ANPD may issue complementary norms on DPO definition and duties.
> - **LGPD Art. 38** — controller may be required to produce DPIA/RIPD; DPO authors or supervises authoring.
> - **GDPR Art. 37–39** (cross-framework; required for EU enterprise tenants) — DPO designation, position, tasks.
> - **ANPD Resolução CD/ANPD nº 18/2024** (DPO orientation note, in force) — DPO may be PF or PJ; may be internal or external; may accumulate duties so long as no conflict of interest.
>
> **Closure target:** GAP-01 (`SOC2-EVIDENCE-ROLLUP-2026-05-15.md §5` + `compliance_matrix.md §9` row GAP-01).
>
> **Companion docs:**
> - `specs/_compliance/DPO-RESPONSIBILITIES-MATRIX.md` — RACI for DPO-touched activities.
> - `specs/_runbooks/RB-DPO-ESCALATION.md` — escalation triggers and SLAs.
> - `specs/_compliance/DPO-HANDOFF-PLAN.md` — 90-day plan to permanent DPO.
> - `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md` — operational cadence (25 items).
> - `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md §1.12` — Art. 41 article-level audit row.

---

## 1. Designation

> By this instrument, **HuGR Labs Ltda.** ("HuGR", or "the controller"), in compliance with Article 41 of Lei nº 13.709/2018 (LGPD), formally designates the **interim Data Protection Officer (`encarregado pelo tratamento de dados pessoais`)** identified below.

| Field | Value |
|---|---|
| **Controller legal name** | HuGR Labs Ltda. |
| **Controller CNPJ** | _to be populated on H-1 Stripe-Atlas counsel handover_ |
| **Controller registered address** | _to be populated on H-1 Stripe-Atlas counsel handover_ |
| **DPO type (per ANPD Resolução 18/2024)** | Internal, natural person, accumulating founder duties (no conflict of interest documented in §4) |
| **DPO full name** | Gustavo Schneiter |
| **DPO role at HuGR** | Founder + interim Privacy Officer (no operational role on production data pipelines — see §4) |
| **DPO public email (Art. 41 §1º obligation)** | `privacy@hugr.com` (canonical, routes to DPO inbox) |
| **DPO personal contact (internal)** | `gustavo@humangr.com` |
| **Designation effective date** | 2026-05-15 |
| **Designation term** | Interim, until permanent DPO appointment (target Q3-2026, hard cap T+1m of GA per `compliance_matrix.md §9` GAP-01) |
| **Publication surface (Art. 41 §1º)** | `apps/docs/docs/explanation/privacy/lgpd-full.mdx` + `legal/privacy-notice/v*.{pt-BR,en-US,es-MX}.md §1` + this document |
| **ANPD registration** | Filed via ANPD's online form (`https://www.gov.br/anpd/`); template in §7 below |
| **Cross-framework GDPR Art. 37** | Same individual designated as GDPR DPO; gating on first EU enterprise tenant (not SAM GA) |

---

## 2. Reporting line

The interim DPO reports **directly to the HuGR Labs Board** (currently constituted as the founding member's solo proprietorship until external investors join post-GA), bypassing the operational engineering chain. This satisfies:

- **LGPD Art. 41 §2º interpretive requirement** that the DPO be able to communicate ANPD directives without operational filtering.
- **GDPR Art. 38(3)** that the DPO not be dismissed or penalised for performing tasks.
- **ANPD Resolução 18/2024 §3.II** that the DPO be guaranteed direct communication with the highest hierarchical level of the controller.

**Reporting chain diagram (interim):**

```
HuGR Labs Board (currently: Gustavo Schneiter as sole director)
        │
        ├── DPO / Privacy Officer (interim: Gustavo Schneiter) ── direct, no filter
        │       │
        │       └── escalates to External Privacy Advisor (R5-8 pool) for countersign
        │
        └── Engineering / Operations chain
                │
                ├── Security Lead (interim: Gustavo Schneiter)
                ├── SRE Lead (interim: Gustavo Schneiter)
                └── Support Lead (interim: Gustavo Schneiter)
```

**Post-DPO-appointment chain (target Q3-2026):**

```
HuGR Labs Board (founder + external investor seats, post-Series-Seed)
        │
        ├── DPO (permanent hire, external candidate) ── direct, no filter
        │
        └── Engineering / Operations chain (CTO/CEO/Security Lead, distinct individuals)
```

---

## 3. Independence safeguards

Per **GDPR Art. 38(3)** and **LGPD Art. 41 §2º** interpretive guidance from ANPD Resolução 18/2024, the DPO must perform duties independently and not receive instructions on the exercise of those duties.

CoreLink's interim independence safeguards:

| Safeguard | Implementation | Evidence |
|---|---|---|
| **No operational role on production data pipelines** | Interim DPO does not author code in `crates/corelink-privacy-*` or `crates/corelink-dsr/` without external advisor countersign. Authoring of these crates by the same individual triggers a privacy-conflict flag in code review. | `specs/_compliance/DPO-RESPONSIBILITIES-MATRIX.md §3` (conflict-of-interest decision rules) |
| **No commercial KPI tied to data-volume growth that would conflict with minimization principle** | Interim DPO compensation is founder equity, not data-growth bonuses. | Founder's deed of incorporation (`legal/incorporation/`) |
| **Right to refuse or veto privacy-adverse processing** | DPO has unilateral veto on new `purpose_tag` enum additions, new sub-processor onboarding, and `legal_basis` changes. Veto recorded as `dpo_veto.v1` CloudEvent. | `LGPD-FULL-AUDIT-2026-05-15.md §1.3` (basis-fixed-per-purpose invariant); `RB-DPO-ESCALATION.md §3.2` |
| **Right to direct ANPD communication without controller pre-clearance** | DPO inbox (`privacy@hugr.com`) and personal contact may correspond directly with ANPD per `legal/breach-notification/anpd-contacts.md` without engineering or marketing pre-clearance. | `RB-DPO-ESCALATION.md §5` |
| **External-advisor pool countersign** (R5-8) | All material privacy decisions during interim window — DPIA approvals, breach declaration, sub-processor admission — receive countersign from one of 2 external privacy advisors. Mitigates segregation-of-duties deficit. | `LGPD-RESIDENCY-ATTESTATION-2026-05-15.md §7`; `compliance_matrix.md §9` row GAP-04 |
| **Cannot be dismissed for performing DPO duties** | Per GDPR Art. 38(3) and ANPD Resolução 18/2024 §3.III, removing the DPO due to a privacy decision is recorded as a breach of governance. Documented in founder's deed of incorporation §11 (privacy-governance clause). | `legal/incorporation/§11` (to be populated by H-11 law firm) |
| **Documentation independence** | This appointment document, the responsibilities matrix, the escalation runbook, and the handoff plan are owned by the DPO and may not be amended without DPO sign-off + external advisor countersign during the interim window. | `inherits_from:` chain in this doc's front-matter |

---

## 4. Conflict-of-interest analysis (ANPD Resolução 18/2024 §4 + GDPR WP29 guidance on DPO)

The interim DPO is the same individual as the founder, Security Lead, SRE Lead, and Support Lead. ANPD Resolução 18/2024 §4 permits accumulation of roles **so long as no conflict of interest exists**. The following analysis documents the residual risk and mitigations.

| Accumulated role | Conflict risk | Risk level | Mitigation |
|---|---|---|---|
| Founder (commercial leadership) | Commercial pressure to permit data-monetisation flows that minimization principle would disallow | M | DPO veto over `purpose_tag` enum (§3 above); annual founder attestation that no veto was overridden |
| Security Lead | Convergent (security and privacy share controls); not adverse | L | Documented as convergent in `LGPD-FULL-AUDIT-2026-05-15.md §1.14` (Art. 46-48 cross-framework with SOC 2 CC6.x) |
| SRE Lead | Tension: SRE may want exhaustive observability that minimization disallows | M | External advisor countersign on telemetry-purpose additions (see GAP-14 sub-processor backlog: Sentry/PostHog/LogRocket disabled for `sam` tenants by default) |
| Support Lead | Tension: support may want subject lookup conveniences that bypass DSR pipeline | L | Hard rule: all subject lookups MUST flow through DSR pipeline (`crates/corelink-dsr/`); ad-hoc lookups blocked by RBAC (`CTRL-AUTHZ-002`) |
| Compliance Lead / Final approver | Not adverse to DPO duties; convergent | L | Same individual signs as Compliance and as DPO during interim window; re-sign on permanent DPO appointment |

**Residual risk:** the same individual currently signs as DPO, Security Lead, and Compliance/Final approver on every attestation. Mitigation: external advisor pool (R5-8) provides countersign on material decisions. Documented in `LGPD-RESIDENCY-ATTESTATION-2026-05-15.md §7` and re-asserted here.

**Conflict-of-interest verdict (2026-05-15):** ACCEPTABLE for SAM-only GA launch. NOT acceptable for EU enterprise onboarding — must close GAP-01 before first EU enterprise contract.

---

## 5. Term + sunset

| Field | Value |
|---|---|
| **Interim term start** | 2026-05-15 |
| **Interim term hard cap** | Q3-2026 (T+1m of GA per `compliance_matrix.md §9` GAP-01) |
| **Trigger for early termination of interim** | (a) permanent DPO accepts offer + start date confirmed; OR (b) first EU enterprise tenant signs LOI; whichever earlier |
| **Sunset action** | Re-sign LGPD-RESIDENCY-ATTESTATION-2026-05-15, LGPD-FULL-AUDIT-2026-05-15, SOC2-EVIDENCE-ROLLUP-2026-05-15 with distinct DPO + Security Lead + Compliance signers. See `DPO-HANDOFF-PLAN.md`. |
| **Documentation refresh** | This appointment document superseded by a permanent-DPO appointment record (`DPO-APPOINTMENT-{YYYY-MM-DD}.md`) carrying `supersedes: DPO-APPOINTMENT-2026-05-15` in front-matter. |

---

## 6. Duties (LGPD Art. 41 §2º + GDPR Art. 39 cross-framework)

The full RACI for DPO-touched activities is in `DPO-RESPONSIBILITIES-MATRIX.md`. This section enumerates the Art. 41 §2º statutory minimums and the GDPR Art. 39 cross-framework additions.

### 6.1 LGPD Art. 41 §2º statutory minimums

| LGPD duty | CoreLink implementation | Cross-link |
|---|---|---|
| **§2º I** — Accept complaints and communications from subjects; provide explanations and adopt measures | Public inbox `privacy@hugr.com`; DSR self-service via `POST /v1/privacy/dsr/*`; 30-day wall-clock SLA per `RB-DSR-TICKET-TRIAGE.md` | `LGPD-FULL-AUDIT-2026-05-15.md §1.8` |
| **§2º II** — Receive communications from ANPD and adopt measures | `legal/breach-notification/anpd-contacts.md`; 48h internal SLA on inbound ANPD comms | `RB-DPO-ESCALATION.md §5` |
| **§2º III** — Orient employees and contractors about data-protection practices | Annual LGPD training (every engineer with prod access); monthly DPO checklist item 23 verifies completion | `LGPD-DPO-MONTHLY-CHECKLIST.md` item 23 |
| **§2º IV** — Execute other tasks defined by controller or norms | DPIA authoring, consent ledger oversight, sub-processor reviews, residency attestation refresh | `DPO-RESPONSIBILITIES-MATRIX.md` |

### 6.2 GDPR Art. 39 cross-framework additions (gating on EU enterprise)

| GDPR duty | CoreLink implementation | Cross-link |
|---|---|---|
| Art. 39(1)(a) — Inform and advise controller and processors of GDPR obligations | DPO authors quarterly compliance briefing to Board | `DPO-HANDOFF-PLAN.md §3` |
| Art. 39(1)(b) — Monitor compliance with GDPR and the controller's data-protection policies | Monthly checklist (25 items) + quarterly attestation refresh | `LGPD-DPO-MONTHLY-CHECKLIST.md` |
| Art. 39(1)(c) — Provide DPIA advice on request | `legal/dpia/template.md`; DPO authors or supervises authoring | `LGPD-FULL-AUDIT-2026-05-15.md §1.11` |
| Art. 39(1)(d) — Cooperate with supervisory authority | ANPD + EDPB liaison; documented contacts | `legal/breach-notification/anpd-contacts.md` |
| Art. 39(1)(e) — Act as contact point for supervisory authority on processing-related issues | Same public inbox + escalation tree | `RB-DPO-ESCALATION.md §5` |

---

## 7. ANPD registration template (LGPD Art. 41 §1º)

Per **ANPD Resolução nº 18/2024 §5**, the controller must register the DPO with ANPD via the agency's online form. This section is the filled template (data captured here; submission tracked separately in `legal/anpd-registration/`).

### 7.1 Controller identification

| Field (ANPD form label) | Value |
|---|---|
| Razão social / Nome empresarial | HuGR Labs Ltda. |
| CNPJ | _to be populated post H-1 incorporation_ |
| Endereço sede | _to be populated post H-1 incorporation_ |
| Setor de atuação | Tecnologia da informação — desenvolvimento de software (CNAE 6201-5/01) + serviços de computação em nuvem (CNAE 6311-9/00) |
| Porte (Receita Federal) | EPP (presumed at filing; reclassify to ME if revenue < R$ 360k/ano) |
| Site institucional | https://hugr.com |

### 7.2 DPO identification

| Field (ANPD form label) | Value |
|---|---|
| Nome completo do encarregado | Gustavo Schneiter |
| Vínculo com o controlador | Sócio fundador (sócio-administrador) e Privacy Officer interino |
| Tipo (PF / PJ / sócio / colaborador / terceiro) | Pessoa Física — sócio do controlador |
| Acumulação de funções | Sim — fundador + Privacy Officer interino (justificativa documentada em §4 deste documento; sem conflito de interesse material per ANPD Resolução 18/2024 §4) |
| E-mail público de contato | privacy@hugr.com |
| Telefone público de contato | _to be populated; recommended: dedicated landline_ |
| E-mail interno | gustavo@humangr.com |
| Data de início do mandato | 2026-05-15 |
| Tipo de mandato | Interino (até nomeação definitiva, alvo Q3-2026) |

### 7.3 Publication confirmation (Art. 41 §1º)

| Field (ANPD form label) | Value |
|---|---|
| URL de divulgação pública da identidade do encarregado | https://hugr.com/legal/privacy + https://docs.hugr.com/explanation/privacy/lgpd-full (mirrored in `apps/docs/docs/explanation/privacy/lgpd-full.mdx`) |
| URL do aviso de privacidade | https://hugr.com/legal/privacy-notice (pt-BR canonical: `legal/privacy-notice/v*.pt-BR.md`) |
| URL da página de canal do titular | https://hugr.com/privacy/dsr (mirrored in DSR self-service form WI-S16-004) |

### 7.4 Filing tracking

| Step | Owner | Target | Status |
|---|---|---|---|
| H-1 incorporation populates CNPJ / address | Founder + Stripe-Atlas counsel | R-2 day 0 | Pending H-1 |
| Filing submission to ANPD via online form | Interim DPO | D+5 of CNPJ assignment | Pending |
| Filing confirmation receipt archived | Interim DPO | D+30 of submission | Pending |
| Filing refresh on permanent DPO appointment | Permanent DPO | D+5 of permanent appointment | Pending — gates GAP-01 closure |

---

## 8. Cross-references

- **GAP-01 closure target:** `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md §5` (row GAP-01); `specs/03_architecture/compliance_matrix.md §9` (row GAP-01).
- **LGPD article-level audit:** `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md §1.12` (Art. 41).
- **Privacy model:** `specs/03_architecture/privacy_model.md` (LINDDUN + DSR pipeline + retention).
- **Privacy notice:** `legal/privacy-notice/v*.{pt-BR,en-US,es-MX}.md`.
- **DPA + residency amendment:** `legal/dpa/`, `legal/dpa-residency-amendment.md`.
- **Breach notification:** `legal/breach-notification/anpd-contacts.md` + `RB-DPO-ESCALATION.md §5`.
- **Customer-facing explainer:** `apps/docs/docs/explanation/privacy/lgpd-full.mdx`.
- **DSR runbook:** `specs/_runbooks/RB-DSR-LGPD-FULL.md`, `specs/_runbooks/RB-DSR-TICKET-TRIAGE.md`.
- **Responsibilities RACI:** `specs/_compliance/DPO-RESPONSIBILITIES-MATRIX.md`.
- **Escalation runbook:** `specs/_runbooks/RB-DPO-ESCALATION.md`.
- **Handoff plan:** `specs/_compliance/DPO-HANDOFF-PLAN.md`.
- **DPO monthly checklist:** `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md`.
- **ROADMAP-TO-GA §9 Human Track row H-15:** advisor pool (R5-8) that countersigns the interim DPO during the gap.

---

## 9. Sign-off

| Role | Name | Signature | Date |
|---|---|---|---|
| Controller (HuGR Labs Ltda. — sócio-administrador) | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| Interim Data Protection Officer (encarregado) | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |
| External Privacy Advisor (R5-8 countersign) | _to be populated post advisor onboarding_ | `__________________` | _2026-__-__ |
| Compliance / Final approver | _Gustavo Schneiter_ | `__________________` | _2026-05-__ |

> **Note on signer concentration:** same root cause as `LGPD-RESIDENCY-ATTESTATION-2026-05-15.md §7`. Re-sign on permanent DPO appointment per `DPO-HANDOFF-PLAN.md §5`.

---

**Fim de DPO-APPOINTMENT-2026-05-15.** This document supersedes any prior informal designation of "Privacy Officer (interim)" recurring in earlier compliance docs; from this date forward, all such references resolve to this appointment record.
