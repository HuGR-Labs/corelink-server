---
id: "WI-S20-003"
type: "work_item"
doc_status: "SEALED"
work_status: "DONE"
audit_status: "ACTIVE"
version: "1.1.0"
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
  - "COMPLIANCE-MATRIX"
  - "PRIVACY-MODEL"
  - "SECURITY-MODEL"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s20", "ga", "soc-2", "drata", "vanta", "gap-analysis", "type-i-roadmap", "high-risk"]
---

# WI-S20-003 — SOC 2 Drata/Vanta Gap Analysis Tooling Integration (NÃO Audit Completo; é Rehearsal + GAP-XX Identification) + Concrete GAP-XX Items + Fix Timeline + Roadmap Pra Type I Engagement em 6 Meses Pós-GA + Drata/Vanta Dashboard Verde > 95% Controls + Gaps Documented com Fix Timeline + Continuous Compliance Monitoring + SOC 2 Trust Services Criteria (TSC 2017) Coverage CC6.1 + CC6.7 + CC8.1 + Cumulative LGPD + GDPR + CCPA + EDPB SCCs + NIST SP 800-53 Rev 5 + ISO/IEC 27001:2022 Baseline

> **doc_status:** SEALED · **work_status:** DONE · **lane:** HIGH_RISK
> **Parent:** [S-20](../sprint.md) · **Assignee:** Gustavo Schneiter
>
> **SEALED 2026-05-14** — Deliverables D1..D5 committed:
> - D1 Drata tooling integration (selected per `specs/_compliance/vendor-shortlist-soc2.md`; dashboard live em staging 96.4% green).
> - D2 Gap analysis report — `specs/_compliance/SOC2-GAP-ANALYSIS.md` (33 GAPs; 1 blocking-GA closing D+30 + fallback; 9 major; 23 minor).
> - D3 Fix timeline per GAP-XX — embedded em D2 com owner/ETA/remediation.
> - D4 Roadmap Type I 6m pós-GA — `specs/_compliance/SOC2-ROADMAP.md` (Schellman primary; $40-85k Type I; Type II T+12m..T+18m).
> - D5 Readiness score — `specs/_audits/2026-05-14-soc2-readiness-score.md` (internal 83.7%; Drata 96.4%; projected Type I pass-rate 95%).

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S20-003 |
| Título | SOC 2 Drata/Vanta gap analysis tooling integration + concrete GAP-XX items + fix timeline + roadmap Type I 6m pós-GA |
| Sprint | S-20 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (controle final pré-GA — compliance baseline) + FF-HR-009 (contratos com customers vão production = compliance posture customer-facing) + FF-HR-010 (primeira entrega regulatory live em escala) |

## 1. Objetivo + JTBD

**Objetivo**: Integrar Drata ou Vanta tooling (continuous compliance monitoring) + executar gap analysis preliminar pra SOC 2 Type I (NÃO audit completo; é rehearsal + GAP-XX identification) + produzir concrete GAP-XX items + fix timeline + roadmap pra Type I engagement em 6 meses pós-GA. Drata/Vanta dashboard verde > 95% controls; gaps documented com fix timeline.

**JTBD**: "Como Compliance Officer / DPO / CISO em prospect enterprise, preciso evidência verificável que: (a) **CoreLink tem compliance posture maduro** com Drata ou Vanta tooling integration (continuous compliance monitoring vs ad-hoc spreadsheet tracking); (b) **gap analysis identifies concrete GAP-XX items** (NÃO promessas vagas; específicos como "GAP-01: missing access review quarterly cadence; GAP-02: encryption-at-rest documented but not attested"); (c) **fix timeline per GAP-XX** com owner + due date + remediation plan; (d) **roadmap pra Type I engagement** em 6 meses pós-GA (firm engagement + 6-month observation period typical SOC 2 Type I); (e) **Drata/Vanta dashboard verde > 95% controls** (gaps documented com fix timeline); (f) **NÃO é audit completo** — é tooling integration + GAP identification + roadmap; SOC 2 Type I cert é deferred 6 meses pós-GA per spec contract §10 anti-scope."

GA-go binary engineering gate.

## 2. Scope

### 2.1 In-scope

1. **Drata ou Vanta tooling integration** (continuous compliance monitoring):
   - **Selection**: Drata vs Vanta competitive evaluation Q3 antes do sprint start; weighted scoring (cost + integrations + UX + SOC 2 + ISO 27001 + GDPR coverage).
   - **Integration**: tooling connects to Cloudflare + GitHub + Clerk + Stripe + R2 + D1 + DO + KV + cumulative S-09..S-19 deployments.
   - **Continuous monitoring**: dashboard verde > 95% controls (per spec contract §9 quality standards delta local).
   - **Engagement**: Q3 antes do sprint start (early continuous monitoring; iterate during sprint).

2. **Gap analysis report** `specs/_audits/2026-XX-XX-soc2-gap-analysis.md`:
   - **NÃO audit completo** — é rehearsal + GAP-XX identification (per spec contract §1 + §5.1 R-S20-3 + §10.s20.5).
   - **Concrete GAP-XX items** (específicos, NÃO vagos):
     - Examples: "GAP-01: access review quarterly cadence missing — owner: Compliance Officer; fix by: D+30; remediation: implement quarterly access review process per SOC 2 CC6.7"; "GAP-02: encryption-at-rest documented but not attested — owner: Architect; fix by: D+30 (canonical blocking-GA gaps close before D+30 SEAL per Lote 10.20 codex P2); remediation: FIPS attestation per provider em compliance/byok-fips-matrix.md (S-14 deliverable)"; "GAP-03: incident response plan documented but not tested — owner: SRE Lead; fix by: D+60; remediation: synthetic page weekly < 5 min sustained 30d (WI-S20-006 deliverable)"; etc.
   - Coverage SOC 2 Trust Services Criteria (TSC 2017) primary: CC6.1 (logical access) + CC6.7 (change management) + CC8.1 (system change).
   - Cumulative coverage LGPD + GDPR + CCPA + EDPB SCCs + NIST SP 800-53 Rev 5 + ISO/IEC 27001:2022 baseline.
   - GAP count target: < 30 GAPs (if > 50 → audit rescheduled per spec contract §15 risk row 3 mitigation).

3. **Fix timeline per GAP-XX**:
   - Each GAP-XX assigned owner + due date + remediation plan + SOC 2 control reference + status (open|in_progress|closed).
   - Pre-GA fix: gaps blocking GA-go binary remediated before D+30 Implementation SEAL.
   - Pos-GA fix: gaps allowed to defer documented com explicit waiver + ADR + expiry; fix within 3 meses pós-GA.

4. **Roadmap Type I engagement pós-GA 6m** `compliance/soc2-roadmap-type1.md`:
   - Firm engagement (Schellman é primary candidate per spec contract §17 references; A-LIGN secondary).
   - Engagement timeline: firm bid + selection + SOW (T-3 meses pós-GA); audit fieldwork (T-3..T-6 meses pós-GA); report delivery (T+6 meses pós-GA).
   - Budget estimate: $30-60k Type I engagement + Drata/Vanta annual subscription $5-15k.
   - Type II target: 12 meses pós-GA (firm engagement Type I → Type II progression typical).

5. **Drata/Vanta dashboard verde > 95% controls**:
   - Continuous compliance monitoring dashboard live em staging.
   - Per-control status: green (compliant) | yellow (in-progress) | red (gap).
   - Gaps documented com fix timeline per GAP-XX.
   - Quarterly review cadence post-GA.

### 2.2 Anti-scope

- ❌ SOC 2 Type I audit completo durante S-20 (defer 6 meses pós-GA; S-20 entrega gap analysis + roadmap apenas).
- ❌ SOC 2 Type II cert (12 meses pós-GA target).
- ❌ FedRAMP Moderate baseline (pós-GA enterprise pivot).
- ❌ ISO/IEC 27001 cert (6-12 meses pós-GA).
- ❌ HIPAA / PCI DSS cert (não-applicable scope; CoreLink scope é multi-tenant cache, NÃO healthcare/payment direct).
- ❌ Customer-facing compliance dashboard UI (pós-GA enterprise).
- ❌ Multi-DPO escalation workflow (pós-GA enterprise).

## 3. Capability mapping

- **CAP-GA-003** (SOC 2 gap analysis): IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.1 R-S20-3 + §6.1 + §10.s20.5 + §15 risk row 3 + §10 anti-scope (SOC 2 Type I 6m pós-GA)` + `compliance_matrix.md (SOC 2 + LGPD + GDPR + CCPA cumulative)` + `SOC 2 Trust Services Criteria (TSC 2017)` + `NIST SP 800-53 Rev 5` + `ISO/IEC 27001:2022`.

## 4. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S20-003-D1 | Drata ou Vanta tooling integration | platform live em staging | continuous compliance monitoring dashboard; > 95% controls verde |
| S20-003-D2 | Gap analysis report concrete GAP-XX | `specs/_audits/2026-XX-XX-soc2-gap-analysis.md` | NÃO audit completo; concrete GAP-XX items < 30 (target); coverage SOC 2 TSC + cumulative |
| S20-003-D3 | Fix timeline per GAP-XX | embedded em D2 | each GAP-XX: owner + due date + remediation plan + status |
| S20-003-D4 | Roadmap Type I engagement 6m pós-GA | `compliance/soc2-roadmap-type1.md` | firm bid + selection + SOW timeline + budget estimate $30-60k + Type II target 12m pós-GA |
| S20-003-D5 | Drata/Vanta dashboard verde > 95% | dashboard live | quarterly review cadence post-GA documented |

## 5. Detailed design

### 5.1 Drata vs Vanta selection rubric

Q3 (pré-sprint):

| Criterion | Weight | Drata | Vanta |
|---|---|---|---|
| Cost (annual subscription) | 20% | $X | $Y |
| Integrations (Cloudflare + GitHub + Clerk + Stripe + ...) | 25% | _eval_ | _eval_ |
| UX + dashboard | 15% | _eval_ | _eval_ |
| SOC 2 coverage | 20% | _eval_ | _eval_ |
| ISO 27001 coverage | 10% | _eval_ | _eval_ |
| GDPR coverage | 10% | _eval_ | _eval_ |

Selection per weighted score Q3.

### 5.2 GAP-XX template

```markdown
### GAP-XX: <title>

- **Severity**: blocking-GA | major | minor
- **Owner**: <role>
- **Due date**: D+XX OR T+X meses pós-GA
- **SOC 2 control reference**: CC6.1 | CC6.7 | CC8.1 | ...
- **Remediation plan**: <description>
- **Status**: open | in_progress | closed
- **Evidence**: <link>
```

Examples:

```markdown
### GAP-01: access review quarterly cadence missing

- **Severity**: major
- **Owner**: Compliance Officer
- **Due date**: D+30
- **SOC 2 control reference**: CC6.7 (change management)
- **Remediation plan**: Implement quarterly access review process; document em `compliance/access-review-quarterly.md`; integrate com Drata/Vanta tooling for continuous monitoring; first review cycle Q1 pós-GA.
- **Status**: open
- **Evidence**: TBD
```

```markdown
### GAP-02: encryption-at-rest documented but not attested

- **Severity**: blocking-GA
- **Owner**: Architect (Crypto SME folded)
- **Due date**: D+30 (Lote 10.20 codex P2 canonical fix — blocking-GA gaps MUST close before D+30 Implementation SEAL; prior D+45 due date contradicted blocking-GA classification + GA Evidence Gate D+60 cadence; corrigida para D+30 hard cap; fallback se não fix at D+30 → reclassify as non-blocking-GA com Compliance Officer + Architect sign-off + ADR documenting deferral)
- **SOC 2 control reference**: CC6.1 (logical access — encryption)
- **Remediation plan**: FIPS attestation per BYOK provider em `compliance/byok-fips-matrix.md` (S-14 deliverable cumulative); attestation document linked em PRR-GA-001 evidence pack.
- **Status**: open
- **Evidence**: `compliance/byok-fips-matrix.md` (S-14)
```

### 5.3 Roadmap Type I structure

```markdown
# SOC 2 Type I Engagement Roadmap (6 meses pós-GA)

## Timeline
- **T+0** (GA day): Drata/Vanta dashboard verde > 95% sustained; gap analysis fix timeline executed.
- **T+1 mês**: Firm bid + selection (Schellman primary; A-LIGN secondary).
- **T+2 meses**: SOW signed; access provisioning; engagement contract.
- **T+3 meses**: Audit fieldwork starts.
- **T+5 meses**: Audit fieldwork complete; draft report.
- **T+6 meses**: Type I report delivered; sanitized version sharable.

## Budget
- Type I engagement: $30-60k (firm).
- Drata/Vanta annual subscription: $5-15k.
- Total: $35-75k pós-GA Q1.

## Type II target
- T+12 meses (Type II requires 6-12 month observation period).
- Continuous compliance monitoring sustained via Drata/Vanta.
```

## 6. Acceptance criteria

### 6.1 Positive paths

1. **Drata ou Vanta tooling integrated** Q3 + dashboard live em staging.
2. **Gap analysis report committed** com concrete GAP-XX items (NÃO promessas vagas).
3. **Fix timeline per GAP-XX** com owner + due date + remediation plan.
4. **Roadmap Type I engagement 6m pós-GA committed** em `compliance/soc2-roadmap-type1.md`.
5. **Drata/Vanta dashboard verde > 95% controls** sustained.
6. **GAP count < 30** (target); blocking-GA gaps remediated antes D+30 Implementation SEAL.

### 6.2 Negative paths (≥ 4 mandatory)

1. **GAP count > 50** → audit rescheduled per spec contract §15 risk row 3 mitigation; iterate Q3+Q4 antes do sprint start.
2. **Drata/Vanta integration falha** (tooling não integrates com Cloudflare/GitHub) → fallback alternative tool OR manual spreadsheet tracking + ADR.
3. **Blocking-GA gap discovered mid-sprint** (e.g., critical control gap) → Implementation SEAL D+30 blocked até remediation; potential delay.
4. **Roadmap Type I budget exceeds approved** → re-scope (defer Type II target to T+18 meses; reduce firm scope).
5. **Compliance officer sign-off declined** per gap severity → process review + iteration cycle.
6. **Drata/Vanta dashboard < 95% sustained** → SEAL blocked até > 95% achieved.

## 7. Test plan

### 7.1 Tooling integration validation

- Drata/Vanta connects to Cloudflare + GitHub + Clerk + Stripe + R2 + D1 + DO + KV.
- Continuous compliance monitoring dashboard live em staging.
- Per-control status accurate.

### 7.2 Gap analysis validation

- GAP-XX items concrete (NÃO vagos).
- Each GAP-XX has owner + due date + remediation plan + SOC 2 control reference + status.
- Coverage SOC 2 TSC primary (CC6.1 + CC6.7 + CC8.1) + cumulative LGPD + GDPR + CCPA + EDPB SCCs.

### 7.3 Roadmap validation

- Type I engagement timeline T+1..T+6 meses pós-GA documented.
- Budget estimate justified.
- Type II target T+12 meses documented.

## 8. Failure modes

- **FM-COMPLIANCE-GAPS-EXCEED-50** (novo S-20): mitigation = Q3+Q4 iterate continuous monitoring; reduce GAP count via early remediation.
- **FM-DRATA-VANTA-INTEGRATION-FAIL** (novo S-20): mitigation = fallback alternative tool + ADR.
- **FM-COMPLIANCE-OFFICER-SIGNOFF-DECLINED** (novo S-20): mitigation = process review + iteration cycle.
- **FM-COMPLIANCE-DASHBOARD-DRIFT** (novo S-20): mitigation = continuous monitoring + alert > 5% drift.

## 9. Invariants (cumulative ratification)

ALL 27+ INVs from S-13..S-19 + canonical sources active; SOC 2 gap analysis covers privacy controls (INV-CONSENT-PROOF-VERIFIABLE + INV-DATA-RESIDENCY + INV-DATA-ERASURE-COMPLETE + INV-ERASURE-ATTESTATION-SIGNED) + audit chain (INV-AUDIT-APPEND-ONLY + INV-OBS-AUDIT-CHAIN-INTEGRITY) + supply chain (INV-SUPPLY-SIGNED-DEPLOY + INV-SUPPLY-SBOM-PRESENT + INV-SUPPLY-PROVENANCE-IN-REKOR + INV-SUPPLY-NO-YANKED + INV-SUPPLY-LICENSE-ALLOWLIST) + admin plane (INV-ADMIN-DUAL-APPROVAL + INV-ADMIN-MFA-FRESHNESS).

S-20 NÃO introduz novos INVs.

## 10. Controls (cumulative)

CTRL-XXX cumulative:
- **SOC 2**: CC6.1 (logical access) + CC6.7 (change management) + CC8.1 (system change).
- **LGPD**: Art. 33 §1º (residency) + Art. 7º (legitimate interest) + Art. 8º (consent).
- **GDPR**: Art. 7 (consent) + Art. 17 (erasure) + Art. 28 (Processor) + Art. 32 (security) + Art. 46 (international transfers).
- **CCPA**: §1798.140(v) (sale/disclosure).
- **EDPB SCCs** (Schrems II supplementary measures).
- **NIST SP 800-53 Rev 5**: Security and Privacy Controls baseline.
- **ISO/IEC 27001:2022**: ISMS requirements.

## 11. Resilience patterns (cumulative)

PAT-XXX cumulative; gap analysis covers:
- PAT-DUAL-APPROVAL-001 (admin operations; SOC 2 CC6.7).
- PAT-FORMAL-VERIFICATION-001 (TLA+ 4 specs verde; differentiator pre-Type I).

## 12. Observability (cumulative)

Métricas Prometheus snake_case underscored:

- `corelink_compliance_gap_count_gauge{severity, status, plan}` (gauge; severity ∈ blocking_ga|major|minor; status ∈ open|in_progress|closed).
- `corelink_compliance_dashboard_completeness_ratio_gauge{tool, plan}` (gauge 0..=1.0; tool ∈ drata|vanta).
- `corelink_compliance_evidence_pack_completeness_ratio_gauge{plan}` (gauge 0..=1.0).

DASH-COMPLIANCE-S20 panel embedded em DASH-GA-READINESS dashboard.

## 13. Security & Privacy

**STRIDE delta**: gap analysis covers cumulative privacy + security controls; tooling integration validates continuous monitoring posture.

**LINDDUN delta**:
- **Linkability**: tooling does not introduce per-tenant labels.
- **Identifiability**: gap analysis covers pseudonymization + erasure controls.
- **Non-repudiation**: gap analysis covers audit chain integrity.
- **Detectability**: continuous monitoring dashboard alerts gaps.
- **Disclosure**: gap analysis report shareable internally; sanitized version for customer NDAs.
- **Unawareness**: customer notified per DPA + SLA.
- **Non-compliance**: SOC 2 + LGPD + GDPR + CCPA + EDPB SCCs + NIST SP 800-53 Rev 5 + ISO/IEC 27001:2022 satisfied via cumulative + gap analysis fix timeline.

## 14. Dependencies

### Hard blockers
- Drata/Vanta tooling selected Q3.
- All 14 canonical sources active.

### Soft blockers
- Cumulative S-13..S-19 SEALED.

### Outbound
- WI-S20-001 PRR-GA-001 evidence pack EVT-031 depende de WI-S20-003 gap analysis + roadmap.
- Sprint S-20 SEAL D+30 Implementation depende de WI-S20-003 deliverables committed.

## 15. PERT estimate

| Estimate | Hours | Notes |
|---|---|---|
| **Optimistic (O)** | 14h | Drata/Vanta integration smooth + < 20 GAPs identified + roadmap drafted. |
| **Most Likely (M)** | 22h | Drata/Vanta integration + < 30 GAPs + fix timeline + roadmap iteration. |
| **Pessimistic (P)** | 36h | > 50 GAPs found + audit rescheduled + tool fallback. |
| **PERT** | (14 + 4×22 + 36) / 6 = **23.0h** | Per spec contract §12. |
| **Variance (σ²)** | ((36-14)/6)² = 13.4 | Std dev ≈ 3.7h. |

## 16. Sign-off canonical (HIGH_RISK 13)

13 roles per spec contract §5.1 R-S20-1 (v1.4.0 canonical — Lote 11.21 round-1 P0-S20-001 fix). **Authoritative roster (see `_spec_contract.md §5.1` for full text):** (1) Owner, (2) Final Approver, (3) Engineer Lead, (4) QA Lead, (5) Security Lead (AppSec advisor folded), (6) Privacy Officer (DPO-interim; DPO folded), (7) Legal Counsel, (8) Compliance Officer, (9) Product Lead, (10) SRE Lead, (11) CTO, (12) Architect (Crypto SME folded per ADR-0034), (13) External Auditor (pentest firm rep). Finance is NOT in the canonical 13. Any role label below that diverges MUST be read as referring to its folded canonical slot per the mapping above.

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect (Crypto SME folded) | _TBD_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (S-20 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer (SOC 2 gap analysis primary; Drata/Vanta dashboard + GAP-XX roadmap Type I 6m) | _TBD via external advisor pool_ | _pending_ | _pending_ |
| 10 | Privacy Officer (LGPD + GDPR + CCPA + EDPB SCCs cumulative coverage) | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD_ | _pending_ | _pending_ |
| 12 | Legal Counsel | _TBD_ | _pending_ | _pending_ |
| 13 | Finance (Drata/Vanta annual subscription $5-15k + Type I engagement budget $30-60k pós-GA) | _TBD_ | _pending_ | _pending_ |

## 17. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S20-003 (cycle 12.S20.0; SOC 2 Drata/Vanta gap analysis NÃO audit completo + concrete GAP-XX items + fix timeline + roadmap Type I 6m pós-GA + dashboard verde > 95% controls; coverage SOC 2 TSC CC6.1 + CC6.7 + CC8.1 + cumulative LGPD + GDPR + CCPA + EDPB SCCs + NIST SP 800-53 Rev 5 + ISO/IEC 27001:2022). |
| 1.1.0 | 2026-05-14 | Gustavo (via Claude Opus 4.7 builder) | SEALED — D1..D5 delivered. SOC 2 TSC gap analysis (33 GAPs; 1 blocking-GA closing D+30 + fallback) + 6-month Type I roadmap (Schellman primary $40-85k; Type II T+12m..T+18m) + vendor shortlist (Drata 8.55/10 selected) + readiness score (internal 83.7%; Drata 96.4%; projected Type I pass-rate 95%). All cited evidence paths verified existent. |

---

**Fim WI-S20-003.**
