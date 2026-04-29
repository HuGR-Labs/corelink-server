---
id: "WI-S20-004"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
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
  - "SLO-CATALOG"
  - "PRIVACY-MODEL"
  - "AUTH-MODEL"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s20", "ga", "lighthouse-customers", "forge", "oss", "enterprise-byok", "30d-observation", "high-risk"]
---

# WI-S20-004 — 3 Lighthouse Customers Migration (2 Team Tier — Forge Customer-Zero HuGR Org Primeira Tenant Ativada + 1 External OSS Bazel/Buck2 Ecosystem Reach via OSS Maintainer Outreach + 1 Enterprise BYOK + DPA TBD via Sales Engagement ICP Enterprise Pre-Revenue Q3) + Migration Plan Per-Customer (Signup + DPA Signing + Tier Selection + First PAT + First CAS PUT + 30d Observation Period) + 30d Observation Post-Migration + SLA Claim Met em 30d Sustained (SLO-LAT-CAS-GET p99 < 300ms + SLO-AVAIL-CAS-PUT/GET ≥ 99.9% + SLO-FRESH-DSR-ERASURE ≤ 30d + SLO-FRESH-BILLING ≤ 24h Reconciliation < 0.1% Drift) + Customer Attestations Testimonial + Case Study Marketing Co-Led com Customer Success + Customer Attestation Reports + EVT-018 Evidence

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-20](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S20-004 |
| Título | 3 lighthouse customers migration + 30d observation post-migration + SLA claim met sustained 30d + customer attestations testimonial + case study |
| Sprint | S-20 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (controle final pré-GA — customer trust baseline) + FF-HR-009 (contratos com customers vão production = legal exposure permanente; SLA breach = customer trust loss) + FF-HR-010 (primeira entrega regulatory live em escala) |

## 1. Objetivo + JTBD

**Objetivo**: Migrar **3 lighthouse customers** (2 team tier — Forge customer-zero HuGR org primeira tenant ativada + 1 external OSS Bazel/Buck2 ecosystem; **1 enterprise** com BYOK + DPA TBD via Sales engagement ICP enterprise pre-revenue Q3) com migration plan per-customer + 30d observation post-migration + SLA claim met em 30d sustained + customer attestations testimonial + case study Marketing co-led com Customer Success.

**JTBD**: "Como CISO em prospect enterprise / DPO compliance auditor / Engineer evaluating CoreLink for Bazel/Buck2 RBE backend, preciso evidência verificável que: (a) **CoreLink GA é production-grade** com 3 lighthouse customers já migrated successfully (NÃO synthetic; real-world validation); (b) **2 team tier** — Forge (customer-zero per spec contract §5.1; HuGR org primeira tenant ativada) + 1 external OSS Bazel/Buck2 ecosystem (validation OSS ecosystem fit per REMOTE-CACHE-PRODUCT-PROFILE); (c) **1 enterprise** com BYOK + DPA TBD via Sales engagement ICP enterprise pre-revenue Q3 (validation enterprise tier complete S-14 deliverables); (d) **migration plan per-customer** (signup + DPA signing + tier selection + first PAT + first CAS PUT + 30d observation period); (e) **30d observation post-migration + SLA claim met em 30d sustained** (SLO-LAT-CAS-GET p99 < 300ms + SLO-AVAIL-CAS-PUT/GET ≥ 99.9% + SLO-FRESH-DSR-ERASURE ≤ 30d + SLO-FRESH-BILLING ≤ 24h reconciliation < 0.1% drift); (f) **customer attestations** testimonial + case study Marketing co-led com Customer Success; (g) **GA Evidence Gate D+60 criterion** — 3 lighthouse customers SLA claim met sustained 30d obrigatório (per spec contract §10.s20.7)."

GA-go binary engineering gate.

## 2. Scope

### 2.1 In-scope

1. **Engagement 4-5 candidates pré-sprint** (Q3 antes do sprint start):
   - Signed letter of intent (LOI) Q3 antes de sprint start (per spec contract §15 risk row 2 mitigation; alternative lined up se desistência mid-sprint).
   - Finalize 3 lighthouse customers Q3-Q4.
   - Backup candidates 2 (alternative lined up).

2. **3 lighthouse customers profile**:
   - **Customer 1 (Team tier — customer-zero)**: **Forge** (HuGR org primeira tenant ativada; per spec contract §5.1; product team lighthouse customer self-served).
     - Migration: signup → DPA → tier=team → first PAT → first CAS PUT → 30d observation.
     - SLA claim: SLO-LAT-CAS-GET p99 < 300ms + SLO-AVAIL-CAS-PUT/GET ≥ 99.9%.
     - Attestation: internal testimonial + case study (HuGR org self-attestation).
   - **Customer 2 (Team tier — external OSS)**: 1 external OSS project Bazel/Buck2 ecosystem (reach via OSS maintainer outreach Q3-Q4; engagement via GitHub OSS maintainer email + community Discord/Slack).
     - Profile: open source maintainer using Bazel/Buck2 RBE; pre-CoreLink baseline = AWS S3 + bazel-remote-cache OR Buck2 cas client; CoreLink upgrade = managed service + multi-region failover.
     - Migration: signup → DPA → tier=team → first PAT → first CAS PUT → 30d observation.
     - SLA claim: SLO-LAT-CAS-GET p99 < 300ms + SLO-AVAIL-CAS-PUT/GET ≥ 99.9%.
     - Attestation: external testimonial (OSS maintainer endorsement) + case study (CAP-LAUNCH-001 marketing material).
   - **Customer 3 (Enterprise BYOK)**: 1 enterprise com BYOK + DPA TBD via Sales engagement ICP enterprise pre-revenue Q3.
     - Profile: ICP enterprise (FedRAMP-ready / EU customer / financial services); engagement via Sales founder outreach Q3 (10-20 ICP candidates pre-revenue funnel).
     - Migration: signup → DPA amendment Schrems II TIA (S-14 deliverable) → tier=enterprise + BYOK setup wizard (S-16 admin UI) → AWS KMS provider primary → first PAT → first CAS PUT → 30d observation.
     - SLA claim: enterprise tier SLA (per WI-S20-005); BYOK kill switch ≤ 5 min sustained (S-14); BYOK matrix test 4 providers verde (S-14); erasure attestation Ed25519 verifiable (S-14).
     - Attestation: external testimonial (NDA + sanitized case study sharable em sales conversations).

3. **Migration plan per-customer** (`docs/internal/lighthouse-migration-{forge,oss,enterprise}.md`):
   - Pre-migration: customer onboarding Q3-Q4 (LOI + scoping call + technical fit assessment + DPA review).
   - Migration day: signup → DPA signing → tier selection → first PAT → first CAS PUT (S-19 onboarding business logic flow).
   - 30d observation post-migration: customer usage instrumentation + SLA claim tracking + bug reports + iteration.
   - Customer Success engagement: weekly check-ins + bug triage + UX iteration + testimonial collection.

4. **30d observation post-migration** (per spec contract §6.1 + §10.s20.7):
   - **SLA claim met em 30d sustained** (SLO-LAT-CAS-GET p99 < 300ms + SLO-AVAIL-CAS-PUT/GET ≥ 99.9% + SLO-FRESH-DSR-ERASURE ≤ 30d + SLO-FRESH-BILLING ≤ 24h reconciliation < 0.1% drift).
   - **GA Evidence Gate D+60 criterion** — 3/3 lighthouse customers SLA claim met sustained 30d obrigatório.
   - **Per-customer dashboard panel** em DASH-LIGHTHOUSE-CUSTOMERS (DASH-GA-READINESS embedded).

5. **Customer attestations testimonial + case study**:
   - **Testimonial**: short quote (1-2 paragraphs) endorsing CoreLink quality + production-grade posture; signed por customer technical lead OR CTO.
   - **Case study**: long-form narrative (3-5 pages) covering pre-CoreLink baseline + migration experience + 30d observation results + SLA claim met evidence + future plans.
   - **Marketing co-led com Customer Success**: drafted Q3-Q4 + iterated post-30d observation + Legal-reviewed (per WI-S20-005 DPA + sub-processor agreement) + customer-approved.
   - **Customer attestation reports**: `specs/_audits/2026-XX-XX-lighthouse-customer-{forge,oss,enterprise}-attestation.md` (sanitized + sharable em sales sob NDA).

6. **EVT-018 evidence captured** em PRR-GA-001 evidence pack.

### 2.2 Anti-scope

- ❌ Pre-GA paying customers além de 3 lighthouse — closed beta only at S-20; expansion post-GA.
- ❌ APAC region lighthouse customer — pós-GA Q1 demand-driven (4 regions WNAM/ENAM/WEUR/SAM stable from S-14 sufficient at GA).
- ❌ Customer-facing onboarding analytics dashboard — pós-GA enterprise.
- ❌ Customer expansion beyond 3 lighthouse during S-20 (focus depth não width).
- ❌ Non-attested case study without customer approval (mandatory customer-approved per Legal review).

## 3. Capability mapping

- **CAP-GA-004** (3 lighthouse customers migrated + attestations): IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.1 R-S20-4 + §6.1 + §10.s20.7 + §15 risk row 2 + §19 waiver policy (3 → 2 lighthouse com plan to add 1 within 60d pós-GA waivable apenas)` + `slo_catalog.md (SLO-AVAIL-* + SLO-LAT-* + SLO-FRESH-*)` + `auth_model.md (signup + first PAT)` + `privacy_model.md (DPA signing)` + `01_vision/remote_cache_product_profile.md (REMOTE-CACHE-PRODUCT-PROFILE Bazel/Buck2/RBE alignment)`.

## 4. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S20-004-D1 | 4-5 candidates LOI engagement Q3 | `specs/_audits/2026-XX-XX-lighthouse-candidates-loi.md` | LOI signed Q3; 3 finalized + 2 backup |
| S20-004-D2 | Migration plan per-customer | `docs/internal/lighthouse-migration-{forge,oss,enterprise}.md` | per-customer plan; pre-migration + migration day + 30d observation + Customer Success engagement |
| S20-004-D3 | 3 lighthouse customers migrated | per-customer migration tracker | signup + DPA + tier + first PAT + first CAS PUT executed; per-customer dashboard panel live |
| S20-004-D4 | 30d observation post-migration | DASH-LIGHTHOUSE-CUSTOMERS panel | SLA claim met em 30d sustained; per-customer panel live em DASH-GA-READINESS embedded; *(GA Evidence Gate D+60)* |
| S20-004-D5 | Customer attestations | `specs/_audits/2026-XX-XX-lighthouse-customer-{forge,oss,enterprise}-attestation.md` | testimonial + case study; Marketing co-led com Customer Success; Legal-reviewed; customer-approved; sanitized sharable sob NDA |
| S20-004-D6 | EVT-018 evidence | PRR-GA-001 Annex A | per-customer attestation linked + 30d observation metrics linked |

## 5. Detailed design

### 5.1 Per-customer migration timeline

| Customer | Pre-migration Q3-Q4 | Migration Day | 30d observation | Attestation |
|---|---|---|---|---|
| Forge (team customer-zero) | LOI signed Q3 (HuGR org self-attestation) | D+0..D+5 (sprint kick-off; Forge product team self-served) | D+5..D+35 | D+35 internal testimonial + case study draft |
| OSS (team external) | LOI signed Q3 (OSS maintainer outreach) | D+5..D+10 | D+10..D+40 | D+40 external testimonial + CAP-LAUNCH-001 case study |
| Enterprise BYOK (TBD ICP) | LOI signed Q3 (Sales founder outreach 10-20 ICP candidates) | D+10..D+15 | D+15..D+45 | D+45 external testimonial NDA + sanitized case study |

### 5.2 SLA claim met sustained 30d

Per-customer dashboard panel em DASH-LIGHTHOUSE-CUSTOMERS (DASH-GA-READINESS embedded):

| SLO | Target | Customer 1 (Forge) | Customer 2 (OSS) | Customer 3 (Enterprise BYOK) |
|---|---|---|---|---|
| SLO-AVAIL-CAS-PUT | ≥ 99.9% | sustained 30d | sustained 30d | sustained 30d |
| SLO-AVAIL-CAS-GET | ≥ 99.9% | sustained 30d | sustained 30d | sustained 30d |
| SLO-LAT-CAS-GET p99 | < 300ms | sustained 30d | sustained 30d | sustained 30d |
| SLO-FRESH-DSR-ERASURE | ≤ 30d | (n/a sample period) | (n/a sample period) | sustained se DSR triggered |
| SLO-FRESH-BILLING | < 0.1% drift 24h | sustained 30d | sustained 30d | sustained 30d |
| SLO-BYOK-KILL-SWITCH (S-14) | ≤ 5 min p99 | n/a (team tier) | n/a (team tier) | sustained chaos drill weekly |

### 5.3 Customer attestation template

```markdown
# Customer Attestation — {Forge|OSS|Enterprise BYOK}

## Customer profile
- **Name**: ...
- **Industry**: ...
- **Use case**: Bazel/Buck2 RBE backend / shared cache / ML model checkpoint
- **Pre-CoreLink baseline**: ...
- **Migration date**: D+...

## Migration experience
- (3-5 paragraphs covering signup + DPA + tier + first PAT + first CAS PUT)

## 30d observation results
- SLA claim met: yes
- SLO-AVAIL-CAS-PUT: ≥ 99.9% sustained
- SLO-LAT-CAS-GET p99: < 300ms sustained
- (per-SLO evidence)

## Testimonial
> "..." — Customer Technical Lead / CTO

## Future plans
- (1-2 paragraphs)

## Attestation date
- D+35 / D+40 / D+45

## Signed
- Customer signatory: Name, Role
```

## 6. Acceptance criteria

### 6.1 Positive paths

1. **3 lighthouse customers LOI signed Q3** + 2 backup.
2. **Migration plan per-customer committed** em `docs/internal/lighthouse-migration-{forge,oss,enterprise}.md`.
3. **3 lighthouse customers migrated successfully** (signup + DPA + tier + first PAT + first CAS PUT).
4. **30d observation post-migration**: 3/3 lighthouse customers SLA claim met sustained 30d.
5. **Customer attestations** testimonial + case study Marketing co-led + Legal-reviewed + customer-approved.
6. **EVT-018 evidence captured** em PRR-GA-001 Annex A.
7. **GA Evidence Gate D+60 criterion** met: 3/3 lighthouse customers SLA claim met sustained 30d.

### 6.2 Negative paths (≥ 4 mandatory)

1. **Lighthouse customer desiste mid-sprint** → engage backup candidate (4-5 LOI signed Q3 mitigation); 1 of 2 backup picks up; OR per spec contract §19 waiver: 3 → 2 lighthouse customers se 1 desiste com plan to add 1 within 60d pós-GA + ADR.
2. **Lighthouse customer SLA claim miss em 30d** → SRE escalation + remediation flow + iteration cycle; if sustained miss → SEAL gate blocked + post-mortem + 5-Why.
3. **Customer attestation rejected** (Legal review or customer-approved fail) → iterate; budget multiple revision cycles.
4. **OSS maintainer outreach falha** (no engagement) → backup OSS candidate (engagement Q3-Q4 multiple outreach) OR pivot ao 2nd team customer-zero per ADR-0034 solo-tier waiver.
5. **Enterprise BYOK ICP candidate falha** (DPA not signed; pricing not approved) → fallback alternative ICP candidate; budget timeline 4-week additional Sales engagement.
6. **Marketing case study não Legal-approved em D+45** → Legal review iteration; case study published post-GA Q1.
7. **Per spec contract §19 waiver: 3 → 2 lighthouse waivable** apenas com plan to add 1 within 60d pós-GA + ADR + CEO/Founder approval.

## 7. Test plan

### 7.1 Pre-migration validation

- LOI signed Q3 per-customer.
- DPA review per-customer (enterprise tier requires Schrems II TIA per S-14 deliverable).
- Technical fit assessment (Bazel/Buck2 ecosystem fit OSS; FedRAMP-ready / EU / financial services enterprise BYOK).

### 7.2 Migration day validation

- Per-customer signup → DPA → tier → first PAT → first CAS PUT executed successfully.
- Customer onboarding metrics em DASH-ONBOARDING (S-19) tracked.
- INV-ONBOARD-DPA-FIRST + INV-ONBOARD-ATOMIC-PROVISIONING ratificadas.

### 7.3 30d observation validation

- Per-customer dashboard panel live em DASH-LIGHTHOUSE-CUSTOMERS.
- SLA claim tracking continuous; alert > 0 sustained miss.
- Customer Success weekly check-ins + bug triage.
- 3/3 SLA claim met sustained 30d obrigatório (GA Evidence Gate D+60).

## 8. Failure modes

- **FM-LIGHTHOUSE-CUSTOMER-DESISTS-MID-SPRINT** (per spec contract §15 row 2): mitigation = engage 4-5 candidates LOI signed Q3 + backup; alternative lined up.
- **FM-LIGHTHOUSE-SLA-CLAIM-MISS-30D** (per spec contract §15 row 7): mitigation = SLO targets achievable + 30d monitoring + SRE escalation + remediation flow.
- **FM-OSS-MAINTAINER-OUTREACH-FAIL** (novo S-20): mitigation = backup OSS candidate + multiple outreach Q3-Q4.
- **FM-ENTERPRISE-BYOK-ICP-FAIL** (novo S-20): mitigation = fallback alternative ICP + 4-week additional Sales engagement.
- Cumulative coverage: FMs S-13..S-19 ratificadas em 30d observation clean.

## 9. Invariants (cumulative ratification)

ALL 27+ INVs from S-13..S-19 + canonical sources active during 30d observation:

- **CRITICAL** validated em lighthouse customer migration: INV-TENANT-ISOLATION (multi-tenant correctness) + INV-CAS-INTEGRITY (CAS PUT/GET) + INV-AUDIT-APPEND-ONLY (audit chain) + INV-BILLING-NO-LOSS/NO-DUP (Stripe reconciliation) + INV-BYOK-CRYPTO-SOVEREIGNTY (enterprise BYOK; chaos drill weekly) + INV-CONSENT-PROOF-VERIFIABLE (DPA signing) + INV-REGION-NO-CROSS-LEAK (multi-region routing).
- **HIGH** validated em lighthouse customer migration: INV-DATA-RESIDENCY (residency enforcement) + INV-DATA-ERASURE-COMPLETE (DSR if triggered) + INV-OBS-CARDINALITY-BUDGET (per-customer panel cardinality safe — apenas 3 customers excepcional canonical limited per sprint.md §11) + INV-DEDUP-CONSISTENCY (CAS dedup) + INV-RATE-LIMIT-PROPORTIONALITY + INV-BILLING-RECONCILE-3-LAYER + INV-ERASURE-ATTESTATION-SIGNED (enterprise BYOK if DSR triggered) + INV-ADMIN-DUAL-APPROVAL (admin operations) + INV-ONBOARD-DPA-FIRST + INV-ONBOARD-ATOMIC-PROVISIONING.

S-20 NÃO introduz novos INVs.

## 10. Controls (cumulative)

CTRL-XXX cumulative validated em lighthouse customer migration:
- CTRL-CRYPTO-005 (BYOK enterprise) + CTRL-KEY-010..015 (envelope encryption + Ed25519 attestation).
- CTRL-AUDIT-001 + CTRL-AUDIT-002 + CTRL-AUDIT-003 + CTRL-AUDIT-005 (audit chain).
- CTRL-AUTH-010 (MFA UV=1).
- CTRL-PRIV-001..031 (consent + residency + erasure + DPA).
- SOC 2 + LGPD + GDPR + CCPA + EDPB SCCs cumulative.

## 11. Resilience patterns (cumulative)

PAT-XXX cumulative tested em 30d observation:
- PAT-REGION-FAILOVER-001 (region outage chaos test impacts customer SLA).
- PAT-ROLL-FORWARD-001 (secret rotation in-flight).
- PAT-AUTO-ROLLBACK-001 (error budget burn).
- PAT-DUAL-APPROVAL-001 (admin operations).
- PAT-SAGA-ATOMIC-001 (enterprise handoff).

## 12. Observability (cumulative)

Métricas Prometheus snake_case underscored (label `plan` aplicável; cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado):

- `corelink_lighthouse_customer_sla_claim_met_total{customer_id, plan}` (counter; per-customer panel; cardinality safe — apenas 3 customers excepcional canonical limited).
- `corelink_lighthouse_customer_migration_duration_days_bucket{customer_id, plan}` (histogram).
- `corelink_lighthouse_customer_observation_status_gauge{customer_id, plan}` (gauge; 0=blocked|1=in_progress|2=sla_met|3=attestation_signed).
- `corelink_lighthouse_customer_slo_violation_total{customer_id, slo_id, plan}` (counter; alert > 0 sustained 7d).

DASH-LIGHTHOUSE-CUSTOMERS panel embedded em DASH-GA-READINESS dashboard.

SLO-CATALOG cumulative validated:
- SLO-AVAIL-CAS-PUT ≥ 99.9% sustained 30d per-customer.
- SLO-AVAIL-CAS-GET ≥ 99.9% sustained 30d per-customer.
- SLO-LAT-CAS-GET p99 < 300ms sustained 30d per-customer.
- SLO-FRESH-DSR-ERASURE ≤ 30d (se DSR triggered enterprise BYOK).
- SLO-FRESH-BILLING < 0.1% drift 24h reconciliation sustained 30d per-customer.

## 13. Security & Privacy

**STRIDE delta**: cumulative validation across S-13..S-19; lighthouse customer migration validates multi-tenant isolation + BYOK enterprise tier holistically.

**LINDDUN delta**:
- **Linkability**: tenant_id em audit accountability (compliance baseline); customer_id em DASH-LIGHTHOUSE-CUSTOMERS apenas 3 customers cardinality safe excepcional canonical limited.
- **Identifiability**: customer pseudonymization via tenant_id_hash em audit chain.
- **Non-repudiation**: customer attestation signed por technical lead OR CTO + 30d observation evidence pack.
- **Detectability**: SLA violation alerts; SLO targets monitored continuously.
- **Disclosure**: customer attestation sanitized sharable sob NDA.
- **Unawareness**: customer notified per breach notification SLA per DPA v1.
- **Non-compliance**: SOC 2 + LGPD + GDPR + CCPA + EDPB SCCs satisfied via cumulative + lighthouse customer evidence pack.

## 14. Dependencies

### Hard blockers
- All 14 canonical sources active.
- Cumulative S-13..S-19 SEALED (signup + DPA + tier + BYOK + first PAT + audit chain + region + billing).
- 4-5 candidates LOI signed Q3.
- Sales engagement ICP enterprise pre-revenue Q3.
- OSS maintainer outreach Q3-Q4.

### Soft blockers
- WI-S20-005 SLA + DPA finalization (DPA v1 signed 3 lighthouse).
- WI-S20-006 incident response 24/7 (oncall during 30d observation).

### Outbound
- WI-S20-001 PRR-GA-001 evidence pack EVT-018 depende de WI-S20-004 3/3 SLA met sustained 30d.
- Sprint S-20 GA Evidence Gate D+60 = GA-GO depende de WI-S20-004 3 lighthouse SLA met sustained 30d.

## 15. PERT estimate

| Estimate | Hours | Notes |
|---|---|---|
| **Optimistic (O)** | 24h | 3 customers LOI Q3 + smooth migration + clean 30d observation. |
| **Most Likely (M)** | 40h | 3 customers + migration + 30d observation + iteration + attestation Marketing co-led. |
| **Pessimistic (P)** | 64h | 1 customer desists + backup engagement + SLA miss + iteration + Legal review iteration. |
| **PERT** | (24 + 4×40 + 64) / 6 = **41.3h** | Per spec contract §12. |
| **Variance (σ²)** | ((64-24)/6)² = 44.4 | Std dev ≈ 6.7h. |

## 16. Sign-off canonical (HIGH_RISK 13)

13 roles per spec contract §5.1.

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect (Crypto SME folded; enterprise BYOK migration review) | _TBD_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD_ | _pending_ | _pending_ |
| 5 | SRE Lead (30d observation oncall + SLA claim tracking) | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (S-20 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead (per-customer migration validation + 30d observation) | _TBD_ | _pending_ | _pending_ |
| 8 | Product (Customer Success co-led + customer attestation Marketing co-led) | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy Officer (DPA signing 3 lighthouse + Schrems II TIA enterprise) | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD_ | _pending_ | _pending_ |
| 12 | Legal Counsel (DPA + sub-processor agreement + customer attestation Legal-reviewed) | _TBD_ | _pending_ | _pending_ |
| 13 | Finance (enterprise BYOK pricing + customer billing reconciliation) | _TBD_ | _pending_ | _pending_ |

## 17. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S20-004 (cycle 12.S20.0; 3 lighthouse customers migration — 2 team tier Forge customer-zero + 1 OSS Bazel/Buck2 + 1 enterprise BYOK + 30d observation post-migration + SLA claim met sustained 30d + customer attestations testimonial + case study Marketing co-led com Customer Success; GA Evidence Gate D+60 criterion). |

---

**Fim WI-S20-004.**
