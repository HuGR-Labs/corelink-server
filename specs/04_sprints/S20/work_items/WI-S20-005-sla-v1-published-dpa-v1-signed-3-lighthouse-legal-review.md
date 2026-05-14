---
id: "WI-S20-005"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-05-14"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005", "FF-HR-009", "FF-HR-010"]
parent: "S-20"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "PRIVACY-MODEL"
  - "SLO-CATALOG"
  - "COMPLIANCE-MATRIX"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s20", "ga", "sla-v1", "dpa-v1", "legal-review", "lighthouse-signed", "high-risk"]
---

# WI-S20-005 — SLA Contractual v1 Published (Covering Cumulative SLOs Canonical SLO-AVAIL-CAS-PUT/GET ≥ 99.9% + SLO-LAT-CAS-GET p99 < 300ms + SLO-FRESH-DSR-ERASURE ≤ 30d + SLO-FRESH-BILLING ≤ 24h Reconciliation < 0.1% Drift) Em `legal/sla/v1.md` + `docs.corelink.dev/sla` + DPA v1 Finalization + Legal Review Externo (Cooley/DLA Piper/Bird & Bird ~$15-30k 6-Week Lead Reuso S-14 Path) + 3 Lighthouse Customers Signing (DPA + Sub-Processor Agreement + Breach Notification SLA + Pricing Addendum Se Enterprise) + ADR If Waiver Needed + EVT-044 Evidence

> **doc_status:** SEALED · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-20](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S20-005 |
| Título | SLA contractual v1 published + DPA v1 Legal externo reviewed + 3 lighthouse customers signing |
| Sprint | S-20 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (controle final pré-GA — SLA + DPA customer-facing legal contract) + FF-HR-009 (contratos com customers vão production = legal exposure permanente; SLA breach = legal challenge) + FF-HR-010 (primeira entrega regulatory live em escala) |

## 1. Objetivo + JTBD

**Objetivo**: Publicar **SLA contractual v1** em `legal/sla/v1.md` + `docs.corelink.dev/sla` covering cumulative SLOs canonical (SLO-AVAIL-CAS-PUT/GET ≥ 99.9% + SLO-LAT-CAS-GET p99 < 300ms + SLO-FRESH-DSR-ERASURE ≤ 30d + SLO-FRESH-BILLING ≤ 24h reconciliation < 0.1% drift) + finalizar **DPA v1** com Legal review externo (Cooley/DLA Piper/Bird & Bird ~$15-30k 6-week lead reuso S-14 path) + 3 lighthouse customers signing (DPA + sub-processor agreement + breach notification SLA + pricing addendum se enterprise) + ADR if waiver needed.

**JTBD**: "Como CISO em prospect enterprise / DPO compliance auditor / Legal Counsel customer-side, preciso evidência verificável que: (a) **SLA contractual v1 published** em `legal/sla/v1.md` + `docs.corelink.dev/sla` (NÃO advisory only; é contractual com remediation flow + service credits); (b) **DPA v1 Legal externo reviewed** por Cooley/DLA Piper/Bird & Bird (não in-house counsel; engagement plan ~$15-30k 6-week lead reuso S-14 path); (c) **3 lighthouse customers signed** DPA + sub-processor agreement + breach notification SLA + pricing addendum (se enterprise tier); (d) **ADR if waiver needed** documented + Legal-approved + customer-approved; (e) **EVT-044 evidence captured** em PRR-GA-001 evidence pack."

GA-go binary engineering gate.

## 2. Scope

### 2.1 In-scope

1. **SLA contractual v1** em `legal/sla/v1.md` + `docs.corelink.dev/sla`:
   - Coverage cumulative SLOs canonical (per SLO-CATALOG ratification):
     - SLO-AVAIL-CAS-PUT ≥ 99.9%.
     - SLO-AVAIL-CAS-GET ≥ 99.9%.
     - SLO-LAT-CAS-GET p99 < 300ms.
     - SLO-FRESH-DSR-ERASURE ≤ 30d.
     - SLO-FRESH-BILLING ≤ 24h reconciliation < 0.1% drift.
     - SLO-INCIDENT-RESPONSE-SYNTHETIC-PAGE p99 < 5 min (S-20 novo).
   - **Service credits** per SLO miss (e.g., 99.9% miss → 5% credit; 99.5% miss → 10% credit; 99% miss → 25% credit).
   - **Remediation flow**: customer raises ticket → SRE escalation → 5-Why post-mortem → service credits issued automatic.
   - **Exclusions**: customer-controlled CMK revocation (BYOK kill switch intentional per customer trigger; INV-BYOK-CRYPTO-SOVEREIGNTY) + region-specific outage (force majeure clause).
   - **Reporting**: monthly SLA report customer-facing; quarterly review cadence.
   - **Termination clause**: customer right to terminate after 3 consecutive months SLA miss.
   - **Legal review**: Cooley/DLA Piper/Bird & Bird ~$15-30k 6-week lead.

2. **DPA v1 finalization** em `legal/dpa/v1.md`:
   - **Foundation**: DPA template (Processor → Customer); GDPR Art. 28 + LGPD Art. 39 alignment.
   - **Schrems II TIA template** (S-14 deliverable cumulative; EDPB Recommendations 01/2020 supplementary measures).
   - **Sub-processor agreement**: Cloudflare + Stripe + Clerk + AWS KMS / GCP KMS / Azure Key Vault / HashiCorp Vault (per BYOK provider customer choice) + Drata/Vanta + PagerDuty.
   - **Breach notification SLA**: 72h notification customer per GDPR Art. 33 (Personal data breach to supervisory authority) + 24h internal escalation.
   - **Customer rights**: data portability + erasure (S-11 DSR cumulative) + access + rectification + objection.
   - **Cross-border transfers**: 4 regions WNAM/ENAM/WEUR/SAM enumerated com data localization promises (S-14 deliverable).
   - **Pricing addendum** se enterprise: BYOK setup wizard fee + custom support tier + SLA enhancement (additional 9s).
   - **Legal review**: Cooley/DLA Piper/Bird & Bird ~$15-30k 6-week lead reuso S-14 path.

3. **3 lighthouse customers signing**:
   - **Customer 1 (Forge team customer-zero)**: HuGR org self-signed DPA (internal precedent) + standard SLA.
   - **Customer 2 (OSS team)**: external OSS maintainer signed DPA + standard SLA (Legal review required per customer).
   - **Customer 3 (Enterprise BYOK)**: enterprise customer signed DPA + Schrems II TIA + sub-processor agreement + breach notification SLA + pricing addendum (BYOK setup wizard + custom support tier).
   - Customer signing tracked em `specs/_audits/2026-XX-XX-dpa-signed-3-lighthouse.md`.
   - **ADR if waiver needed** (e.g., customer requests pricing reduction OR DPA simplification): documented em `specs/_audits/2026-XX-XX-dpa-waiver-{customer}.md` + Legal-approved + customer-approved + CEO/Founder approval.

4. **EVT-044 evidence captured** em PRR-GA-001 evidence pack.

### 2.2 Anti-scope

- ❌ DPA template auto-signing flow (S-19 enterprise onboarding wizard cobre; S-20 entrega Legal-reviewed DPA + 3 lighthouse signing apenas).
- ❌ Multi-DPO escalation workflow (pós-GA enterprise).
- ❌ Customer-facing DPA dashboard UI (pós-GA enterprise).
- ❌ SLA enhancement tier (additional 9s) beyond enterprise (pós-GA enterprise demand-driven).
- ❌ Legal review domestic (Brazil) only (multi-jurisdictional review required: US + EU + Brazil).
- ❌ Pricing addendum for non-enterprise tiers (team tier standard SLA only).

## 3. Capability mapping

- **CAP-GA-005** (SLAs contratuais publicados + DPA v1 ready): IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.1 R-S20-5 + §6.1 + §15 risk row 5 (Legal review DPA atrasa)` + `slo_catalog.md (SLO-CATALOG ratification)` + `privacy_model.md (DPA + Schrems II TIA + LGPD Art. 39 + GDPR Art. 28)` + `compliance_matrix.md (SOC 2 + LGPD + GDPR + CCPA + EDPB SCCs)` + `WI-S14-008 DPA amendment Schrems II TIA template (cumulative deliverable)`.

## 4. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S20-005-D1 | SLA contractual v1 | `legal/sla/v1.md` + `docs.corelink.dev/sla` | covering cumulative SLOs canonical; service credits; remediation flow; exclusions; reporting; termination clause; Legal-reviewed |
| S20-005-D2 | DPA v1 finalization | `legal/dpa/v1.md` | Schrems II TIA + sub-processor agreement + breach notification SLA 72h + customer rights + cross-border transfers + pricing addendum enterprise; Legal externo reviewed Cooley/DLA Piper/Bird & Bird ~$15-30k 6-week lead |
| S20-005-D3 | 3 lighthouse customers signing | `specs/_audits/2026-XX-XX-dpa-signed-3-lighthouse.md` | DPA + sub-processor agreement + breach notification SLA signed por 3 customers; pricing addendum signed enterprise |
| S20-005-D4 | ADR if waiver needed | `specs/_audits/2026-XX-XX-dpa-waiver-{customer}.md` (if any) | Legal-approved + customer-approved + CEO/Founder approval; expiry + revalidation trigger |
| S20-005-D5 | EVT-044 evidence | PRR-GA-001 Annex A | DPA signed + SLA published linked |

## 5. Detailed design

### 5.1 SLA v1 structure

```markdown
# CoreLink Service Level Agreement v1

## 1. Definitions
- "Service": CoreLink multi-tenant content-addressable cache.
- "Tier": team | enterprise.
- "Region": WNAM | ENAM | WEUR | SAM.
- "Service Period": calendar month.

## 2. SLO targets
| SLO | Target | Tier coverage |
|---|---|---|
| SLO-AVAIL-CAS-PUT | ≥ 99.9% | team + enterprise |
| SLO-AVAIL-CAS-GET | ≥ 99.9% | team + enterprise |
| SLO-LAT-CAS-GET p99 | < 300ms | team + enterprise |
| SLO-FRESH-DSR-ERASURE | ≤ 30d | team + enterprise |
| SLO-FRESH-BILLING | < 0.1% drift 24h | team + enterprise |
| SLO-INCIDENT-RESPONSE-SYNTHETIC-PAGE p99 | < 5 min | enterprise (premium) |
| SLO-BYOK-KILL-SWITCH p99 | ≤ 5 min | enterprise (BYOK) |

## 3. Service credits
- 99.9% miss → 5% credit.
- 99.5% miss → 10% credit.
- 99% miss → 25% credit.
- < 99% (catastrophic) → 50% credit + termination right.

## 4. Remediation flow
1. Customer raises ticket via support@corelink.dev OR enterprise dedicated channel.
2. SRE escalation (PagerDuty 24/7 3 regions; per WI-S20-006).
3. 5-Why post-mortem published within 72h SEV-1 / 7d SEV-2.
4. Service credits issued automatic on monthly SLA report.

## 5. Exclusions
- Customer-controlled CMK revocation (BYOK kill switch intentional; INV-BYOK-CRYPTO-SOVEREIGNTY).
- Region-specific outage (force majeure clause; mitigated via PAT-REGION-FAILOVER-001).
- Maintenance windows (announced ≥ 7d prior; max 4h/quarter).

## 6. Reporting
- Monthly SLA report customer-facing (per-tenant dashboard).
- Quarterly review cadence (Customer Success engagement).

## 7. Termination clause
- Customer right to terminate after 3 consecutive months SLA miss; pro-rata refund.

## 8. Legal review
- Cooley/DLA Piper/Bird & Bird ~$15-30k 6-week lead reuso S-14 path.
- Multi-jurisdictional review: US + EU + Brazil.
```

### 5.2 DPA v1 structure

```markdown
# CoreLink Data Processing Agreement v1

## 1. Parties
- **Processor**: CoreLink (HuGR Labs).
- **Customer**: ...

## 2. Scope of processing
- Data categories: tenant data + audit chain + billing reconciliation + erasure attestation.
- Subject categories: customer end-users (B2B SaaS).

## 3. Sub-processor agreement
| Sub-processor | Purpose | Region |
|---|---|---|
| Cloudflare | Workers + R2 + D1 + DO + KV | global |
| Stripe | billing | US/EU |
| Clerk | auth + email verify | US |
| AWS KMS | BYOK customer choice | US/EU |
| GCP KMS | BYOK customer choice | US/EU |
| Azure Key Vault | BYOK customer choice | US/EU |
| HashiCorp Vault | BYOK customer choice (customer-hosted) | customer-controlled |
| Drata/Vanta | continuous compliance monitoring | US |
| PagerDuty | incident response | US/EU |

## 4. Breach notification SLA
- 72h notification to customer per GDPR Art. 33.
- 24h internal escalation.

## 5. Customer rights
- Data portability (export via S-15 CLI/SDK).
- Erasure (S-11 DSR cumulative; crypto-erase BYOK NIST SP 800-88 Rev.1 compliant).
- Access (per-tenant dashboard).
- Rectification (admin plane S-13).
- Objection (DSR opt-out flow).

## 6. Cross-border transfers
- 4 regions WNAM/ENAM/WEUR/SAM enumerated.
- Data localization promises per region (INV-DATA-RESIDENCY).
- Schrems II TIA template (EDPB Recommendations 01/2020 supplementary measures; S-14 deliverable cumulative).

## 7. Pricing addendum (enterprise tier only)
- BYOK setup wizard fee.
- Custom support tier.
- SLA enhancement (additional 9s; SLO-AVAIL ≥ 99.95%).

## 8. Legal review
- Cooley/DLA Piper/Bird & Bird ~$15-30k 6-week lead reuso S-14 path.
- Multi-jurisdictional review.
```

### 5.3 3 lighthouse customers signing tracker

```markdown
# DPA Signed 3 Lighthouse Customers

| Customer | DPA signed | Sub-processor agreement | Breach notification SLA | Pricing addendum | Status |
|---|---|---|---|---|---|
| Forge (team customer-zero) | HuGR org self-signed | n/a | 72h | n/a | signed |
| OSS (team external) | _pending_ | _pending_ | 72h | n/a | _pending_ |
| Enterprise BYOK | _pending_ | _pending_ | 72h | _pending_ | _pending_ |
```

## 6. Acceptance criteria

### 6.1 Positive paths

1. **SLA v1 published** em `legal/sla/v1.md` + `docs.corelink.dev/sla`.
2. **DPA v1 Legal externo reviewed** por Cooley/DLA Piper/Bird & Bird (multi-jurisdictional US + EU + Brazil).
3. **3 lighthouse customers signed** DPA + sub-processor agreement + breach notification SLA.
4. **Pricing addendum signed enterprise** (Customer 3 BYOK).
5. **ADR if waiver needed** documented + Legal-approved + customer-approved + CEO/Founder approval.
6. **EVT-044 evidence captured** em PRR-GA-001 Annex A.

### 6.2 Negative paths (≥ 4 mandatory)

1. **Legal review DPA atrasa** (per spec contract §15 row 5) → engage Legal externo Q3 antes do sprint start; iterate during sprint; fallback DPA v1 simplified com ADR.
2. **Customer DPA legal challenge** → post-mortem com Legal + customer trust review; iteration cycle.
3. **3 lighthouse customers signing fail** (1 customer não signs) → engage backup candidate (per WI-S20-004 4-5 LOI mitigation); per spec contract §19 waiver: 3 → 2 lighthouse com plan to add 1 within 60d pós-GA + ADR.
4. **Multi-jurisdictional review iteration cycles** → buffer 6-week lead Q3 antes do sprint start; budget contingency $30-60k full Legal review.
5. **Pricing addendum não approved enterprise** → fallback alternative ICP candidate (per WI-S20-004 mitigation).
6. **DPA simplification waiver** (e.g., Schrems II TIA não-applicable per customer profile) → ADR + Legal-approved + expiry + revalidation trigger.

## 7. Test plan

### 7.1 Legal review validation

- Cooley/DLA Piper/Bird & Bird engaged Q3 (6-week lead).
- Multi-jurisdictional review (US + EU + Brazil).
- DPA v1 + SLA v1 + sub-processor agreement Legal-approved.

### 7.2 Customer signing validation

- 3 lighthouse customers signed DPA + sub-processor agreement + breach notification SLA.
- Pricing addendum signed enterprise (Customer 3 BYOK).
- Customer signing tracker complete.

### 7.3 Integration validation

- SLA v1 + DPA v1 referenced em customer onboarding flow (S-19 DPA click-through 6-field consent + JWT receipt).
- Schrems II TIA template applicable enterprise tier (S-14 cumulative).

## 8. Failure modes

- **FM-LEGAL-REVIEW-DPA-ATRASA** (per spec contract §15 row 5): mitigation = engage Legal externo Q3 antes do sprint start + iterate during sprint + fallback DPA v1 simplified com ADR.
- **FM-DPA-LEGAL-CHALLENGE** (cumulative S-19; FM-X-DPA-LEGAL-CHALLENGE): mitigation = Legal review iteration + customer trust review.
- **FM-CUSTOMER-SIGNING-FAIL** (novo S-20): mitigation = engage 4-5 candidates LOI Q3 + backup; per spec contract §19 waiver 3 → 2 lighthouse + plan to add 1 within 60d pós-GA.
- **FM-PRICING-ADDENDUM-NOT-APPROVED-ENTERPRISE** (novo S-20): mitigation = fallback alternative ICP.

## 9. Invariants (cumulative ratification)

ALL 27+ INVs from S-13..S-19 + canonical sources active; SLA + DPA validation covers:

- **CRITICAL**: INV-CONSENT-PROOF-VERIFIABLE (DPA verify endpoint S-11 cumulative) + INV-DATA-RESIDENCY (4 regions enumerated em DPA cross-border transfers).
- **HIGH**: INV-DATA-ERASURE-COMPLETE (DSR rights em DPA customer rights) + INV-ERASURE-ATTESTATION-SIGNED (BYOK enterprise crypto-erase) + INV-ONBOARD-DPA-FIRST (DPA-first per S-19 cumulative) + INV-ONBOARD-ATOMIC-PROVISIONING.

S-20 NÃO introduz novos INVs.

## 10. Controls (cumulative)

CTRL-XXX cumulative validated em SLA + DPA:
- **GDPR Art. 28** (Processor obligations).
- **LGPD Art. 39** (Operator).
- **GDPR Art. 32** (security of processing).
- **GDPR Art. 33** (breach notification).
- **GDPR Art. 17** (erasure).
- **GDPR Art. 46** (international transfers).
- **CCPA §1798.140(v)** (sale/disclosure).
- **EDPB SCCs** (Schrems II supplementary measures).
- SOC 2 (CC6.1, CC6.7, CC8.1) cumulative.

## 11. Resilience patterns (cumulative)

PAT-XXX cumulative referenced em SLA exclusions + remediation flow:
- PAT-REGION-FAILOVER-001 (region outage exclusion via failover transparent).
- PAT-AUTO-ROLLBACK-001 (error budget burn → service credits).
- PAT-DUAL-APPROVAL-001 (admin operations).

## 12. Observability (cumulative)

Métricas Prometheus snake_case underscored:

- `corelink_dpa_signed_count_gauge{customer_tier, plan}` (gauge; lighthouse customer tracking; cardinality safe — apenas 3 customers).
- `corelink_sla_credit_issued_total{customer_id, slo_id, plan}` (counter; alert > 0).
- `corelink_dpa_legal_review_status_gauge{phase, plan}` (gauge; phase ∈ engaged|review|approved|signed).

DASH-LEGAL-S20 panel embedded em DASH-GA-READINESS dashboard.

## 13. Security & Privacy

**STRIDE delta**: SLA + DPA evidence-grade legal contract; Legal review externo prevents customer challenges; multi-jurisdictional coverage.

**LINDDUN delta**:
- **Linkability**: customer_id em DPA tracker apenas 3 customers cardinality safe.
- **Identifiability**: customer signing identification per Legal review.
- **Non-repudiation**: DPA signed customer + Legal-approved = forensic-grade evidence.
- **Detectability**: SLA credit alerts + breach notification SLA 72h.
- **Disclosure**: DPA + SLA contractual customer-facing legal contract (intentional disclosure transparency per EDPB Recommendations 01/2020).
- **Unawareness**: customer notified per breach notification SLA per DPA v1 + monthly SLA report.
- **Non-compliance**: SOC 2 + LGPD + GDPR + CCPA + EDPB SCCs satisfied via SLA + DPA + Legal review externo.

## 14. Dependencies

### Hard blockers
- Legal externo engaged Q3 (Cooley/DLA Piper/Bird & Bird ~$15-30k 6-week lead reuso S-14 path).
- WI-S14 DPA amendment Schrems II TIA template cumulative deliverable available.
- WI-S20-004 3 lighthouse customers LOI signed Q3.

### Soft blockers
- Cumulative S-13..S-19 SEALED.

### Outbound
- WI-S20-001 PRR-GA-001 evidence pack EVT-044 depende de WI-S20-005 DPA signed + SLA published.
- WI-S20-004 customer attestation requires DPA signed.

## 15. PERT estimate

| Estimate | Hours | Notes |
|---|---|---|
| **Optimistic (O)** | 12h | Legal review smooth + 3 customer signing fast. |
| **Most Likely (M)** | 18h | Legal review iteration + 3 customer signing + pricing addendum enterprise. |
| **Pessimistic (P)** | 30h | Legal review extended cycles + customer challenge + ADR waiver iteration. |
| **PERT** | (12 + 4×18 + 30) / 6 = **19.0h** | Per spec contract §12. |
| **Variance (σ²)** | ((30-12)/6)² = 9.0 | Std dev ≈ 3.0h. |

## 16. Sign-off canonical (HIGH_RISK 13)

13 roles per spec contract §5.1 R-S20-1 (v1.4.0 canonical — Lote 11.21 round-1 P0-S20-001 fix). **Authoritative roster (see `_spec_contract.md §5.1` for full text):** (1) Owner, (2) Final Approver, (3) Engineer Lead, (4) QA Lead, (5) Security Lead (AppSec advisor folded), (6) Privacy Officer (DPO-interim; DPO folded), (7) Legal Counsel, (8) Compliance Officer, (9) Product Lead, (10) SRE Lead, (11) CTO, (12) Architect (Crypto SME folded per ADR-0034), (13) External Auditor (pentest firm rep). Finance is NOT in the canonical 13. Any role label below that diverges MUST be read as referring to its folded canonical slot per the mapping above.

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect (Crypto SME folded) | _TBD_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD_ | _pending_ | _pending_ |
| 5 | SRE Lead (SLA service credits + remediation flow) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (S-20 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product (SLA + DPA customer-facing UX) | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy Officer (DPA + Schrems II TIA + LGPD + GDPR + CCPA cumulative) | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD_ | _pending_ | _pending_ |
| 12 | Legal Counsel (DPA v1 + SLA v1 + sub-processor agreement + breach notification SLA review primary) | _TBD via Cooley/DLA Piper/Bird & Bird ~$15-30k 6-week lead reuso S-14 path_ | _pending_ | _pending_ |
| 13 | Finance (pricing addendum enterprise + service credits financial impact) | _TBD_ | _pending_ | _pending_ |

## 17. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S20-005 (cycle 12.S20.0; SLA contractual v1 published + DPA v1 Legal externo reviewed Cooley/DLA Piper/Bird & Bird ~$15-30k 6-week lead reuso S-14 path + 3 lighthouse customers signing + ADR if waiver needed; coverage SLO-CATALOG cumulative + GDPR Art. 28 + LGPD Art. 39 + CCPA + EDPB SCCs). |
| 1.0.0 | 2026-05-14 | Gustavo (via Sonnet builder) | SEALED — entregáveis: `legal/sla/v1.0.0.md` + `legal/sla/CHANGELOG.md` + `legal/dpa/v1.0.0.{en-US,pt-BR,es-419}.md` expandidos para Art. 28 GDPR + LGPD compliance + `legal/dpa/STANDARD-CONTRACTUAL-CLAUSES-EU.md` (Module 2 ref) + `legal/dpa/SUB-PROCESSOR-COMMITMENTS.md` (Cloudflare/Clerk/Stripe/Neon DPAs) + `specs/_legal/lighthouse-legal-review-tracker.md` (5-gate per customer; 3 lighthouse) + `.github/workflows/legal-changes-review.yml` SHA-pinned + CODEOWNERS expansão `/legal/**` + `/specs/_legal/`. Frontmatter SEALED/DONE. |

---

**Fim WI-S20-005.**
