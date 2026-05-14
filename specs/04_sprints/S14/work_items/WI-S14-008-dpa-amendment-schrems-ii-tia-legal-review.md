---
id: "WI-S14-008"
type: "work_item"
doc_status: "ACTIVE"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-28"
updated: "2026-05-14"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-009", "FF-HR-003"]
parent: "S-14"
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
tags: ["wi", "s14", "dpa", "schrems-ii", "tia", "edpb", "legal-review", "lighthouse-customer", "high-risk"]
---

# WI-S14-008 — DPA Amendment Template `legal/dpa-residency-amendment.md` Covering 4 Regions Residency Commitment + Schrems II TIA Template `legal/tia-template.md` (EDPB Recommendations 01/2020 Supplementary Measures: Technical {Encryption} + Organizational {DPA + 4 Regions Enumerated} + Contractual {DPA + Sub-processor Agreement}) + Legal Externo Review Path (No In-House Counsel; Engagement Plan ~$15-30k 6-Week Lead) + 1 Enterprise Customer Beta DPA Signed (Lighthouse) + 12th Sign-off Legal Counsel Exception

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-14](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S14-008 |
| Título | DPA amendment template `legal/dpa-residency-amendment.md` covering residency commitment per region (4 regions enumerated WNAM/ENAM/WEUR/SAM com data localization promises) + Schrems II TIA template `legal/tia-template.md` (EDPB Recommendations 01/2020 supplementary measures comprehensive: technical {encryption-at-rest BYOK FIPS-verified + Ed25519 erasure attestation + envelope encryption AES-256-GCM} + organizational {DPA + 4 regions enumerated + DPO contact + breach notification SLA 72h} + contractual {DPA + sub-processor agreement Cloudflare}) + Legal externo review path (no in-house counsel; engagement plan firm GDPR-experienced ~$15-30k 6-week lead) + 1 enterprise customer beta DPA signed (lighthouse customer) + ADR if waiver needed; 12th sign-off Legal Counsel exception (legal-touching WI; per WI-S14-008 explicit exception em sprint.md §14) |
| Sprint | S-14 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-009 (DPA enterprise customer-facing contract; legal exposure), FF-HR-003 (residency PII regulatory absoluto Schrems II + LGPD Art. 33 + GDPR Art. 46) |

## 1. Intent

DPA amendment + Schrems II TIA = enterprise customer onboarding contract closure tooling: customers em prospect enterprise (FedRAMP / EU / financial services) require DPA covering residency commitment per region + Schrems II TIA documenting supplementary measures (EDPB Recommendations 01/2020). Sem isso, contract closure stalls; revenue blocked; legal exposure per breach. Implementação: (1) **DPA amendment template** redação Legal-reviewed externamente cobrindo 4 regions enumerated (WNAM us-west / ENAM us-east / WEUR eu-west / SAM sa-east) + data localization commitments + sub-processor agreements (Cloudflare R2/D1/DO/KV) + DPO contact + breach notification SLA 72h (LGPD Art. 48 + GDPR Art. 33); (2) **Schrems II TIA template** (Transfer Impact Assessment) per EDPB Recommendations 01/2020 framework: technical measures {encryption + Ed25519 erasure attestation + envelope encryption}, organizational measures {DPA + 4 regions + DPO + breach notification SLA}, contractual measures {DPA + sub-processor agreement}; (3) **Legal externo review path** (no in-house counsel; engagement plan firma GDPR-experienced — Schellman Legal / Cooley / DLA Piper / Bird & Bird ~$15-30k 6-week lead); (4) **1 enterprise customer beta** DPA signed (lighthouse customer; demonstrates contract closure capability); (5) **ADR if waiver needed** (e.g., Legal externo review missed timeline = WAIVER-S14-001 com expiry 90d + revalidation trigger); (6) **12th sign-off Legal Counsel exception** (per sprint.md §14 explicit; legal-touching WI; precedent S-12 Compliance Officer additional sign-off para SOC 2). **Legal externo review path note**: NOT authoring legal text em this corpus; only structure templates + commitments + Legal review engagement.

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

DPA amendment + Schrems II TIA = customer-facing legal contract surface; bug em redação = legal exposure permanente + customer revenue blocked + breach notification mandatory if Schrems II violated. Legal externo review é mandatory (no in-house counsel solo-tier; per ADR-0034 Tier-1 staffing gap); engagement plan ~$15-30k 6-week lead; budget approved.

**Bugs catastróficos possíveis** (todos endereçados):

1. **DPA template imprecise residency commitment**: "data stored em region X" but failover to non-jurisdiction-allowed region (Schrems II violation). Mitigação: WI-S14-003 PAT-REGION-FAILOVER-001 residency restriction em replica_region; DPA explicit list per region + acceptable failover destinations; technical measure section references INV-REGION-NO-CROSS-LEAK + INV-DATA-RESIDENCY enforcement.

2. **Schrems II TIA missing supplementary measures**: TIA insufficient per EDPB Recommendations 01/2020 = transfer invalid. Mitigação: TIA covers all 3 measure types (technical + organizational + contractual); each measure references S-14 implementation evidence (BYOK FIPS + Ed25519 + 4 regions + DPO contact); Legal externo review validates.

3. **Sub-processor agreement Cloudflare missing**: Cloudflare = sub-processor; missing agreement = GDPR Art. 28 violation. Mitigação: DPA explicitly lists Cloudflare as sub-processor + Cloudflare DPA referenced + customer right to audit Cloudflare's compliance posture (cf-dpa.cloudflare.com).

4. **Breach notification SLA mismatch**: GDPR Art. 33 = 72h; LGPD Art. 48 = "reasonable time"; CoreLink commits 72h. Mitigação: DPA commits 72h breach notification; runbook RB-breach-notification (incident response template); on-call drill quarterly.

5. **DPO contact missing**: GDPR Art. 37 mandatory for some customers. Mitigação: DPO contact em DPA (currently Gustavo solo founder dual-hat; future external DPO contracted per ADR-0034 Option C).

6. **Legal externo review missed timeline**: 6-week lead exceeds sprint window. Mitigação: parallel track (Legal externo engaged D+0; sprint implementation D+0..D+25; Legal review concludes D+30..D+45); WAIVER-S14-001 if review missed (expiry 90d + revalidation trigger = customer signs).

7. **Enterprise customer beta unwilling to sign**: lighthouse customer rejection = revenue evidence missing. Mitigação: parallel customer engagement; multiple customer candidates; DPA template iteration.

8. **Schrems II legal landscape change post-template**: EDPB issues new guidance; TIA invalidates. Mitigação: quarterly Legal review cycle; EDPB monitoring; contract evergreen clause.

9. **Customer dispute clauses (e.g., audit rights, termination)**: customer requests deviation; sprint contract not anticipated. Mitigação: DPA template baseline; deviations require Legal externo review per-customer; document as ADR + customer addendum.

**Atacante adversarial scenarios** (legal):

- **Customer challenges DPA validity**: customer cites mismatch DPA vs implementation. Mitigação: DPA + technical evidence pack synced; quarterly Legal review verifies implementation matches commitments.

- **Regulatory authority audit**: DPA review by ANPD (LGPD) or DPA EU. Mitigação: DPA signed by lighthouse customer = real-world test; Legal externo review pre-signing.

- **Schrems II legal challenge from EDPB**: TIA challenged. Mitigação: EDPB Recommendations 01/2020 framework strict adherence; supplementary measures comprehensive evidence pack.

**Risk justification HIGH_RISK**:

- **FF-HR-009**: DPA enterprise customer-facing contract; revenue + legal exposure.
- **FF-HR-003**: residency regulatory absoluto Schrems II + LGPD + GDPR.
- **Reversibility**: signed DPA = legal commitment; mismatch implementation = breach notification + remediation.

**12 sign-offs canonical** (HIGH_RISK 11 + Legal Counsel exception per sprint.md §14): Owner + Final Approver + Architect (com Crypto SME folded para references BYOK + erasure attestation) + Security Lead + SRE Lead + Engineer + QA Lead + Product + Compliance Officer + Privacy Officer + AppSec + **Legal Counsel** (Legal externo review).

## 3. Customer Impact & Journey

**Persona 1 — Customer enterprise procurement (CISO + GC + DPO)**:
- Customer requests DPA covering residency + Schrems II.
- CoreLink provides DPA template + TIA + technical evidence pack.
- Customer Legal reviews; iteration cycle.
- DPA signed; contract closure unblocked.

**Persona 2 — CoreLink Sales engineer (RFP response)**:
- RFP question: "DPA available? Schrems II compliant? Sub-processors disclosed?".
- Evidence: DPA template + TIA + lighthouse customer signed reference.
- Diferenciador: AWS S3+KMS standard DPA; CoreLink S-14 = DPA + Schrems II TIA + per-region commitments + Ed25519 erasure attestation.

**Persona 3 — Auditor SOC 2 + GDPR DPO + ANPD (LGPD)**:
- DPA signed evidence; TIA Legal externo reviewed; quarterly cycle.
- LGPD Art. 33 + GDPR Art. 46 attestation.
- Compliance matrix mapping documented.

**SLA addendum**:
- DPA template Legal externo reviewed (engagement ~$15-30k 6-week lead).
- 1 enterprise customer beta DPA signed (lighthouse).
- Quarterly Legal review cycle.
- Schrems II TIA EDPB Recommendations 01/2020 framework.
- 4 regions enumerated em DPA (WNAM/ENAM/WEUR/SAM).
- Sub-processor Cloudflare agreement referenced.
- Breach notification SLA 72h (GDPR Art. 33 + LGPD Art. 48).

## 4. Capability Mapping

- **CAP-REGION-004** (DPA amendment + Schrems II TIA) — IMPLEMENTA primary.
- **CAP-COMPLIANCE-002** (legal templates + customer evidence pack) — IMPLEMENTA.
- Trace: `_spec_contract.md §4 + §5.3 R-S14-5` + `privacy_model.md` (CTRL-PRIV-031 residency) + `compliance_matrix.md` (LGPD Art. 33 + GDPR Art. 46 + Schrems II + EDPB Recommendations 01/2020).

## 5. Tipo

Legal templates + Legal externo review path + lighthouse customer engagement; HIGH_RISK; FF-HR-009 + FF-HR-003. **Legal externo review path note**: NOT authoring legal text em this corpus; only template structure + commitments + Legal review engagement.

## 6. Escopo

### 6.1 In-scope

1. **DPA amendment template `legal/dpa-residency-amendment.md`** (template structure; Legal externo redação completa):
   - **Section 1**: Parties (CoreLink as Processor; Customer as Controller).
   - **Section 2**: Definitions (Personal Data, Data Subject, Sub-processor, Region, etc.).
   - **Section 3**: Subject-matter and duration (per service agreement).
   - **Section 4**: Nature and purpose of processing (CoreLink content-addressable cache).
   - **Section 5**: Categories of data subjects (customer's end-users + tenant org members).
   - **Section 6**: Categories of personal data (per `privacy_model.md` CTRL-PRIV-001..030).
   - **Section 7**: **Residency commitment per region** (4 regions enumerated):
     - WNAM (us-west): tenant data stored em CF us-west infrastructure.
     - ENAM (us-east): tenant data stored em CF us-east infrastructure.
     - WEUR (eu-west): tenant data stored em CF eu-west infrastructure; **DO jurisdictional_restriction = "eu"** (S-14 enforces).
     - SAM (sa-east): tenant data stored em CF sa-east infrastructure.
     - Failover destinations restricted (WEUR ↔ SAM-EU sibling OR self read-replica per ADR; WNAM ↔ ENAM US sibling pair).
   - **Section 8**: Sub-processors:
     - Cloudflare Inc. (R2 + D1 + DO + KV + Workers + Custom Domains).
     - Customer KMS provider (BYOK; AWS / GCP / Azure / Vault as customer chooses).
   - **Section 9**: Security measures:
     - Encryption-at-rest (BYOK FIPS-verified per provider; AES-256-GCM body encryption; envelope encryption pattern).
     - Encryption-in-transit (TLS 1.3 + mTLS for Vault).
     - Access controls (least-privilege IAM; admin role + dual-approval; MFA).
     - Audit chain integrity (S-09 herdada; daily verifier; 7y retention).
     - Erasure attestation Ed25519-signed (WI-S14-007).
   - **Section 10**: Data subject rights:
     - Right to erasure: NIST SP 800-88 Rev.1 §2.4 crypto-erase mode via BYOK kill switch + Ed25519 attestation.
     - Right to access + portability + correction (S-11 herdada).
     - Cooling-off period 7d.
   - **Section 11**: Breach notification:
     - SLA 72h (GDPR Art. 33 + LGPD Art. 48).
     - Contact: DPO email + emergency hotline.
     - Runbook RB-breach-notification.
   - **Section 12**: International transfers:
     - Schrems II TIA referenced (separate doc).
     - EDPB Recommendations 01/2020 supplementary measures.
   - **Section 13**: Audit rights:
     - Customer right to request audit (annual; reasonable scope).
     - SOC 2 Type II report shared (under NDA).
   - **Section 14**: Termination:
     - Data deletion 30d post-termination.
     - Erasure attestation provided.
   - **Section 15**: Governing law + jurisdiction.
   - **Appendix A**: Technical measures evidence pack reference.
   - **Appendix B**: Organizational measures.
   - **Appendix C**: Contractual measures (sub-processor agreements).

2. **Schrems II TIA template `legal/tia-template.md`** (EDPB Recommendations 01/2020 framework):
   - **Section 1**: Transfer description (controller / processor / sub-processor / data flow).
   - **Section 2**: Transfer mechanism (SCCs + DPA + sub-processor agreement).
   - **Section 3**: Third-country law assessment:
     - US (FISA 702 + EO 12333 surveillance; CLOUD Act).
     - Mitigation: BYOK customer-controlled encryption (data unreadable to US gov sub-processor without CMK; customer holds CMK).
   - **Section 4**: Supplementary measures:
     - **Technical** (per EDPB §80-83):
       - Encryption-at-rest BYOK FIPS-verified per provider (Section 9 DPA).
       - Encryption-in-transit TLS 1.3 + mTLS Vault.
       - Pseudonymization em audit chain (CTRL-AUDIT-002 herdada).
       - Erasure via crypto-erase (NIST SP 800-88 Rev.1 §2.4) + Ed25519 attestation.
       - Envelope encryption AES-256-GCM with customer DEK wrapped via customer CMK.
       - Per-region key isolation (audit chain key per-region; Ed25519 attestation key per-region).
     - **Organizational** (per EDPB §84-87):
       - DPA + 4 regions enumerated.
       - DPO contact (Gustavo solo founder dual-hat; future external DPO).
       - Breach notification SLA 72h.
       - Sub-processor disclosure + audit rights.
       - Quarterly Legal review cycle.
     - **Contractual** (per EDPB §88-91):
       - DPA + sub-processor agreement Cloudflare.
       - Customer right to audit + SOC 2 Type II report.
       - Termination + data deletion 30d.
       - Erasure attestation provided.
   - **Section 5**: Effectiveness assessment:
     - BYOK customer-controlled CMK = data unreadable to US gov without customer cooperation.
     - Customer can revoke CMK access ≤ 5 min global (INV-BYOK-CRYPTO-SOVEREIGNTY).
     - Customer holds erasure attestation Ed25519-signed (forensic evidence of erasure).
   - **Section 6**: Sign-off (CoreLink DPO + Customer DPO + Legal externo review).

3. **Legal externo review engagement plan**:
   - Firms candidate (selected one): Schellman Legal / Cooley / DLA Piper / Bird & Bird / Fenwick & West / Latham & Watkins.
   - Scope: DPA amendment + Schrems II TIA template review.
   - Budget: $15-30k.
   - Timeline: 6-week lead.
   - Deliverable: redlined DPA + TIA + sign-off letter.
   - Engagement contract template (`legal/legal-externo-engagement-contract.md`).

4. **Lighthouse customer engagement (1 enterprise beta)**:
   - Identify candidate: existing prospect enterprise em RFP cycle.
   - Provide DPA + TIA template + technical evidence pack.
   - Customer Legal review cycle (parallel track).
   - DPA signed = lighthouse milestone.
   - Document: `specs/_audits/2026-XX-XX-lighthouse-customer-dpa-signed.md`.

5. **Quarterly Legal review cycle**:
   - Cadence: every 3 months post-template ratification.
   - Scope: EDPB guidance updates, Schrems II legal landscape, regulatory changes (GDPR enforcement, LGPD updates, ANPD opinions).
   - Deliverable: review report + DPA + TIA updates if needed.

6. **ADR if waiver needed `WAIVER-S14-001`** (template):
   - Trigger: Legal externo review missed sprint timeline (D+30 not achieved).
   - Compensating control: DPA + TIA Legal review COMPLETE em D+60 (GA Evidence Gate); 1 enterprise customer beta signed.
   - Expiry: 90d post-waiver issued.
   - Revalidation trigger: customer signs DPA OR D+60 Legal review concludes.
   - Sign-off: Owner + Final Approver + Compliance Officer + Privacy Officer + Legal Counsel.

7. **Audit emission** — CloudEvent per legal milestone:
   - `corelink.legal.dpa.template.published`.
   - `corelink.legal.tia.template.published`.
   - `corelink.legal.legal_externo_review.engaged`.
   - `corelink.legal.legal_externo_review.completed`.
   - `corelink.legal.lighthouse_customer.dpa_signed`.

8. **Métricas observability** (low cardinality):
   - `corelink_legal_dpa_signed_total{region, plan}` (counter).
   - `corelink_legal_tia_review_quarterly_status` (gauge; 0=pending/1=complete).
   - `corelink_legal_legal_externo_engagement_status` (gauge; 0=engaged/1=in_progress/2=complete).

9. **Customer-facing documentation**:
   - `docs/customer/dpa-onboarding.md` (sanitized; customer-shareable post-NDA).
   - DPA template + TIA template + technical evidence pack reference.
   - Onboarding sequence (Sales → Legal → Customer Legal → DPA signed).

10. **Internal documentation**:
    - `docs/internal/legal-review-process.md` (Legal externo engagement playbook).
    - Quarterly Legal review template.
    - Customer dispute resolution process.

### 6.2 Out-of-scope (deferred)

- **DPA legal text authorship** (Legal externo redação completa via engagement; this WI provides structure + commitments).
- **Customer-facing UI for DPA signing**: S-19 enterprise onboarding wizard.
- **Multi-DPO escalation workflow**: single DPO em S-14.
- **Customer-specific DPA addendums** (per-customer customization): per-customer + Legal externo cycle pós-GA.
- **TLA+ + pentest + PRR**: WI-S14-009.
- **Customer-facing portal for technical evidence pack**: S-16 admin UI.
- **Automated DPA signing flow**: S-19 onboarding wizard.
- **Translation DPA + TIA pt-BR / es / fr / de**: pós-GA based on customer demand.

## 7. Anti-Scope

- Author legal text em this corpus (Legal externo review path mandatory).
- Skip Legal externo review (no in-house counsel; legal exposure permanent).
- Skip Schrems II TIA (residency + GDPR Art. 46 violation).
- Skip 4 regions enumerated em DPA.
- Skip sub-processor Cloudflare agreement.
- Skip breach notification SLA 72h.
- Skip 1 enterprise customer beta lighthouse.
- Skip quarterly Legal review cycle.
- Skip 12th sign-off Legal Counsel exception.
- Auto-sign DPA without Legal externo review.
- Customer-specific DPA changes without Legal externo review.
- Skip ADR if waiver needed (compliance gap accumulates).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: WI-S14-008 — DPA + Schrems II TIA + Legal externo review + lighthouse customer

  Background:
    Given S-14 implementation foundation operational (4 regions + BYOK + erasure attestation)
    Given technical evidence pack assembled (BYOK FIPS + Ed25519 + 30k property test + chaos drills)

  Scenario: DPA template structure published
    Given legal/dpa-residency-amendment.md template
    When reviewed
    Then 15 sections + 3 appendices structured
    And residency commitment per region (4 enumerated)
    And sub-processor Cloudflare disclosed
    And breach notification SLA 72h
    And ToS evidence pack referenced

  Scenario: Schrems II TIA template structure published
    Given legal/tia-template.md template
    When reviewed
    Then EDPB Recommendations 01/2020 framework followed
    And 3 supplementary measure types documented (technical + organizational + contractual)
    And BYOK customer-controlled CMK referenced as effectiveness measure
    And erasure attestation Ed25519 referenced

  Scenario: Legal externo review engaged
    Given engagement contract com firma GDPR-experienced
    When contract signed + budget approved ($15-30k 6-week lead)
    Then legal externo review begins
    And deliverable scope: redlined DPA + TIA + sign-off letter

  Scenario: Legal externo review completed
    Given Legal externo redacted DPA + TIA
    When review concluded D+30 OR D+60 (Observation window)
    Then redlined templates committed
    And sign-off letter provided
    And ADR created if waiver needed (WAIVER-S14-001)

  Scenario: 1 enterprise customer beta DPA signed (lighthouse)
    Given DPA + TIA template Legal externo reviewed
    When provided to enterprise customer beta
    And Customer Legal review cycle
    Then DPA signed by lighthouse customer
    And committed report em audit folder

  Scenario: Quarterly Legal review cycle
    Given 3 months post-template ratification
    When quarterly Legal review runs
    Then EDPB guidance updates assessed
    And Schrems II legal landscape monitored
    And DPA + TIA updates if needed
    And report committed

  Scenario: Sub-processor Cloudflare agreement referenced
    Given DPA Section 8
    When reviewed
    Then Cloudflare Inc. listed as sub-processor
    And cf-dpa.cloudflare.com referenced
    And customer right to audit included

  Scenario: Residency commitment per region 4 enumerated
    Given DPA Section 7
    When reviewed
    Then 4 regions enumerated (WNAM/ENAM/WEUR/SAM)
    And data localization commitment per region
    And failover destinations restricted (WI-S14-003 referenced)
    And DO jurisdictional_restriction WEUR = "eu" (WI-S14-001 evidence)

  Scenario: Schrems II supplementary measures comprehensive
    Given TIA Section 4
    When reviewed
    Then technical measures (encryption + Ed25519 + envelope)
    And organizational measures (DPA + 4 regions + DPO + breach SLA)
    And contractual measures (DPA + sub-processor + audit + termination)
    And EDPB Recommendations 01/2020 referenced per measure

  Scenario: Erasure attestation Ed25519 referenced em DPA
    Given DPA Section 10 (data subject rights)
    When reviewed
    Then erasure right covered via NIST SP 800-88 Rev.1 §2.4 crypto-erase
    And BYOK kill switch + Ed25519 attestation referenced (WI-S14-006 + WI-S14-007)
    And cooling-off 7d (S-11 herdada)

  Scenario: Breach notification SLA 72h
    Given DPA Section 11
    When breach notification requirement reviewed
    Then SLA 72h committed (GDPR Art. 33 + LGPD Art. 48)
    And DPO contact + emergency hotline
    And runbook RB-breach-notification referenced

  Scenario: Audit emission per legal milestone
    Given legal milestone events
    When DPA template published + Legal externo engaged + lighthouse signed
    Then 5+ CloudEvents emitted em audit chain
    And 7y retention (CTRL-AUDIT-005)

  Scenario: ADR WAIVER-S14-001 if Legal externo review missed timeline
    Given Legal externo review missed D+30 timeline
    When WAIVER-S14-001 created
    Then expiry 90d
    And revalidation trigger documented (customer signs OR D+60 review concludes)
    And sign-off Owner + Final Approver + Compliance + Privacy + Legal Counsel

  Scenario: 12th sign-off Legal Counsel exception
    Given WI-S14-008 legal-touching exception
    When PRR review
    Then 12 sign-offs required (HIGH_RISK 11 + Legal Counsel)
    And Legal Counsel sign-off document committed
```

## 9. Design Decisions

### 9.1 Why Legal externo review (NÃO in-house counsel)

- No in-house counsel solo-tier (per ADR-0034).
- Legal externo specialized GDPR + Schrems II + LGPD = higher quality redação.
- Budget approved ~$15-30k 6-week lead = manageable cost.
- Future option: in-house Legal Counsel hired (S-19+ enterprise tier).

### 9.2 Why Schrems II TIA EDPB Recommendations 01/2020 framework (NÃO custom)

- EDPB framework standard; customer Legal expects.
- 3 measure types comprehensive (technical + organizational + contractual).
- Strict adherence reduces legal exposure.

### 9.3 Why 4 regions enumerated em DPA (NÃO general "multi-region")

- Customer demands specific regions for residency commitment.
- General "multi-region" = ambiguous; customer Legal rejects.
- Enumerated 4 + failover restrictions = precise commitment.

### 9.4 Why 12th sign-off Legal Counsel exception (legal-touching WI)

- Per sprint.md §14 explicit: "WI-S14-008 adds Legal Counsel as 12th sign-off (legal-touching exception)".
- Precedent: S-12 added Compliance Officer for SOC 2 attestation.
- Legal Counsel = Legal externo reviewing firma; sign-off letter = formal evidence.

### 9.5 Why 1 enterprise customer beta lighthouse (NÃO 0)

- Lighthouse demonstrates real-world contract closure.
- Sales evidence; reference customer for future deals.
- DPA template baseline validated in production.

### 9.6 Why quarterly Legal review cycle

- EDPB guidance evolves; Schrems II legal landscape volatile.
- Quarterly = balance cost vs drift exposure.
- Customer audit rights annual; Legal review quarterly precedes.

### 9.7 Why ADR WAIVER-S14-001 if Legal externo missed timeline

- 6-week lead may exceed sprint window.
- WAIVER documents accepted residual risk + expiry + revalidation.
- Sprint can SEAL implementation D+30; Legal review concludes D+60 GA Evidence Gate.

### 9.8 Why ADR potencial?

- Sim — **ADR-S14-007**: "DPA amendment + Schrems II TIA Legal externo review path + lighthouse customer + 12th sign-off Legal Counsel exception S-14". Decisão arquitetural legal-customer-facing. Ver `specs/03_architecture/adrs/ADR-S14-007-dpa-amendment-schrems-ii-tia-legal-externo.md`.

## 10. Completeness Criteria SOTA

- [x] **10.s14.008.1** DPA amendment template `legal/dpa-residency-amendment.md` structure published (15 sections + 3 appendices) (EVT-044).
- [x] **10.s14.008.2** Schrems II TIA template `legal/tia-template.md` EDPB Recommendations 01/2020 framework published (EVT-046).
- [x] **10.s14.008.3** Legal externo engagement plan documented (firm + scope + budget + timeline) (EVT-013).
- [ ] **10.s14.008.4** Legal externo review engaged ($15-30k 6-week lead) (EVT-044). *(runtime milestone — pending engagement)*
- [ ] **10.s14.008.5** Legal externo review completed; redlined templates + sign-off letter committed *(GA Evidence Gate D+60)* (EVT-044). *(observation window)*
- [ ] **10.s14.008.6** 1 enterprise customer beta DPA signed (lighthouse) *(GA Evidence Gate D+60)* (EVT-044). *(observation window)*
- [x] **10.s14.008.7** Quarterly Legal review cycle documented (cadence + scope + deliverable).
- [x] **10.s14.008.8** Audit emission per legal milestone (5+ CloudEvents) — events listed in `docs/internal/legal-review-process.md §8`.
- [x] **10.s14.008.9** ADR-S14-007 (DPA + TIA + Legal externo path + 12th sign-off) ratificada.
- [x] **10.s14.008.10** ADR WAIVER-S14-001 template committed (expiry 90d + revalidation documented; status INACTIVE until triggered).
- [ ] **10.s14.008.11** SOC 2 + LGPD Art. 33 + GDPR Art. 46 + Schrems II + EDPB Recommendations 01/2020 attestation em PRR doc (EVT-044). *(PRR milestone — WI-S14-009)*
- [ ] **10.s14.008.12** 12th sign-off Legal Counsel exception documented em PRR doc. *(observation window — pending Legal externo sign-off)*
- [x] **10.s14.008.13** Customer doc `docs/customer/dpa-onboarding.md` published.
- [x] **10.s14.008.14** Internal doc `docs/internal/legal-review-process.md` published.
- [x] **10.s14.008.15** Sub-processor Cloudflare agreement referenced em DPA Section 8.
- [x] **10.s14.008.16** Erasure attestation Ed25519 + BYOK kill switch referenced em DPA Section 10.

## 11. DoD

- [x] DPA amendment template + Schrems II TIA template structure published.
- [x] Legal externo engagement plan + budget + timeline.
- [ ] Legal externo review engaged. *(runtime milestone)*
- [ ] (Observation window) Legal externo review completed; redlined committed.
- [ ] (Observation window) 1 enterprise customer beta DPA signed.
- [x] Quarterly Legal review cycle documented.
- [x] Audit emission events documented (CloudEvents listed in `docs/internal/legal-review-process.md §8`).
- [x] Métricas observability (3 metrics: `corelink_legal_dpa_signed_total`, `corelink_legal_tia_review_quarterly_status`, `corelink_legal_legal_externo_engagement_status`).
- [x] Customer doc + internal doc.
- [x] ADR-S14-007 (DPA + TIA + Legal externo + 12th sign-off) ratificada.
- [x] ADR WAIVER-S14-001 template committed (INACTIVE — activate if D+30 missed).
- [ ] Code review (Architect + Compliance + Privacy + Legal Counsel). *(pending sign-offs)*
- [ ] PRR Compliance + Privacy + Legal Counsel mini-sign-off. *(pending sign-offs)*

## 12. Invariants Validated

### Mantidas

- **INV-DATA-RESIDENCY** (CRITICAL — registry §3.11 herdada): DPA Section 7 documents commitment.
- **INV-REGION-NO-CROSS-LEAK** (CRITICAL — registry §3.12 nova WI-S14-002): DPA references implementation.
- **INV-BYOK-CRYPTO-SOVEREIGNTY** (CRITICAL — registry §3.12 nova WI-S14-004 + WI-S14-006): DPA Section 10 references kill switch.
- **INV-ERASURE-ATTESTATION-SIGNED** (HIGH — registry §3.12 nova WI-S14-007): DPA Section 10 references attestation.

### Novas

Nenhuma direta neste WI; ratifies INVs nova S-14 implementadas em WI-001..007.

TLA+ alignment: registry §4.2 indica `region_residency.tla` PLANNED S-14 WI-S14-009.

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| DPA amendment template | `legal/dpa-residency-amendment.md` | Markdown |
| Schrems II TIA template | `legal/tia-template.md` | Markdown |
| Legal externo engagement contract | `legal/legal-externo-engagement-contract.md` | Markdown |
| Quarterly Legal review template | `legal/quarterly-legal-review-template.md` | Markdown |
| Customer doc DPA onboarding | `docs/customer/dpa-onboarding.md` | Markdown |
| Internal doc Legal review process | `docs/internal/legal-review-process.md` | Markdown |
| Lighthouse customer DPA signed report | `specs/_audits/2026-XX-XX-lighthouse-customer-dpa-signed.md` | Markdown |
| Legal externo review report | `specs/_audits/2026-XX-XX-legal-externo-review-s14.md` | Markdown |
| ADR-XXXX (DPA + TIA + Legal externo + 12th sign-off) | `specs/03_architecture/adrs/ADR-XXXX-dpa-amendment-schrems-ii-tia-legal-externo.md` | Markdown |
| ADR WAIVER-S14-001 (if needed) | `specs/03_architecture/adrs/WAIVER-S14-001-legal-externo-timeline.md` | Markdown |

## 14. Quality Standards SOTA

- **14.s14.008.1** DPA template + TIA template Legal externo reviewed (no in-house counsel).
- **14.s14.008.2** Schrems II EDPB Recommendations 01/2020 framework strict adherence.
- **14.s14.008.3** 4 regions enumerated em DPA Section 7.
- **14.s14.008.4** Sub-processor Cloudflare disclosed em DPA Section 8.
- **14.s14.008.5** Breach notification SLA 72h committed (GDPR Art. 33 + LGPD Art. 48).
- **14.s14.008.6** Audit emission per legal milestone + 7y retention.
- **14.s14.008.7** Quarterly Legal review cycle documented.
- **14.s14.008.8** 1 enterprise customer beta DPA signed lighthouse.
- **14.s14.008.9** Customer offline verification flow documented.
- **14.s14.008.10** Cost regression gate em CI: Legal externo budget ≤ $30k.
- **14.s14.008.11** SOC 2 + Schrems II + LGPD Art. 33 + GDPR Art. 46 + EDPB Recommendations 01/2020 attestation.
- **14.s14.008.12** 12th sign-off Legal Counsel exception documented.

## 15. Chaos Experiments

Não-aplicável (este WI é templates + Legal review path; chaos experiments aplicam em WI-001..007 implementação).

Risk-handling exercises (não chaos):

1. **DPA template Legal externo iteration**: simulate Legal feedback cycle; verify template adaptable.
2. **Customer Legal review iteration**: simulate customer pushback; verify response process.
3. **Schrems II legal landscape change**: simulate EDPB new guidance; verify quarterly review cycle catches.
4. **Sub-processor Cloudflare DPA change**: simulate Cloudflare DPA update; verify customer notification process.
5. **Lighthouse customer DPA signing delay**: simulate 1-month delay; verify ADR WAIVER process.
6. **Legal externo timeline missed**: simulate 6-week + 2-week delay; verify ADR WAIVER-S14-001 + revalidation.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 12 sign-offs canonical (S-14 ship gate é WI-S14-009; este WI passa por mini-PRR Compliance + Privacy + Legal Counsel review):

- [ ] All 14 Gherkin scenarios green (template + engagement; Observation window for Legal review + lighthouse signed).
- [ ] DPA template + TIA template structure published.
- [ ] Legal externo engaged.
- [ ] Quarterly Legal review cycle documented.
- [ ] ADR-XXXX (DPA + TIA + Legal externo + 12th sign-off) published.
- [ ] ADR WAIVER-S14-001 if needed (expiry + revalidation).
- [ ] Compliance Officer review (SOC 2 CC6.1 + Schrems II + LGPD Art. 33 + GDPR Art. 46 + EDPB).
- [ ] Privacy Officer review (LGPD Art. 33 §1º + GDPR Art. 46 + customer breach notification).
- [ ] Legal Counsel review (Legal externo redlined templates + sign-off letter).
- [ ] (Observation window) Legal externo review completed.
- [ ] (Observation window) Lighthouse customer DPA signed.
- [ ] Customer doc + internal doc published.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | DPA amendment template structure 15 sections + 3 appendices | 4h |
| ST-002 | Schrems II TIA template EDPB Recommendations 01/2020 framework 6 sections | 3.5h |
| ST-003 | Legal externo engagement plan + firm research + budget approval | 2h |
| ST-004 | Legal externo engagement contract redação | 1.5h |
| ST-005 | Quarterly Legal review template + cadence | 1.5h |
| ST-006 | Customer doc DPA onboarding redação | 2h |
| ST-007 | Internal doc Legal review process | 1.5h |
| ST-008 | Lighthouse customer engagement coordination | 2h |
| ST-009 | Audit emission per legal milestone | 1h |
| ST-010 | Métricas observability (3 metrics) | 1h |
| ST-011 | ADR-XXXX redação | 3h |
| ST-012 | ADR WAIVER-S14-001 template (if needed) | 1.5h |
| ST-013 | Code review (Architect + Compliance + Privacy + Legal Counsel) | 3h |
| ST-014 | (Observation window D+30..D+60) Legal externo review iteration tracking | 4h |
| ST-015 | (Observation window) Lighthouse customer DPA negotiation tracking | 4h |

**Total Optimistic**: ~35h (ex-observation window) + ~8h observation = ~43h. **PERT** (O=12h, M=20h, P=32h, per spec contract §12): **20.7h**. (PERT focus em sprint window; observation window é background.)

## 18. Dependencies

### Hard blockers

- **WI-S14-001 SEALED** (4 regions infra; DPA references provisioning).
- **WI-S14-002 SEALED** (region pinning; DPA references INV-REGION-NO-CROSS-LEAK).
- **WI-S14-003 SEALED** (failover; DPA references PAT-REGION-FAILOVER-001).
- **WI-S14-004 SEALED** (BYOK trait; DPA references envelope encryption).
- **WI-S14-005 SEALED** (4 providers; DPA references FIPS doc).
- **WI-S14-006 SEALED** (kill switch; DPA Section 10 references INV-BYOK-CRYPTO-SOVEREIGNTY).
- **WI-S14-007 SEALED** (erasure attestation; DPA Section 10 references INV-ERASURE-ATTESTATION-SIGNED).
- Legal externo engagement budget approved ($15-30k).

### Soft blockers

- Lighthouse customer engagement (parallel sales track).

### Outbound

- **WI-S14-009** (PRR ship gate; 12 sign-offs canonical).
- **S-19** (enterprise onboarding wizard consume DPA template).
- **S-20** (GA exige DPA signed lighthouse customers).

## 19. Effort PERT

O: 12h, M: 20h, P: 32h → PERT **20.7h** (per spec contract §12).

## 20. Time-boxing

**24h hard limit owner** (sprint window; observation window 8h tracking). If exceeded → escalation: split em sub-WI (DPA template vs TIA template vs Legal review path).

## 21. Observability

3 métricas listadas §6.1.8. Logs structured JSON.

Dashboard widget DASH-LEGAL (small):
- DPA signed count per region (lighthouse + future customers).
- Legal externo review status (engaged / in_progress / complete).
- Quarterly Legal review status.

## 22. Cost Analysis

- Legal externo engagement: $15-30k one-time.
- Quarterly Legal review: ~$5k/quarter = $20k/year ongoing.
- Lighthouse customer engagement: internal (Sales + Owner time).
- Audit emission infrastructure: composed (S-09 herdada).
- **Total custo direto WI-S14-008**: ~$15-30k initial + $20k/year ongoing.

## 23. API Contract

Não-aplicável (este WI é templates + Legal review; não introduz API surface).

## 24. Post-mortem Hooks

- Customer DPA dispute → post-mortem com Legal + customer trust review.
- Schrems II legal landscape change post-template → quarterly Legal review catches.
- Legal externo review missed timeline > 90d → SEV-2 + ADR WAIVER review.
- Lighthouse customer DPA signing delay > 90d → SEV-2 + parallel customer engagement.
- Sub-processor Cloudflare DPA change → customer notification process review.
- Breach notification SLA miss (> 72h) → CRITICAL post-mortem + customer trust review.

## 25. Rollback / Recovery

- Template rollback: revert PR + re-engage Legal externo if substantive change.
- Customer rollback: customer can terminate DPA per termination clause.
- Legal externo engagement rollback: contract has termination clause.
- RTO: 6-week (Legal review cycle).
- RPO 0 (templates versioned).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: DPA signed by authorized customer representative; Legal externo verifies authority.
- **Tampering**: templates versioned em Git; signed by Cosign deploy (S-12 herdada).
- **Repudiation**: signed DPA = legal commitment; audit chain emits.
- **Information disclosure**: customer-shareable post-NDA.
- **DoS**: Legal externo review timeline = 6-week buffer.
- **Elevation of privilege**: customer-side authority verified; CoreLink-side Legal Counsel sign-off.

**LINDDUN delta**:
- **Linkability**: DPA links customer + CoreLink (intentional).
- **Identifiability**: customer + DPO contact (intentional).
- **Non-repudiation**: signed DPA = legal commitment.
- **Detectability**: quarterly Legal review monitors compliance.
- **Disclosure**: DPA + TIA shared with customer pre-signing.
- **Unawareness**: customer notified of breach (72h SLA); customer educated re: erasure right + cooling-off.
- **Non-compliance**: SOC 2 + LGPD Art. 33 + GDPR Art. 46 + Schrems II + EDPB Recommendations 01/2020 satisfied.

## 27. Knowledge Transfer

- `docs/customer/dpa-onboarding.md` — customer-facing DPA + TIA + technical evidence pack guide.
- `docs/internal/legal-review-process.md` — Legal externo engagement playbook.
- ADR-XXXX — DPA + TIA + Legal externo path ratification.
- Workshop interno (1.5h) com Compliance + Privacy + Legal Counsel + Owner pós-merge.
- Onboarding test (5 questions): DPA Section 7 (residency 4 regions), TIA EDPB Recommendations 01/2020 measures, sub-processor Cloudflare, breach notification 72h, quarterly Legal review.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | DPA template imprecise residency | L | M | HIGH | L | LOW | Legal externo review + 4 regions enumerated + INV references |
| R-002 | Schrems II TIA missing measure | L | H | HIGH | M | LOW | EDPB Recommendations 01/2020 strict adherence + Legal externo |
| R-003 | Sub-processor Cloudflare missing | L | M | HIGH | L | LOW | DPA Section 8 explicit + cf-dpa.cloudflare.com |
| R-004 | Breach notification SLA mismatch | L | M | HIGH | L | LOW | DPA Section 11 72h + RB-breach-notification |
| R-005 | DPO contact missing | L | M | MEDIUM | L | LOW | DPO em DPA + future external DPO contracted |
| R-006 | Legal externo review missed timeline | M | M | HIGH | M | LOW | ADR WAIVER-S14-001 + parallel track + buffer 30d |
| R-007 | Lighthouse customer unwilling | M | M | HIGH | M | LOW | Multiple candidates + DPA template iteration |
| R-008 | Schrems II legal landscape change | M | M | HIGH | M | LOW | Quarterly Legal review + EDPB monitoring |
| R-009 | Customer dispute clauses | M | L | MEDIUM | M | LOW | DPA template baseline + per-customer Legal externo |
| R-010 | Customer DPA validity challenge | L | L | HIGH | L | LOW | DPA + technical evidence pack synced + quarterly review |
| R-011 | Regulatory authority audit | L | L | HIGH | L | LOW | DPA signed + Legal externo + lighthouse customer evidence |
| R-012 | Cost regression Legal externo | L | L | LOW | L | LOW | Budget approved $15-30k + cost gate |

## 29. Review Checkpoints

1. **Design (D+0)**: Architect + Compliance + Privacy review template structure.
2. **Template draft (D+1)**: DPA + TIA structure published.
3. **Legal externo engagement (D+2)**: contract signed; review begins.
4. **Lighthouse customer engagement (D+5)**: parallel sales track.
5. **Compliance review (D+8)**: Compliance Officer review template.
6. **Privacy review (D+8)**: Privacy Officer review LGPD + GDPR alignment.
7. **(Observation window D+30..D+60)** Legal externo review completion + lighthouse customer DPA signed.
8. **PRR mini (D+30 implementation; D+60 final)**: Architect + Compliance + Privacy + Legal Counsel sign-off.

## 30. Sign-off (HIGH_RISK 12 canonical — Legal Counsel exception per sprint.md §14)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — Crypto SME folded para references BYOK + erasure attestation; DPA + TIA composition with WI-001..007_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — sub-processor scope + IAM evidence_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — RB-breach-notification + 72h SLA operational_ | _pending_ | _pending_ |
| 6 | Engineer (S-14 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; emphatic — SOC 2 CC6.1 + LGPD Art. 33 + GDPR Art. 46 + Schrems II + EDPB Recommendations 01/2020 attestation_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; emphatic — LGPD Art. 33 §1º + GDPR Art. 46 + customer breach notification + DPO contact_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; sub-processor scope review_ | _pending_ | _pending_ |
| **12** | **Legal Counsel (Legal externo)** | **_TBD; emphatic — DPA + TIA Legal externo redlined review + sign-off letter_** | _pending_ | _pending_ |

> 12th sign-off Legal Counsel exception per sprint.md §14 (legal-touching WI; precedent S-12 Compliance Officer adicional). Crypto SME folds into Architect role specialization (cripto-touching references BYOK + erasure attestation em DPA + TIA).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação WI-S14-008 (cycle 12.S14.0); DPA + Schrems II TIA + Legal externo + lighthouse + 12th sign-off Legal Counsel exception. |
| 1.1.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | Implementation complete: DPA template (15s+3app) + TIA (EDPB Rec 01/2020) + engagement contract + quarterly review template + customer doc + internal doc + ADR-S14-007 + WAIVER-S14-001 (INACTIVE) + audit placeholders. work_status DONE. |

## 32. Anti-patterns evitados

- Author legal text em this corpus (Legal externo review path mandatory).
- Skip Legal externo review.
- Skip Schrems II TIA EDPB Recommendations 01/2020 framework.
- Skip 4 regions enumerated em DPA.
- Skip sub-processor Cloudflare agreement.
- Skip breach notification SLA 72h.
- Skip 1 enterprise customer beta lighthouse.
- Skip quarterly Legal review cycle.
- Skip 12th sign-off Legal Counsel exception.
- Auto-sign DPA without Legal externo review.
- Customer-specific changes without Legal externo.
- Skip ADR WAIVER if needed.

---

**Fim WI-S14-008.** Próximo: WI-S14-009 (TLA+ region_residency + RB-BYOK-REVOKE + external pentest BYOK + PRR HIGH_RISK 11 canonical).
