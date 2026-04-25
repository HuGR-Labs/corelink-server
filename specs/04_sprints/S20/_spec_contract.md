---
id: "SPEC-CONTRACT-S20"
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
tags: ["spec-contract", "s20", "ga", "readiness", "prr", "external-pentest", "soc-2", "lighthouse-customers", "high-risk", "sota-v1.1"]
---

# Spec Contract — S-20: GA Readiness (PRR Global + 30d Sustained Staging + External Pentest + Lighthouse Customers + Engineering/Launch Split)

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-20 |
| Nome | GA Readiness |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-005 (controle final), FF-HR-009 (contratos com customers vão production), FF-HR-010 (primeira entrega regulatory live em escala) |
| Duração estimada | 4 semanas |
| WIs antecipados | 8 |
| SOTA target | GA gate definitive — PRR global 14 canonical sources + external pentest clean + 30d sustained staging + 3 lighthouse customers + Engineering/Launch separation |

## 1. Objetivo

**Última milha pra General Availability**: PRR global (todos os 14 canonical sources verdes), pentest externo completo (Schellman ou A-LIGN), **30d sustained staging** (anti-scope: chaos S-17 já roda 4w; S-20 stabilization 30d adicional), SOC 2 gap analysis preliminar, customer reference beta (3 lighthouse customers migrados — 2 team tier + 1 enterprise BYOK), SLA contratual standardized, marketing prep separado de engineering gate. **Pivot crítico:** S-20 separa **GA Engineering Gate** (technical readiness) de **Launch Orchestration** (marketing/PR/Product Hunt) per codex feedback.

**Por que SOTA:** competitors lançam GA com pentest sem retest, sem 30d sustained, sem 3 lighthouse customers. CoreLink S-20 entrega: (a) external pentest com retest pós-remediação; (b) 30d sustained staging com 0 SEV-1 + < 3 SEV-2 not resolved; (c) 3 lighthouse customers com SLA claim met em 30d; (d) SOC 2 gap analysis (Drata/Vanta) com concrete GAP-XX items + fix timeline; (e) **Engineering/Launch separation** — engineering gate é binary (DoD complete) e launch é orchestration separada.

**Codex findings remediated:**
- "10 canonical sources" → **14 canonical sources** (correto count alinhado §3).
- "Full SBOM v1.0" → **CycloneDX 1.5+** (alinhado S-12 R-S12-3).
- "30d gates impossible em 3 semanas" → duração **4 semanas** + buffer 10d + observation period chaos S-17 concurrent.
- Marketing/Product Hunt separated em §4 CAP-LAUNCH separated from CAP-GA.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK. Qualquer miss aqui = delay GA.
- **FF-HR-005, FF-HR-009, FF-HR-010** — todos os 3 ativos.

## 3. Inherits_from

```yaml
inherits_from:
  - "FRAMEWORK-00"
  - "SECURITY-MODEL"
  - "PRIVACY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "SLO-CATALOG"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "DATA-MODEL"
  - "STORAGE-SEMANTICS-MATRIX"
  - "AUTH-MODEL"
  - "KEY-MANAGEMENT"
  - "COMPLIANCE-MATRIX"
  - "INVARIANT-REGISTRY"
  - "REMOTE-CACHE-PRODUCT-PROFILE"
```

**14 canonical sources** (count corrigido per codex finding).

## 4. CAPs entregues

| ID | Capability | Detalhe |
|---|---|---|
| **CAP-GA-001** | GA readiness (engineering gate) | All boxes checked em roadmap.md; PRR global APPROVED. |
| **CAP-GA-002** | External pentest report clean | Schellman ou A-LIGN; report + retest; HIGH/CRITICAL remediated; EVT-040. |
| **CAP-GA-003** | SOC 2 gap analysis | Drata/Vanta gap analysis (não audit completo); concrete GAP-XX items + fix timeline; preparation for Type I engagement pós-GA 6 meses. |
| **CAP-GA-004** | 3 lighthouse customers migrated + attestations | 2 team tier + 1 enterprise com BYOK; SLA claim met em 30d. |
| **CAP-GA-005** | SLAs contratuais publicados + DPA v1 ready | Published SLA terms; DPA v1 signed com 3 lighthouse customers. |
| **CAP-GA-006** | Incident response 24/7 ready | PagerDuty schedule cobrindo 24/7 em 3 regiões; response < 5 min testada. |
| **CAP-LAUNCH-001** | Launch orchestration prep | Press release + blog posts + case studies + Product Hunt launch prep — **separated from engineering gate**; managed by Marketing + PR. |

## 5. Requirements específicos

### 5.1 Engineering Gate (CAP-GA-001..006)

- **R-S20-1**: **Global PRR** (`PRR-GA-001`) seguindo `_templates/production_readiness_review.md` com lane HIGH_RISK; 10–12 sign-offs:
  - SRE lead + Security lead + Privacy officer + DPO interim + Compliance officer + Engineer + QA + Product + Architect + AppSec + Crypto SME (BYOK) + Finance (billing).
- **R-S20-2**: External pentest engagement:
  - Hire Schellman ou A-LIGN (scope: full CoreLink production-equivalent staging environment).
  - Pentest duration: 2 semanas + retest 1 semana.
  - Report + retest; zero HIGH/CRITICAL findings pending.
  - EVT-040 evidence captured.
- **R-S20-3**: SOC 2 Type I gap analysis:
  - Tooling: Drata ou Vanta integration (continuous compliance monitoring).
  - Não é audit completo — é rehearsal + GAP-XX identification.
  - Output: roadmap pra Type I engagement em 6 meses pós-GA.
- **R-S20-4**: 3 lighthouse customers migration:
  - **2 team tier**: Forge (customer-zero) + 1 external OSS project (Bazel/Buck2 ecosystem).
  - **1 enterprise** com BYOK + DPA: TBD via Sales engagement.
  - Migration plan + 30d observation + SLA claim met em 30d.
  - Customer attestations (testimonial + case study).
- **R-S20-5**: SLA doc published em `legal/sla/v1.md` + `docs.corelink.dev/sla`; DPA v1 signed com 3 lighthouse customers.
- **R-S20-6**: Incident response 24/7:
  - PagerDuty schedule live em 3 regiões (US/EU; APAC eventual scaling).
  - On-call manager + escalation matrix.
  - Response < 5 min testadas via synthetic page weekly.
- **R-S20-7**: 30d sustained staging:
  - Concurrent ao S-17 chaos automation (4 weeks chaos already running).
  - Adicional 30d: zero SEV-1 + < 3 SEV-2 not resolved.
  - All SLOs sustained: SLO-AVAIL-CAS-PUT, SLO-AVAIL-CAS-GET, SLO-LAT-CAS-GET, SLO-FRESH-DSR-ERASURE, SLO-FRESH-BILLING.

### 5.2 Launch Orchestration (CAP-LAUNCH-001 — separated)

- **R-S20-8**: Marketing launch prep (separado de engineering gate; managed by Marketing + PR):
  - Press release reviewed por PR + Legal.
  - Blog posts ≥ 5 (deep dives technical + business).
  - Case studies dos 3 lighthouse customers.
  - Product Hunt launch prep (assets + outreach).
- **R-S20-9**: **Engineering gate é binary** (DoD complete = GA-go). Launch orchestration **não bloqueia** engineering gate; pode shift launch date sem touching engineering readiness.

## 6. Definition of Done

### 6.1 Engineering Gate (binary)

- [ ] **Engineering Gate WIs SEALED**: 7/7 (WI-S20-001..007). **WI-S20-008 (launch orchestration) NÃO bloqueia engineering gate** — tracked separately em §6.2 launch orchestration.
- [ ] **PRR global APPROVED** com zero CONDITIONALLY_APPROVED sub-items (EVT-031).
- [ ] **External pentest report clean**: zero HIGH/CRITICAL findings pending; retest passed (EVT-040).
- [ ] **30d sustained staging**: zero SEV-1; < 3 SEV-2 not resolved (concurrent observation period; documented post-S-17 chaos automation 4-week period).
- [ ] **3 lighthouse customers** com SLA claim met em 30d (1 team beta + 1 OSS + 1 enterprise BYOK) (EVT-018).
- [ ] **SOC 2 gap analysis delivered** (não audit; é roadmap) — concrete GAP-XX items + fix timeline (EVT-031).
- [ ] **Oncall schedule** rodando 24/7; PagerDuty responses < 5 min testadas weekly synthetic page (EVT-026).
- [ ] **All docs (S-18) complete + reviewed** (EVT-016 + EVT-018).
- [ ] **Compliance officer sign-off** (EVT-044).
- [ ] **All SLOs sustained 30d** prod-like load (concurrent staging) (EVT-021).
- [ ] **All 40 runbooks dry-run** executed em últimos 90d (cumulative S-17 + S-20) (EVT-017).
- [ ] **Zero active waivers em controles CRITICAL** (EVT-015 if exists; EVT-031 PRR).
- [ ] **TLA+ all 4 specs verde em CI**: tenant_isolation + cas_integrity + audit_immutability + gc_correctness (EVT-022).
- [ ] **SBOM CycloneDX 1.5+ signed (alinhado S-12 R-S12-3) signed published** (alinhado S-12 R-S12-3) (EVT-010).
- [ ] **Zero SEV-1 in prod in 30d prior to GA** (production-like staging observation).
- [ ] **All TLA+ specs verdes** em CI sustained.

### 6.2 Launch Orchestration (separated, soft-gate)

- [ ] Marketing ready: press release reviewed por PR + Legal.
- [ ] 5 blog posts published em staging blog.
- [ ] 3 case studies drafted com lighthouse customer testimonials.
- [ ] Product Hunt launch assets prepared.
- [ ] **Note**: launch orchestration não bloqueia engineering gate; engineering gate aprova GA-go independentemente.

## 7. Completeness Criteria (delta local)

- [ ] **10.s20.1** All SLOs sustained 30d prod-like load.
- [ ] **10.s20.2** All runbooks dry-run executed in last 90d.
- [ ] **10.s20.3** All CAP-XXX in roadmap delivered (roadmap coverage = 100%).
- [ ] **10.s20.4** Zero active waivers em controles CRITICAL.
- [ ] **10.s20.5** SOC 2 gap analysis identifies concrete GAP-XX items + fix timeline.
- [ ] **10.s20.6** **External pentest retest passed** (post-remediation verification).
- [ ] **10.s20.7** **3 lighthouse customers SLA claim met** sustained 30d.
- [ ] **10.s20.8** **Synthetic page weekly < 5 min response** sustained 30d.
- [ ] **10.s20.9** **Engineering gate vs Launch orchestration separated** documented em GA gate decision criteria.

## 8. Invariants

Todas as invariants CRITICAL (14 canonical sources contribute) **must be active**:

- **INV-TENANT-ISOLATION** (CRITICAL, TLA+ verified + pentest red-team tested).
- **INV-CAS-INTEGRITY** (CRITICAL, scrub + client verify).
- **INV-AUDIT-APPEND-ONLY** (CRITICAL, Object Lock + chain verify daily).
- **INV-GC-001 + INV-GC-004** (CRITICAL, TLA+ verified + 30d staging clean).
- **INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP** (CRITICAL, reconciliation < 0.1% drift).
- **INV-SUPPLY-SIGNED-DEPLOY + INV-SUPPLY-SBOM-PRESENT + INV-SUPPLY-PROVENANCE-IN-REKOR** (HIGH).
- **INV-DATA-RESIDENCY + INV-DATA-ERASURE-COMPLETE** (HIGH).
- **INV-CONSENT-PROOF-VERIFIABLE** (HIGH).
- **INV-OBS-CARDINALITY-BUDGET + INV-OBS-AUDIT-CHAIN-INTEGRITY** (HIGH).
- **INV-DEDUP-CONSISTENCY + INV-RATE-LIMIT-PROPORTIONALITY** (HIGH).
- **INV-BILLING-RECONCILE-3-LAYER + INV-BILLING-REPLAYABLE-FROM-EVENTS** (HIGH).
- **INV-BYOK-CRYPTO-SOVEREIGNTY + INV-REGION-NO-CROSS-LEAK + INV-ERASURE-ATTESTATION-SIGNED** (CRITICAL/HIGH).
- **INV-ADMIN-DUAL-APPROVAL + INV-ADMIN-MFA-FRESHNESS** (HIGH).
- **INV-ONBOARD-DPA-FIRST + INV-ONBOARD-ATOMIC-PROVISIONING** (HIGH).
- **INV-SUPPLY-NO-YANKED + INV-SUPPLY-LICENSE-ALLOWLIST** (HIGH).

## 9. Quality Standards (delta local)

- **SBOM CycloneDX 1.5+ signed (alinhado S-12 R-S12-3) signed published** — alinhado S-12 R-S12-3.
- **All 40 runbooks dry-run tested in 90d** (cumulative S-17 + S-20 cadence).
- **Zero SEV-1 in prod in 30d prior to GA** (production-equivalent staging — não há prod até GA day).
- **TLA+ all 4 specs verdes em CI** sustained.
- **External pentest retest passed** within 2 weeks of remediation submission.
- **3 lighthouse customers**: feedback collected + iteração; testimonials approved; case studies drafted.
- **Compliance posture**: Drata/Vanta dashboard verde > 95% controls; gaps documented com fix timeline.

## 10. Anti-scope

- ❌ Fase 2 (Remote Execution) — explicitly out of GA.
- ❌ Apache 2.0 open source release (pós-GA decision).
- ❌ SOC 2 Type I cert (6 meses pós-GA).
- ❌ FedRAMP Moderate baseline (pós-GA enterprise pivot).
- ❌ APAC region GA expansion (pós-GA Q1).
- ❌ Multi-cloud federation BYOK (pós-GA enterprise demand).
- ❌ AI-powered features — pós-GA.
- ❌ Mobile app — pós-GA.
- ❌ **Marketing/Launch orchestration as engineering gate dependency** — separated; engineering gate é binary independente de marketing readiness.
- ❌ Pre-GA paying customers além de 3 lighthouse — closed beta only at S-20.

## 11. Dependencies

### Hard blockers

- **S-00 a S-19 todos SEALED**.

### Soft blockers

- **S-17 chaos automation** running concurrent (4 weeks chaos pré-S-20 já started).

## 12. WIs antecipados (PERT)

| ID | Título | Sub-tasks | O | M | P | PERT |
|---|---|---|---|---|---|---|
| **WI-S20-001** | PRR global preparation + orchestration + 14 canonical sources sign-offs | PRR doc; sign-off coordination 14 canonical sources; 10-12 reviewers | 16h | 24h | 38h | **25.0h** |
| **WI-S20-002** | External pentest engagement + 2-week test + 1-week retest + remediation | engage Schellman/A-LIGN; scope; test; remediation; retest; report | 24h | 40h | 60h | **40.7h** |
| **WI-S20-003** | SOC 2 gap analysis (Drata/Vanta) + GAP-XX roadmap | Drata setup; gap analysis; GAP-XX list; fix timeline; report | 14h | 22h | 36h | **23.0h** |
| **WI-S20-004** | 3 lighthouse customer migration + 30d observation + attestations | engage 3 customers; migration plan; 30d obs; testimonial; case study | 24h | 40h | 64h | **41.3h** |
| **WI-S20-005** | SLA + DPA finalization + Legal review + signed lighthouse | SLA v1; DPA v1 final; Legal review; 3 customer signing | 12h | 18h | 30h | **19.0h** |
| **WI-S20-006** | Incident response 24/7 ready + PagerDuty 3 regions + synthetic weekly | PD schedule; escalation matrix; synthetic page test weekly; oncall training | 10h | 16h | 26h | **16.7h** |
| **WI-S20-007** | All TLA+ green CI + 30d staging sustained + zero SEV-1 + 90d runbook coverage | TLA+ CI gate; 30d obs setup; SEV tracking; runbook completeness audit | 14h | 22h | 36h | **23.0h** |
| **WI-S20-008** | Marketing launch prep (separated; soft gate) — press release + blogs + Product Hunt | press release Legal review; 5 blog posts; case studies; PH assets — Marketing-led | 16h | 24h | 38h | **25.0h** |

**Total PERT:** ~213h ≈ 27 dias work × 1 eng (más Marketing co-led WI-008). Buffer 10 dias confere com 4 semanas (pentest findings podem gerar cascata).

## 13. Duração + Timeline

- **Duração:** 4 semanas (20 dias úteis) + buffer 10 dias.
- **Marcos:**
  - **D+5:** WI-001 SEALED (PRR doc draft) + WI-002 pentest started.
  - **D+10:** WI-003 SOC 2 gap delivered + lighthouse migration WI-004 started.
  - **D+14:** WI-005 SEALED (SLA + DPA Legal-reviewed) + lighthouse customer signing.
  - **D+18:** WI-006 SEALED (incident response 24/7).
  - **D+22:** Pentest retest done WI-002.
  - **D+25:** WI-007 SEALED (30d staging sustained gate) + lighthouse 30d observation done.
  - **D+28:** WI-008 launch prep done (separated from gate).
  - **D+30:** GA gate decision — engineering gate APPROVED → GA announcement OR delay if pending CRITICAL.

## 14. Critérios de promoção

- **DoD engineering gate complete**.
- **PRR global APPROVED**.
- **72h to 30d clean staging sustained** (zero SEV-1; < 3 SEV-2 unresolved).
- **Zero pending CRITICAL finding** (pentest + invariant + waivers).
- **3 lighthouse SLA met**.
- **→ GA ANNOUNCEMENT** (public launch) — gated por engineering gate; launch orchestration may follow on day 0 or staggered.

## 15. Riscos (registry expandido)

| Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação |
|---|---|---|---|---|---|---|
| **Pentest encontra CRITICAL** → delay GA | M | M | HIGH (mas é o objetivo do pentest) | M | MEDIUM | Buffer 10 dias para remediation; reputable firm; scope clear; retest plan documented. |
| **Lighthouse customer desiste mid-sprint** | M | M | MEDIUM | M | LOW | Engage 4-5 candidates; signed letter of intent Q3; alternative lined up. |
| **SOC 2 gap analysis revela > 50 gaps** | M | L | MEDIUM (audit rescheduled) | L | LOW | Drata/Vanta continuous monitoring early Q3 antes do sprint; iterate. |
| **Last-minute regression em staging** | M | M | HIGH | M | MEDIUM | 30d observation discipline; chaos automation S-17 running; immediate escalation. |
| **Legal review DPA atrasa** | M | M | MEDIUM | M | LOW | Engage Legal externo Q3; iterate during sprint; fallback DPA v1 simplified. |
| **Marketing pressure to launch antes engineering ready** | M | M | HIGH (technical debt + customer trust) | M | LOW | Engineering gate binary + separated from launch; CEO/Founder enforce gate. |
| **Lighthouse customer SLA claim miss** em 30d | M | M | HIGH | M | LOW | SLO targets achievable; 30d monitoring + SRE escalation; remediation flow. |
| **PagerDuty 24/7 schedule infeasible** com team size | M | L | MEDIUM | L | LOW | 3 regions com on-call manager rotation; prioritize coverage US + EU at GA; APAC followers. |
| **Pentest retest fails** após remediation | L | M | HIGH (delay GA) | L | MEDIUM | Track remediation early; retest scheduling buffer; multiple iteration if needed. |
| **All TLA+ green** breaks during pentest discovery | L | M | HIGH | L | LOW | TLA+ CI gate continuous; pentest finding may strengthen TLA+ scope. |
| **Compliance officer sign-off declined** | L | H | CRITICAL (delay GA) | M | LOW | Engage compliance officer Q3; iterate gaps; clear acceptance criteria upfront. |
| **30d sustained staging breaks** mid-sprint | M | M | HIGH | M | MEDIUM | Concurrent S-17 chaos automation provides safety net; immediate root-cause + fix; potential delay 1 week. |

## 16. Benchmarks SOTA externos

| Critério | AWS GA process | GCP GA process | Cloudflare Workers GA | **CoreLink target S-20** |
|---|---|---|---|---|
| External pentest pre-GA | Yes (multiple) | Yes (multiple) | Yes | **Yes — 1 firm + retest cycle** |
| 30d staging sustained | Yes | Yes | Yes | **Yes — concurrent chaos S-17 4w + 30d S-20** |
| 3 lighthouse customers attested | Many | Many | Yes | **Yes — 2 team + 1 enterprise BYOK** |
| SOC 2 gap analysis pre-GA | Yes (Type I) | Yes (Type I) | Yes (Type II) | **Yes — gap analysis + roadmap; Type I 6m pós-GA** |
| 24/7 oncall ready | Yes | Yes | Yes | **Yes — 3 regions PagerDuty < 5 min response** |
| TLA+ formal verification | Some services | Some services | Limited | **Yes — 4 specs verdes CI** |
| Engineering/Launch separation | Yes | Yes | Yes | **Yes — engineering gate binary independent** |
| SBOM CycloneDX signed | Yes | Yes | Yes | **Yes — CycloneDX 1.5+ aligned S-12** |
| 26 runbooks dry-run 90d | Yes | Yes | Yes | **Yes — cumulative S-17 + S-20** |

**Veredito SOTA:** S-20 v1.1 atinge feature parity com AWS/GCP GA processes em 9/9 dimensões; vantagem em TLA+ verification (rare in industry) + Engineering/Launch separation discipline.

## 17. References (RFCs, papers, standards)

- **Google SRE Book Ch 27** — Reliable Product Launches at Scale.
- **AWS Well-Architected Framework** — Operational Excellence Pillar.
- **NIST SP 800-53 Rev 5** — Security and Privacy Controls.
- **SOC 2 Trust Services Criteria** (TSC 2017).
- **OWASP Application Security Verification Standard (ASVS) v4.0.3**.
- **PCI DSS v4.0** — applicable se enterprise tier processes payment data.
- **ISO/IEC 27001:2022** — ISMS requirements.
- **Schellman & Co.** — SOC 2 + pentest provider.
- **Vanta + Drata** — continuous compliance platforms.

## 18. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc**:

- Pentest CRITICAL finding pós-GA → CRITICAL post-mortem + Security review.
- 30d staging breaks (SEV-1) → CRITICAL post-mortem + GA gate review.
- Lighthouse customer SLA breach → 5-Why + SRE escalation + customer outreach.
- PagerDuty response > 5 min sustained → post-mortem + oncall reinforce.
- Compliance officer sign-off declined → process review + gap fix iteration.
- Engineering gate bypassed por Marketing pressure → CRITICAL post-mortem + governance reinforce.
- TLA+ CI red → CRITICAL post-mortem + invariant scope review.

## 19. Waiver policy

S-20 **NÃO PODE** promover via waiver dos seguintes itens — todos são GA gate requirements:

- ❌ External pentest report clean (zero HIGH/CRITICAL pending) — security baseline.
- ❌ 30d sustained staging zero SEV-1 — operational baseline.
- ❌ 3 lighthouse customers SLA met — customer trust baseline.
- ❌ PRR global APPROVED — process baseline.
- ❌ All 40 runbooks dry-run em 90d — operational baseline.
- ❌ TLA+ all 4 specs verde em CI — formal verification baseline.
- ❌ SBOM CycloneDX 1.5+ signed published — supply chain baseline.
- ❌ Zero active waivers em controles CRITICAL — security baseline.

Itens waivable com 14-canonical-source sign-off + ADR + CEO/Founder approval:

- ⚠️ 3 lighthouse customers → 2 lighthouse customers se 1 desiste (com plan to add 1 within 60d pós-GA).
- ⚠️ SLA claim met em 30d → 21d com explicit acceptance.
- ⚠️ Apache 2.0 open source release timing → pós-GA discussion (anti-scope mantém).
- ⚠️ Marketing/Launch orchestration delay → independent of engineering gate; pode delay launch sin technical impact.

---

**Post-GA:** Sprint S-21+ começam Fase 2 (Remote Execution — `execute-action`, executor identity, sandbox runtime), abrindo novo ciclo de 10+ sprints.

**Fim spec contract S-20 v1.1.0 SOTA.**
