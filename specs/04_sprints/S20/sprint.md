---
id: "S-20"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-29"
updated: "2026-04-29"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005", "FF-HR-009", "FF-HR-010"]
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
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
tags: ["sprint", "s20", "ga", "readiness", "prr", "external-pentest", "soc-2", "lighthouse-customers", "high-risk"]
---

# Sprint S-20 — GA Readiness (PRR Global PRR-GA-001 14 Canonical Sources Cumulative Ratification + External Pentest Schellman/A-LIGN/Trail of Bits 2-Week + 1-Week Retest Zero HIGH/CRITICAL Pending + 30d Sustained Staging Zero SEV-1 < 3 SEV-2 All SLOs Sustained + 3 Lighthouse Customers Migrated 2 Team Tier Forge+OSS + 1 Enterprise BYOK 30d SLA Claim Met + SOC 2 Drata/Vanta Gap Analysis Concrete GAP-XX Roadmap 6-Month Type I Engagement + SLA Contractual v1 Published + DPA v1 Signed 3 Lighthouse + Incident Response 24/7 PagerDuty 3 Regions Synthetic Page Weekly < 5 min + ~25 of 47 P0/P1 Runbooks Dry-Run em 90d Cumulative S-17+S-20 + TLA+ 4 Specs Verde tenant_isolation+cas_integrity+audit_immutability+gc_correctness + SBOM CycloneDX 1.5+ Signed Published + 13 Sign-Offs Canonical Engineering Gate + Two-Phase SEAL D+30 Implementation + D+60 GA Evidence Gate + Engineering vs Launch Orchestration SEPARATED CANONICAL Engineering Gate Binary Soft-Gate Launch Marketing/PR Press Release+5 Blog Posts+3 Case Studies+Product Hunt Prep)

> **doc_status:** DRAFT · **lane:** HIGH_RISK · **Versão:** 1.0.0 · **2026-04-29**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (cycle 9.4 SOTA elevation; codex findings remediated — 14 canonical sources count corrigido + CycloneDX 1.5+ alinhado S-12 + duração 4 semanas + Marketing/Launch separated)

> **Phase boundary:** Fase 6 — última milha pré-General Availability. **FINAL sprint** ratificando cumulativamente todos os 14 canonical sources + invariants + controls + SLOs entregues em S-13..S-19; abre transição pós-GA pra Fase 2 (Remote Execution).
> **FF-HR-005+009+010 specific**: controle final pré-GA = qualquer miss aqui = delay GA inaceitável; contratos com customers vão production = legal exposure permanente; primeira entrega regulatory live em escala = SOC 2 + LGPD + GDPR + CCPA evidence pack auditor-grade obrigatório. Engineering gate failure = customer trust loss permanente + revenue blocked + competitor advantage gain.

---

## 1. Objetivo

Operacionalizar **última milha pré-General Availability** com qualidade enterprise-grade definitiva: PRR global `PRR-GA-001` ratificando cumulativamente **14 canonical sources** (FRAMEWORK-00, SECURITY-MODEL, PRIVACY-MODEL, OBSERVABILITY-MODEL, SLO-CATALOG, FAILURE-MODES, RESILIENCE-PATTERNS, DATA-MODEL, STORAGE-SEMANTICS-MATRIX, AUTH-MODEL, KEY-MANAGEMENT, COMPLIANCE-MATRIX, INVARIANT-REGISTRY, REMOTE-CACHE-PRODUCT-PROFILE) + external pentest 2-week + 1-week retest zero HIGH/CRITICAL pending (Schellman ou A-LIGN ou Trail of Bits firma engagement reuso S-14 WI-S14-009 pattern; staging production-equivalent scope full CoreLink) + 30d sustained staging zero SEV-1 + < 3 SEV-2 not resolved + all SLOs sustained (cumulative S-17 chaos automation 4w + S-20 30d adicional ~50d coverage pré-GA; sequential não overlap) + 3 lighthouse customers migrated (2 team tier Forge customer-zero + 1 OSS Bazel/Buck2 ecosystem + 1 enterprise BYOK TBD via Sales engagement; 30d observation post-migration + SLA claim met sustained = GA Evidence Gate criterion + customer attestations testimonial + case study) + SOC 2 Drata/Vanta gap analysis concrete GAP-XX items + roadmap pra Type I engagement em 6 meses pós-GA (NÃO audit completo; é tooling integration + GAP identification + fix timeline) + SLA contractual published em `legal/sla/v1.md` + `docs.corelink.dev/sla` + DPA v1 signed com 3 lighthouse customers + Legal review path externo + incident response 24/7 PagerDuty schedule live em 3 regiões (US/EU; APAC eventual scaling pós-GA Q1 demand-driven) + on-call manager + escalation matrix + synthetic page weekly < 5 min response sustained 30d + ~25 de 47 P0/P1 runbooks dry-run em 90d cumulative S-17+S-20 (per Lote 10.17 fix S-17 canonical math; NÃO all 47) + TLA+ 4 specs verde em CI sustained (tenant_isolation + cas_integrity + audit_immutability + gc_correctness; per spec contract §6.1) + SBOM CycloneDX 1.5+ signed published (alinhado S-12 R-S12-3) + zero active waivers em controles CRITICAL + launch orchestration **separada de engineering gate canonical** (per codex finding §1 + §6.2 spec contract): Marketing/PR managed press release Legal-reviewed + 5 blog posts deep dives technical+business + 3 case studies lighthouse customer testimonials + Product Hunt launch prep assets+outreach — soft-gate **NÃO bloqueia** engineering gate binary GA-go decision.

Decomposição cumulativa em **8 WIs separados em 2 buckets**: **Engineering Gate (WI-S20-001..007 binary GA-go blocking)**: (1) **WI-S20-001** PRR global `PRR-GA-001` orchestration + 14 canonical sources verification matrix + sign-off coordination 13 reviewers (11 canonical HIGH_RISK + 12th Legal Counsel carryover S-14+S-19 + 13th Finance billing review per spec contract §5.1); (2) **WI-S20-002** external pentest engagement Schellman/A-LIGN/Trail of Bits + 2-week pentest + 1-week retest + remediation buffer + zero HIGH/CRITICAL pending = SEAL gate hard (mirror WI-S14-009 §6 pentest engagement structure); (3) **WI-S20-003** SOC 2 Drata/Vanta gap analysis + concrete GAP-XX items + roadmap pra Type I engagement em 6 meses pós-GA (NÃO audit completo; tooling integration); (4) **WI-S20-004** 3 lighthouse customers migration (2 team tier Forge customer-zero + 1 OSS + 1 enterprise BYOK) + 30d observation post-migration + SLA claim met sustained + customer attestations testimonial + case study; (5) **WI-S20-005** SLA contractual published v1 + DPA v1 finalization + Legal review externo + 3 lighthouse customers signing; (6) **WI-S20-006** incident response 24/7 PagerDuty 3 regions + on-call manager + escalation matrix + synthetic page weekly < 5 min response sustained 30d; (7) **WI-S20-007** closing engineering gate — 30d sustained staging + zero SEV-1 + < 3 SEV-2 + all SLOs sustained + ~25 P0/P1 runbooks dry-run em 90d coverage cumulative S-17+S-20 + TLA+ 4 specs verde sustained CI + SBOM CycloneDX 1.5+ signed published + closing PRR-GA-001 13 sign-offs collected. **Launch Orchestration (WI-S20-008 soft-gate NÃO blocking)**: (8) **WI-S20-008** launch orchestration prep — press release reviewed por PR + Legal + 5 blog posts published em staging blog + 3 case studies drafted com lighthouse customer testimonials + Product Hunt launch assets+outreach Marketing-led; **separated from engineering gate canonical** per spec contract §1 codex finding + §6.2; engineering gate binary GA-go independente de marketing readiness. Mitiga **FM-XXX cumulative** (todas FMs S-13..S-19 ratificadas em 30d staging clean). Implementa **CTRL-XXX cumulative** (todos CTRLs canonical sources ratificados via PRR global). Ratifica **INV-XXX cumulative** (todos 27+ INVs canonical sources S-13..S-19 active + green em CI/property tests/30d staging).

**Por que SOTA**: competitors (AWS GA process, GCP GA process, Cloudflare Workers GA) lançam GA com pentest sem retest, sem 30d sustained, sem 3 lighthouse customers attested, sem TLA+ formal verification, sem Engineering/Launch separation discipline. CoreLink S-20 entrega: (a) **external pentest com retest pós-remediação** (zero HIGH/CRITICAL pending = SEAL gate hard); (b) **30d sustained staging** com zero SEV-1 + < 3 SEV-2 not resolved + all SLOs sustained (cumulative S-17 4w chaos + S-20 30d adicional = ~50d coverage pré-GA); (c) **3 lighthouse customers attested** com SLA claim met em 30d (2 team tier + 1 enterprise BYOK); (d) **SOC 2 gap analysis** Drata/Vanta tooling integration + concrete GAP-XX items + fix timeline + roadmap pra Type I 6 meses pós-GA; (e) **Engineering/Launch separation canonical** — engineering gate é binary (DoD complete = GA-go) e launch é orchestration separada Marketing-led; CEO/Founder enforces gate; (f) **TLA+ 4 specs verde em CI sustained** (rare in industry; AWS/GCP some services only); (g) **SBOM CycloneDX 1.5+ signed published** alinhado S-12. Reference: **Google SRE Book Ch 27** (Reliable Product Launches at Scale), **AWS Well-Architected Framework** (Operational Excellence Pillar), **NIST SP 800-53 Rev 5** (Security and Privacy Controls), **SOC 2 Trust Services Criteria** (TSC 2017), **OWASP ASVS v4.0.3**, **PCI DSS v4.0** (se enterprise tier processes payment data), **ISO/IEC 27001:2022** (ISMS), **Schellman & Co.** (SOC 2 + pentest provider), **Vanta + Drata** (continuous compliance platforms).

**Codex findings remediated** (per spec contract §1):
- "10 canonical sources" → **14 canonical sources** (correto count alinhado §3 spec contract).
- "Full SBOM v1.0" → **CycloneDX 1.5+** (alinhado S-12 R-S12-3).
- "30d gates impossible em 3 semanas" → duração **4 semanas** + buffer 10d + observation period chaos S-17 concurrent.
- Marketing/Product Hunt **separated em §4 CAP-LAUNCH separated from CAP-GA**; engineering gate binary independente.

## 2. Escopo

### 2.1 In-scope (Engineering Gate WIs 001..007 binary GA-go blocking)

- **WI-S20-001** (PRR global orchestration): PRR doc `specs/04_sprints/S20/PRR-GA-001.md` seguindo `_templates/production_readiness_review.md` lane HIGH_RISK; 14 canonical sources verification matrix (cada canonical source verified active + cumulative invariants ratified + cumulative controls enforced + cumulative SLOs sustained); 13 sign-off coordination (11 canonical HIGH_RISK + 12th Legal Counsel carryover S-14+S-19 + 13th Finance billing review per spec contract §5.1); evidence pack assembly (cumulative S-13..S-19 PRRs links + 30d staging metrics + lighthouse customer attestations + pentest report + SOC 2 gap analysis + DPA signed + SLA published + 24/7 oncall live + ~25 runbooks dry-run + TLA+ 4 specs verde + SBOM signed); promotion gate decision: `APPROVED` | `CONDITIONALLY_APPROVED` (com waivers + ADR + expiry; **CONDITIONALLY_APPROVED = block GA per spec contract §10.s20 binary**) | `REJECTED`.

- **WI-S20-002** (external pentest engagement): firm bid + selection (Schellman ou A-LIGN ou Trail of Bits competitive bid; budget $50-150k; 4-week lead time; engagement contract signed D+5); scope full CoreLink production-equivalent staging environment (não scope-limited como S-14 que era AWS KMS primary; S-20 é full system); pentest duration 2 semanas (D+10 to D+24) + retest 1 semana (D+24 to D+31) + remediation buffer 10 dias; methodology NIST SP 800-115 + OWASP Testing Guide v4 + targeted threat-modeling for full system; acceptance gate zero HIGH/CRITICAL findings pending = SEAL gate hard; HIGH findings remediated within buffer; MEDIUM/LOW deferred com explicit waiver + ADR; output `specs/_audits/2026-XX-XX-external-pentest-ga-report.md` (sanitized sharable + raw findings restricted) + remediation tracker; EVT-040 evidence captured.

- **WI-S20-003** (SOC 2 gap analysis): tooling Drata ou Vanta integration (continuous compliance monitoring; engage Q3 antes do sprint start; iterate during); **NÃO é audit completo** — é rehearsal + GAP-XX identification; output: gap analysis report `specs/_audits/2026-XX-XX-soc2-gap-analysis.md` com concrete GAP-XX items (e.g., GAP-01: missing access review quarterly cadence; GAP-02: encryption-at-rest documented but not attested; etc.) + fix timeline + roadmap pra Type I engagement em 6 meses pós-GA (firm engagement + 6-month observation period typical SOC 2 Type I); Drata/Vanta dashboard verde > 95% controls (gaps documented com fix timeline).

- **WI-S20-004** (3 lighthouse customers migration): engage 4-5 candidates pré-sprint (signed letter of intent Q3 antes de sprint start; alternative lined up se desistência) → finalize 3 lighthouse customers — **2 team tier**: Forge (customer-zero per spec contract §5.1; HuGR org primeira tenant ativada) + 1 external OSS project (Bazel/Buck2 ecosystem; reach via OSS maintainer outreach) + **1 enterprise** com BYOK + DPA: TBD via Sales engagement (ICP enterprise pre-revenue Q3); migration plan per-customer (signup + DPA signing + tier selection + first PAT + first CAS PUT + 30d observation period); 30d observation post-migration + SLA claim met em 30d sustained (SLO-LAT-CAS-GET p99 < 300ms + SLO-AVAIL-CAS-PUT ≥ 99.9% + SLO-AVAIL-CAS-GET ≥ 99.9% + SLO-FRESH-DSR-ERASURE ≤ 30 days + SLO-FRESH-BILLING ≤ 24h reconciliation); customer attestations: testimonial + case study (Marketing co-led com Customer Success); customer attestation reports `specs/_audits/2026-XX-XX-lighthouse-customer-{forge,oss,enterprise}-attestation.md`.

- **WI-S20-005** (SLA + DPA finalization): SLA doc `legal/sla/v1.md` + published em `docs.corelink.dev/sla` covering todos os SLOs canonical (SLO-CATALOG ratification cumulative); DPA v1 finalization + Legal review externo (Cooley/DLA Piper/Bird & Bird ~$15-30k 6-week lead reuso S-14 path); 3 lighthouse customers signing (DPA + sub-processor agreement + breach notification SLA + pricing addendum se enterprise); customer signed evidence em `specs/_audits/2026-XX-XX-dpa-signed-3-lighthouse.md`; ADR if waiver needed para 1 customer (tier downgrade ou DPA simplification); EVT-044 evidence captured.

- **WI-S20-006** (incident response 24/7): PagerDuty schedule live em 3 regiões (US Pacific + US Eastern + EU; APAC eventual scaling pós-GA Q1 demand-driven); on-call manager rotation (Owner + Final Approver dual-hat solo-tier per ADR-0034 Option A; OR contract-based escalation Tier-1 SRE engineering pré-GA hire); escalation matrix `specs/05_runbooks/RB-INCIDENT-ESCALATION-MATRIX.md` (P0 → page on-call → escalate manager 5 min → escalate CEO/Founder 15 min); response < 5 min testadas via synthetic page weekly (cron job creates synthetic SEV-2 incident; on-call must ack within 5 min p99); synthetic page weekly < 5 min response sustained 30d (GA Evidence Gate criterion); evidence em DASH-INCIDENT-RESPONSE.

- **WI-S20-007** (closing engineering gate): 30d sustained staging + zero SEV-1 + < 3 SEV-2 not resolved + all SLOs sustained (concurrent observation period documented post-S-17 chaos automation 4-week period; sequential coverage ~50d pré-GA); ~25 de 47 P0/P1 runbooks dry-run em 90d cumulative S-17+S-20 (per Lote 10.17 fix S-17 canonical math; priority subset NÃO all 47); TLA+ all 4 specs verde em CI sustained (tenant_isolation + cas_integrity + audit_immutability + gc_correctness); SBOM CycloneDX 1.5+ signed published (alinhado S-12 R-S12-3); closing PRR-GA-001 13 sign-offs collected; engineering gate decision binary GA-go.

### 2.2 In-scope (Launch Orchestration WI-008 soft-gate NÃO blocking)

- **WI-S20-008** (launch orchestration prep): press release reviewed por PR + Legal (subject: "CoreLink GA — multi-tenant content-addressable cache on Cloudflare; Bazel/Buck2/RBE-compatible Remote Cache; HuGR org launch"); 5 blog posts deep dives published em staging blog (`docs.corelink.dev/blog`): (1) Multi-tenant Cloudflare architecture; (2) BYOK enterprise tier + Schrems II compliance; (3) Customer-managed kill switch & erasure attestation; (4) TLA+ formal verification em production; (5) Lighthouse customers case studies); 3 case studies drafted com lighthouse customer testimonials (Forge + 1 OSS + 1 enterprise BYOK; Marketing co-led com Customer Success); Product Hunt launch prep (assets: hero image + tagline + 1-min demo video + maker comment template + outreach list) + Maker outreach scheduled D+25..D+30 launch day; **Marketing-led + PR-led; engineering gate binary independente** per spec contract §1 + §6.2 codex finding.

### 2.3 Anti-scope

- ❌ Fase 2 (Remote Execution — `execute-action`, executor identity, sandbox runtime) — explicitly out of GA; pós-GA Sprint S-21+ abre novo ciclo.
- ❌ Apache 2.0 open source release timing (pós-GA decision; anti-scope mantém).
- ❌ SOC 2 Type I cert (6 meses pós-GA; S-20 entrega gap analysis + roadmap apenas).
- ❌ FedRAMP Moderate baseline (pós-GA enterprise pivot; demand-driven).
- ❌ APAC region GA expansion (pós-GA Q1 demand-driven; 4 regions WNAM/ENAM/WEUR/SAM stable from S-14 sufficient at GA).
- ❌ Multi-cloud federation BYOK (pós-GA enterprise demand; single CMK per tenant at GA).
- ❌ AI-powered features — pós-GA (CoreLink scope é cache; AI = separate product).
- ❌ Mobile app — pós-GA (CoreLink CLI/SDK + Web admin sufficient).
- ❌ **Marketing/Launch orchestration as engineering gate dependency** — separated; engineering gate é binary independente de marketing readiness; CEO/Founder enforce gate.
- ❌ Pre-GA paying customers além de 3 lighthouse — closed beta only at S-20; expansion post-GA.
- ❌ External (third-party) compliance audit completo SOC 2 Type I — defer 6 meses pós-GA.
- ❌ Continuous fuzzing platform (cargo-fuzz) production deployment — pós-GA Q1.
- ❌ Bug bounty program — pós-GA Q1.
- ❌ Multi-DPO escalation workflow — pós-GA enterprise.
- ❌ Customer-facing compliance dashboard UI — pós-GA enterprise.

## 3. Customer Impact & Journey

**JTBD:** "Como CISO em prospect enterprise / DPO compliance auditor / Engineer evaluating CoreLink for Bazel/Buck2 RBE backend, preciso evidência verificável de que: (a) **CoreLink GA é production-grade** — external pentest clean (zero HIGH/CRITICAL pending) + 30d sustained staging zero SEV-1 + 3 lighthouse customers SLA claim met em 30d + SOC 2 gap analysis Drata/Vanta concrete GAP-XX items + roadmap Type I; (b) **Engineering gate é binary** (DoD complete = GA-go) e launch é orchestration separada — CEO/Founder enforce gate; (c) **incident response 24/7 ready** — PagerDuty schedule 3 regions + synthetic page weekly < 5 min sustained 30d; (d) **all SLOs sustained 30d** com evidence pack auditor-grade (DASH-* dashboards + métricas underscored snake_case + cumulative invariants ratified); (e) **all 14 canonical sources** verified + ratified cumulatively em PRR-GA-001 com 13 sign-offs canonical (11 HIGH_RISK + 12th Legal + 13th Finance). Como compliance auditor SOC 2 Type II futuro + ISO 27001 + GDPR DPO, preciso attestation que: TLA+ 4 specs verde em CI sustained (tenant_isolation + cas_integrity + audit_immutability + gc_correctness); SBOM CycloneDX 1.5+ signed published; ~25 of 47 P0/P1 runbooks dry-run em 90d coverage cumulative S-17+S-20; zero active waivers em controles CRITICAL; DPA v1 signed com 3 lighthouse customers; SLA contractual published; external pentest report sanitized sharable em sales conversations."

**CAPs entregues** (per spec contract §4):
- **CAP-GA-001** (GA readiness engineering gate): all boxes checked em roadmap.md; PRR global APPROVED.
- **CAP-GA-002** (External pentest report clean): Schellman/A-LIGN/Trail of Bits; report + retest; HIGH/CRITICAL remediated; EVT-040.
- **CAP-GA-003** (SOC 2 gap analysis): Drata/Vanta gap analysis (não audit completo); concrete GAP-XX items + fix timeline; preparation for Type I engagement pós-GA 6 meses.
- **CAP-GA-004** (3 lighthouse customers migrated + attestations): 2 team tier + 1 enterprise com BYOK; SLA claim met em 30d.
- **CAP-GA-005** (SLAs contratuais publicados + DPA v1 ready): published SLA terms; DPA v1 signed com 3 lighthouse customers.
- **CAP-GA-006** (Incident response 24/7 ready): PagerDuty schedule cobrindo 24/7 em 3 regiões; response < 5 min testada.
- **CAP-LAUNCH-001** (Launch orchestration prep): press release + blog posts + case studies + Product Hunt launch prep — **separated from engineering gate**; managed by Marketing + PR.

**Persona 1 — CISO em prospect enterprise (FedRAMP-ready / EU customer / financial services)**:
- Evidence pack inclui: external pentest report sanitized (zero HIGH/CRITICAL pending) shareable em sales conversations sob NDA; 30d sustained staging metrics dashboard; 3 lighthouse customer attestations testimonial + case study; SOC 2 gap analysis report + roadmap Type I 6m; SLA v1 contractual + DPA v1 signed.
- Diferenciador competitivo: AWS S3+KMS / GCS / Azure Blob / NativeLink GA processes some elements; CoreLink S-20 entrega **all 9 SOTA dimensions** (per spec contract §16 benchmarks externos): external pentest pre-GA + 30d staging sustained + 3 lighthouse customers attested + SOC 2 gap analysis + 24/7 oncall + TLA+ formal verification + Engineering/Launch separation + SBOM CycloneDX signed + 47 runbooks dry-run subset 90d.

**Persona 2 — DPO / Privacy Officer enterprise customer**:
- DPA v1 signed com 3 lighthouse customers = case study evidence; SOC 2 gap analysis Drata/Vanta = pre-Type I evidence pack; cumulative privacy controls ratified (CTRL-PRIV-001..031 cumulative) + LGPD Art. 33 + GDPR Art. 17/32/46 + CCPA + EDPB SCCs satisfied.

**Persona 3 — Engineer evaluating CoreLink for Bazel/Buck2 RBE backend**:
- `docs.corelink.dev/quickstart` + 5 blog posts technical deep dives + 3 case studies = decision support; SLA v1 contractual published = trust signal; TLA+ region_residency + cas_integrity + tenant_isolation + audit_immutability formal verification em CI = unique differentiator.

**Persona 4 — Compliance auditor (SOC 2 Type II + ISO 27001 + GDPR DPO)**:
- PRR-GA-001 doc 13 sign-offs canonical = audit trail evidence-grade; 14 canonical sources verification matrix + cumulative INVs ratified + cumulative CTRLs enforced; FIPS 140-3/140-2 documented per BYOK provider (S-14 deliverable); NIST SP 800-88 Rev.1 + NIST SP 800-57 Pt 1 Rev 5 + Schrems II TIA + DPA evidence cumulative.

**SLA addendum** (per spec contract §16 + cumulative SLO-CATALOG):
- All SLOs sustained 30d prod-like load: SLO-AVAIL-CAS-PUT ≥ 99.9%, SLO-AVAIL-CAS-GET ≥ 99.9%, SLO-LAT-CAS-GET p99 < 300ms, SLO-FRESH-DSR-ERASURE ≤ 30d, SLO-FRESH-BILLING ≤ 24h reconciliation < 0.1% drift.
- Zero SEV-1 in 30d prod-like staging.
- < 3 SEV-2 not resolved.
- Synthetic page weekly < 5 min response sustained 30d.
- 3 lighthouse customers SLA claim met em 30d sustained.
- ~25 of 47 P0/P1 runbooks dry-run executed em 90d cumulative S-17+S-20.
- TLA+ 4 specs verde em CI sustained.
- External pentest BYOK + full system clean (zero HIGH/CRITICAL pending).

## 4. Capability Mapping (trace)

Ver `_spec_contract.md §4`. Foundation cumulativa: `00_framework.md` (FRAMEWORK-00 governance) + `security_model.md` (SECURITY-MODEL CTRL-CRYPTO-* + CTRL-AUDIT-* + CTRL-AUTH-* + CTRL-KEY-*) + `privacy_model.md` (PRIVACY-MODEL CTRL-PRIV-* + LGPD + GDPR + CCPA) + `observability_model.md §3.1` (OBSERVABILITY-MODEL Prometheus snake_case + cardinality budget INV-OBS-CARDINALITY-BUDGET) + `slo_catalog.md` (SLO-CATALOG SLO-AVAIL-* + SLO-LAT-* + SLO-FRESH-*) + `failure_modes.md` (FAILURE-MODES FM-XXX cumulative) + `resilience_patterns.md` (RESILIENCE-PATTERNS PAT-* cumulative) + `data_model.md` (DATA-MODEL D1 + R2 + DO + KV schemas) + `storage_semantics_matrix.md` (STORAGE-SEMANTICS-MATRIX) + `auth_model.md` (AUTH-MODEL Clerk + WebAuthn + PAT + dual-approval) + `key_management.md §3.2.1` (KEY-MANAGEMENT overlap canonical per asset class) + `compliance_matrix.md` (COMPLIANCE-MATRIX SOC 2 + LGPD + GDPR + CCPA) + `invariant_registry.md §3.X` (INVARIANT-REGISTRY 27+ INVs cumulative) + `01_vision/remote_cache_product_profile.md` (REMOTE-CACHE-PRODUCT-PROFILE Bazel/Buck2/RBE alignment).

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S20-D1 | PRR global PRR-GA-001 doc + 14 canonical sources verification matrix + 13 sign-offs canonical | `specs/04_sprints/S20/PRR-GA-001.md` + `specs/04_sprints/S20/canonical-sources-verification-matrix.md` | PRR doc seguindo `_templates/production_readiness_review.md`; lane HIGH_RISK; 13 sign-offs (11 canonical + 12th Legal + 13th Finance); 14 canonical sources verified active + cumulative INVs ratified + cumulative CTRLs enforced + cumulative SLOs sustained; promotion gate decision binary APPROVED|REJECTED (CONDITIONALLY_APPROVED = block GA per §10.s20) |
| S20-D2 | External pentest engagement + report + retest clean | `specs/_audits/2026-XX-XX-external-pentest-ga-report.md` + remediation tracker | Schellman/A-LIGN/Trail of Bits firm engagement contract D+5; pentest 2 weeks D+10..D+24; retest 1 week D+24..D+31; remediation buffer 10 dias; zero HIGH/CRITICAL pending = SEAL gate hard; report sanitized sharable + raw findings restricted; EVT-040 |
| S20-D3 | SOC 2 gap analysis + GAP-XX roadmap Type I 6m | `specs/_audits/2026-XX-XX-soc2-gap-analysis.md` + `compliance/soc2-roadmap-type1.md` | Drata/Vanta tooling integration; concrete GAP-XX items (e.g., GAP-01..GAP-XX) + fix timeline; roadmap Type I engagement 6m pós-GA; Drata/Vanta dashboard verde > 95% controls; gaps documented |
| S20-D4 | 3 lighthouse customers migration + 30d observation + attestations | `specs/_audits/2026-XX-XX-lighthouse-customer-{forge,oss,enterprise}-attestation.md` + case studies | 2 team tier (Forge + 1 OSS) + 1 enterprise BYOK migrated; 30d observation post-migration; SLA claim met em 30d sustained; testimonials + case studies drafted; EVT-018 |
| S20-D5 | SLA contractual v1 published + DPA v1 signed 3 lighthouse | `legal/sla/v1.md` + `docs.corelink.dev/sla` + `legal/dpa/v1.md` + `specs/_audits/2026-XX-XX-dpa-signed-3-lighthouse.md` | SLA v1 covering todos os SLOs; published em legal/ + docs portal; DPA v1 Legal externo reviewed; 3 lighthouse customers signed; ADR if waiver needed; EVT-044 |
| S20-D6 | Incident response 24/7 PagerDuty 3 regions + escalation matrix + synthetic page weekly | `specs/05_runbooks/RB-INCIDENT-ESCALATION-MATRIX.md` + PagerDuty schedule live + DASH-INCIDENT-RESPONSE | PagerDuty schedule live 3 regions (US/EU); on-call manager + escalation matrix; synthetic page weekly < 5 min response sustained 30d; EVT-026 |
| S20-D7 | Engineering gate closing — 30d staging + ~25 runbook dry-run + TLA+ 4 verde + SBOM signed | `specs/_audits/2026-XX-XX-30d-staging-ga-evidence.md` + `specs/_audits/2026-XX-XX-runbook-coverage-90d-s17-s20.md` + TLA+ CI gate green + SBOM CycloneDX 1.5+ signed | 30d sustained staging zero SEV-1 + < 3 SEV-2; all SLOs sustained; ~25 of 47 P0/P1 runbooks dry-run em 90d cumulative; TLA+ 4 specs verde sustained CI (tenant_isolation + cas_integrity + audit_immutability + gc_correctness); SBOM CycloneDX 1.5+ signed published; zero active waivers em controles CRITICAL |
| S20-D8 | Launch orchestration prep (separated soft-gate) | `marketing/press-release-ga.md` + `marketing/blog-posts-5x.md` + `marketing/case-studies-3x.md` + `marketing/product-hunt-assets/` | Press release Legal-reviewed; 5 blog posts published em staging blog; 3 case studies drafted; Product Hunt assets prepared; Marketing-led; **NÃO blocking engineering gate** |

## 6. Escopo técnico por camada (inherits_from)

### 6.1 Framework-00 (herda `00_framework.md`)

- Governance cumulative ratification: PRINC-001..006 ratified; lane HIGH_RISK enforced; sign-off cardinality 13 canonical; framework §33.5.4.3 + ADR-0034 solo-tier waiver Option A applicable se Tier-1 staffing gap.
- PRR template `_templates/production_readiness_review.md` followed para PRR-GA-001 (escalado para HIGH_RISK GA gate; binary promotion APPROVED|REJECTED).
- 14 canonical sources verification matrix per §4 spec contract.

### 6.2 Security Model (herda `security_model.md`)

- All CTRL-CRYPTO-* + CTRL-AUDIT-* + CTRL-AUTH-* + CTRL-KEY-* enforced cumulative S-13..S-19 ratified em PRR-GA-001.
- External pentest validates security baseline holistically (NÃO scope-limited; full CoreLink production-equivalent staging).
- Zero active waivers em controles CRITICAL (security baseline gate; per spec contract §10.s20.4).

### 6.3 Privacy Model (herda `privacy_model.md`)

- All CTRL-PRIV-001..031 enforced cumulative S-11/S-13/S-14/S-19 ratified.
- LGPD Art. 33 + GDPR Art. 17 + Art. 32 + Art. 46 + CCPA + EDPB SCCs satisfied.
- DPA v1 signed com 3 lighthouse customers; Schrems II TIA template applicable enterprise.
- SOC 2 gap analysis Drata/Vanta covers privacy controls.

### 6.4 Observability Model (herda `observability_model.md §3.1`)

- Prometheus snake_case underscored; cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (≤ 20k séries únicas per métrica; ≤ 100k total; **NUNCA per-tenant labels**).
- DASH-* cumulative S-09 dashboards-as-code committed; 30d staging metrics validated per dashboard.
- Métrica nova S-20: `corelink_ga_readiness_gate_status_gauge{gate, phase, plan}` (gauge; gate ∈ engineering|launch; phase ∈ implementation|ga_evidence; values blocked|pending|approved|rejected).

### 6.5 SLO Catalog (herda `slo_catalog.md`)

- All SLOs sustained 30d prod-like load (concurrent staging + chaos S-17 4w + S-20 30d adicional ~50d coverage):
  - **SLO-AVAIL-CAS-PUT ≥ 99.9%** sustained 30d.
  - **SLO-AVAIL-CAS-GET ≥ 99.9%** sustained 30d.
  - **SLO-LAT-CAS-GET p99 < 300ms** sustained 30d.
  - **SLO-FRESH-DSR-ERASURE ≤ 30 days** sustained 30d.
  - **SLO-FRESH-BILLING ≤ 24h reconciliation < 0.1% drift** sustained 30d.
  - **SLO-INCIDENT-RESPONSE-SYNTHETIC-PAGE p99 < 5 min** sustained 30d (S-20 novo).
- All SLOs validated em DASH-* dashboards.

### 6.6 Failure Modes (herda `failure_modes.md`)

- All FMs cumulative S-13..S-19 ratified em 30d staging clean (zero SEV-1; < 3 SEV-2 not resolved).
- ~25 of 47 P0/P1 runbooks dry-run em 90d coverage cumulative S-17+S-20 (per Lote 10.17 fix S-17 canonical math; priority subset NÃO all 47).

### 6.7 Resilience Patterns (herda `resilience_patterns.md`)

- All PATs cumulative S-13..S-19 ratified: PAT-REGION-FAILOVER-001 (S-14), PAT-ROLL-FORWARD-001 (S-13), PAT-PROGRESSIVE-ROLLOUT-001 (S-13), PAT-AUTO-ROLLBACK-001 (S-13), PAT-FORMAL-VERIFICATION-001 (TLA+ 4 specs verde em CI sustained), PAT-DUAL-APPROVAL-001 (S-13 admin), PAT-SAGA-ATOMIC-001 (S-19 enterprise handoff).

### 6.8 Data Model + Storage Semantics + Auth Model + Key Management (herda canonical sources)

- DATA-MODEL: D1 + R2 + DO + KV schemas cumulative ratified em 30d staging clean.
- STORAGE-SEMANTICS-MATRIX: read-after-write + dedup + retention cumulative ratified.
- AUTH-MODEL: Clerk + WebAuthn UV=1 + PAT + dual-approval cumulative ratified.
- KEY-MANAGEMENT: overlap canonical per asset class (5 classes per ADR-0018; BYOK CMK 7d + Ed25519 attestation 30d + DEK cache TTL 5min hard + Cosign 90d + JWT signing 7d) cumulative ratified.

### 6.9 Compliance Matrix (herda `compliance_matrix.md`)

- SOC 2 + LGPD + GDPR + CCPA cumulative ratified em PRR-GA-001.
- Compliance officer sign-off (canonical sign-off #9) requires evidence pack complete.
- SOC 2 Drata/Vanta gap analysis concrete GAP-XX items + roadmap Type I 6m pós-GA.

### 6.10 Invariant Registry (herda `invariant_registry.md`)

Cumulative ratification — ALL 27+ INVs from S-13..S-19 + canonical sources active:

- **CRITICAL**: INV-TENANT-ISOLATION (TLA+ verified + pentest red-team tested), INV-CAS-INTEGRITY (scrub + client verify + TLA+ verified), INV-AUDIT-APPEND-ONLY (Object Lock + chain verify daily + TLA+ verified), INV-GC-001 + INV-GC-004 (TLA+ verified + 30d staging clean), INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP (reconciliation < 0.1% drift), INV-BYOK-CRYPTO-SOVEREIGNTY (S-14; chaos drill weekly), INV-REGION-NO-CROSS-LEAK (S-14; 30k property test 0 leaks), INV-CONSENT-PROOF-VERIFIABLE (S-11/S-19; round-trip ≥ 99.9%).
- **HIGH**: INV-SUPPLY-SIGNED-DEPLOY + INV-SUPPLY-SBOM-PRESENT + INV-SUPPLY-PROVENANCE-IN-REKOR (S-12), INV-DATA-RESIDENCY + INV-DATA-ERASURE-COMPLETE (S-11/S-14), INV-OBS-CARDINALITY-BUDGET + INV-OBS-AUDIT-CHAIN-INTEGRITY (S-09), INV-DEDUP-CONSISTENCY + INV-RATE-LIMIT-PROPORTIONALITY (S-06/S-08), INV-BILLING-RECONCILE-3-LAYER + INV-BILLING-REPLAYABLE-FROM-EVENTS (S-10), INV-ERASURE-ATTESTATION-SIGNED (S-14), INV-ADMIN-DUAL-APPROVAL + INV-ADMIN-MFA-FRESHNESS (S-13), INV-ONBOARD-DPA-FIRST + INV-ONBOARD-ATOMIC-PROVISIONING (S-19), INV-SUPPLY-NO-YANKED + INV-SUPPLY-LICENSE-ALLOWLIST (S-12), INV-KEY-NO-SKIP + INV-KEY-OVERLAP (S-13).

S-20 **NÃO introduz novos INVs** (cumulative ratification per spec contract §8); validators (`validate_inv_promotion.py`) clean.

### 6.11 Remote Cache Product Profile (herda `01_vision/remote_cache_product_profile.md`)

- Bazel/Buck2/RBE alignment ratified; OSS lighthouse customer validates ecosystem fit.
- 5 blog posts technical deep dives reference profile (CAP-LAUNCH-001 soft-gate).

## 7. Definition of Done (lane HIGH_RISK GA gate)

> **Two-phase SEAL** (per Timeline §9): items verificáveis instantaneamente fecham em **Implementation SEAL D+30**; items requerendo "30d sustained staging zero SEV-1" + "3 lighthouse customers SLA claim met sustained 30d" + "synthetic page weekly < 5 min response sustained 30d" + "all SLOs sustained 30d" janela observation MANDATORY fecham em **GA Evidence Gate D+60 = GA-GO**. Ambos SEALs canonicos; sprint considerado concluído apenas após GA Evidence Gate D+60.
>
> **Engineering Gate vs Launch Orchestration SEPARATION CANONICAL** per spec contract §1 codex finding + §6.2: WI-S20-008 launch orchestration é **soft-gate**, **NÃO blocking** engineering gate. Engineering gate (WI-S20-001..007) é binary GA-go decision; launch can shift date sem affecting engineering readiness.

### 7.1 Engineering Gate (binary; mandatory para GA-go)

- [ ] **Engineering Gate WIs SEALED**: 7/7 (WI-S20-001..007). **WI-S20-008 (launch orchestration) NÃO bloqueia engineering gate** — tracked separately em §7.2 launch orchestration (EVT-031).
- [ ] **PRR global APPROVED** com zero CONDITIONALLY_APPROVED sub-items (binary; CONDITIONALLY_APPROVED = block GA) (EVT-031).
- [ ] **External pentest report clean**: zero HIGH/CRITICAL findings pending; retest passed (EVT-040).
- [ ] **30d sustained staging**: zero SEV-1; < 3 SEV-2 not resolved (concurrent observation period; documented post-S-17 chaos automation 4-week period; sequential coverage ~50d) *(GA Evidence Gate D+60)*.
- [ ] **3 lighthouse customers** com SLA claim met em 30d (1 team Forge + 1 OSS Bazel/Buck2 + 1 enterprise BYOK) (EVT-018) *(GA Evidence Gate D+60)*.
- [ ] **SOC 2 gap analysis delivered** (não audit; é roadmap) — concrete GAP-XX items + fix timeline (EVT-031).
- [ ] **Oncall schedule** rodando 24/7; PagerDuty 3 regions; responses < 5 min testadas weekly synthetic page sustained 30d (EVT-026) *(GA Evidence Gate D+60)*.
- [ ] **All docs (S-18) complete + reviewed** (EVT-016 + EVT-018).
- [ ] **Compliance officer sign-off** (EVT-044).
- [ ] **All SLOs sustained 30d** prod-like load (concurrent staging) (EVT-021) *(GA Evidence Gate D+60)*.
- [ ] **~25 of 47 P0/P1 runbooks dry-run** executed em últimos 90d (cumulative S-17 + S-20; per Lote 10.17 fix S-17 canonical math) (EVT-017).
- [ ] **Zero active waivers em controles CRITICAL** (EVT-031 PRR).
- [ ] **TLA+ all 4 specs verde em CI**: tenant_isolation + cas_integrity + audit_immutability + gc_correctness (EVT-022) sustained.
- [ ] **SBOM CycloneDX 1.5+ signed published** (alinhado S-12 R-S12-3) (EVT-010).
- [ ] **Zero SEV-1 in prod in 30d prior to GA** (production-equivalent staging — não há prod até GA day) *(GA Evidence Gate D+60)*.
- [ ] **DPA v1 signed com 3 lighthouse customers** (EVT-044) *(GA Evidence Gate D+60)*.
- [ ] **SLA contractual v1 published** em `legal/sla/v1.md` + `docs.corelink.dev/sla` (EVT-044).
- [ ] **14 canonical sources verification matrix** committed em `specs/04_sprints/S20/canonical-sources-verification-matrix.md` com cumulative INVs ratified + cumulative CTRLs enforced + cumulative SLOs sustained (EVT-031).
- [ ] **All 27+ INVs cumulative active** (S-13..S-19 + canonical sources): INV-TENANT-ISOLATION + INV-CAS-INTEGRITY + INV-AUDIT-APPEND-ONLY + INV-GC-001/004 + INV-BILLING-NO-LOSS/NO-DUP + INV-BYOK-CRYPTO-SOVEREIGNTY + INV-REGION-NO-CROSS-LEAK + INV-CONSENT-PROOF-VERIFIABLE + INV-SUPPLY-* + INV-DATA-RESIDENCY/ERASURE-COMPLETE + INV-OBS-* + INV-DEDUP-CONSISTENCY + INV-RATE-LIMIT-PROPORTIONALITY + INV-BILLING-RECONCILE-3-LAYER/REPLAYABLE + INV-ERASURE-ATTESTATION-SIGNED + INV-ADMIN-DUAL-APPROVAL/MFA-FRESHNESS + INV-ONBOARD-DPA-FIRST/ATOMIC-PROVISIONING + INV-SUPPLY-NO-YANKED/LICENSE-ALLOWLIST + INV-KEY-NO-SKIP/OVERLAP (EVT-022).
- [ ] **10.s20.1** All SLOs sustained 30d prod-like load *(GA Evidence Gate D+60)*.
- [ ] **10.s20.2** All runbooks dry-run executed em last 90d (~25 of 47 P0/P1 priority subset).
- [ ] **10.s20.3** All **CAP-GA-XXX** delivered (CAP-GA-001..006 engineering gate scope = 100% coverage); **CAP-LAUNCH-001 EXPLICITLY EXCLUDED** from this criterion (Lote 10.20 codex P0 canonical fix — prior wording made WI-S20-008 launch indirectly blocking via this criterion; now engineering gate is truly binary independent of launch readiness).
- [ ] **10.s20.4** Zero active waivers em controles CRITICAL.
- [ ] **10.s20.5** SOC 2 gap analysis identifies concrete GAP-XX items + fix timeline.
- [ ] **10.s20.6** External pentest retest passed (post-remediation verification).
- [ ] **10.s20.7** 3 lighthouse customers SLA claim met sustained 30d *(GA Evidence Gate D+60)*.
- [ ] **10.s20.8** Synthetic page weekly < 5 min response sustained 30d *(GA Evidence Gate D+60)*.
- [ ] **10.s20.9** Engineering gate vs Launch orchestration separated documented em GA gate decision criteria.
- [ ] **PRR HIGH_RISK 13 sign-offs canonical** (per spec contract §5.1 + framework §33.5.4.3 + ADR-0034): Owner + Final Approver + Architect (Crypto SME folded para BYOK + KMS specialization per ADR-0034) + Security Lead + SRE Lead + Engineer + QA Lead + Product + Compliance Officer + Privacy Officer + AppSec advisor + 12th Legal Counsel (carryover S-14+S-19 legal-touching pattern) + 13th Finance (billing review per spec contract §5.1) (EVT-031).
- [ ] **Cost regression gate**: GA infra ≤ $1500/mês (4× R2 + 4× D1 + 4× DO + PagerDuty 3 regions + Drata/Vanta + Grafana cloud + Cloudflare Workers); pentest engagement $50-150k one-time; Legal externo $15-30k one-time.
- [ ] **Métricas underscored Prometheus**: cumulative S-09..S-19 30+ métricas + S-20 novas (synthetic page response + GA readiness gate status) emitting em staging com label `plan` aplicável (NUNCA per-tenant labels per INV-OBS-CARDINALITY-BUDGET) (EVT-013).

### 7.2 Launch Orchestration (separated, soft-gate, NÃO blocking)

- [ ] Marketing ready: press release reviewed por PR + Legal.
- [ ] 5 blog posts published em staging blog (`docs.corelink.dev/blog`).
- [ ] 3 case studies drafted com lighthouse customer testimonials.
- [ ] Product Hunt launch assets prepared + Maker outreach scheduled.
- [ ] **Note canonical**: launch orchestration **não bloqueia engineering gate**; engineering gate aprova GA-go independentemente; CEO/Founder enforce gate; launch pode shift date sem affecting engineering readiness.

## 8. Dependencies

### Hard blockers

- **S-00..S-19 todos SEALED** (per spec contract §11 hard blockers).
- **External pentest firm engagement contract signed** D+5.
- **DPA + SLA Legal externo review path engaged** (Cooley/DLA Piper/Bird & Bird ~$15-30k 6-week lead reuso S-14 path).
- **3 lighthouse customers signed letter of intent** Q3 antes de sprint start.
- **Drata/Vanta tooling integration** Q3 antes do sprint start.

### Soft blockers

- **S-17 chaos automation** running concurrent (4 weeks chaos pré-S-20 já started; sequential observation ~50d coverage pré-GA).

### Outbound

- **GA ANNOUNCEMENT** (public launch) — gated por engineering gate binary; launch orchestration may follow on day 0 ou staggered (independent canonical).
- **Sprint S-21+** começam Fase 2 (Remote Execution — `execute-action`, executor identity, sandbox runtime), abrindo novo ciclo de 10+ sprints pós-GA.

## 9. Timeline

- **Sprint kick-off**: D+0 (após S-19 SEALED + Drata/Vanta engaged Q3 + Legal externo path engaged + 4-5 lighthouse candidates LOI signed Q3).
- **D+5**: WI-S20-001 SEALED (PRR doc draft + 14 canonical sources verification matrix) + WI-S20-002 pentest started (firm engagement contract signed).
- **D+10**: WI-S20-003 SOC 2 gap delivered (Drata/Vanta dashboard + GAP-XX items) + WI-S20-004 lighthouse customer migration started (3 customers).
- **D+14**: WI-S20-005 SEALED (SLA + DPA Legal-reviewed + 3 lighthouse customer signing).
- **D+18**: WI-S20-006 SEALED (incident response 24/7 PagerDuty live).
- **D+22**: Pentest retest done WI-S20-002 (zero HIGH/CRITICAL pending).
- **D+25**: WI-S20-007 SEALED (30d staging sustained gate triggered) + WI-S20-004 lighthouse 30d observation started.
- **D+28**: WI-S20-008 launch prep done (separated from gate; soft-gate Marketing-led).
- **D+30**: **Implementation SEAL D+30** (engineering gate WIs done + 11+ sign-offs collected + pentest retest clean + DoD §7.1 binary criteria met EXCEPT 30d criteria).
- **D+30..D+60**: **Observation window (30d sustained evidence)** — 30d staging sustained zero SEV-1 + < 3 SEV-2 + all SLOs sustained + 3 lighthouse customers SLA claim met sustained 30d + synthetic page weekly < 5 min response sustained 30d. Métricas coletadas continuously; nenhum WI re-aberto exceto fix-critical.
- **D+60**: **GA Evidence Gate SEAL = GA-GO** (sprint sign-off final; **engineering gate APPROVED → GA announcement**) — DoD ship-gate criteria validados com janela 30d real (não-simulada); 30d sustained staging + 3 lighthouse SLA met + synthetic page < 5 min sustained todos comprovados via DASH-* dashboards. Implementation já SEALED em D+30; este gate libera **GA ANNOUNCEMENT** (public launch). OR delay if pending CRITICAL (per spec contract §14 promotion criteria).
- **Total**: 4 semanas implementação (20 dias úteis) + 30d observation window + GA Evidence Gate D+60 + buffer 10 dias multi-cloud + pentest unpredictability + Legal review iteration.

## 10. Risk Register

Ver `_spec_contract.md §15` (12 riscos 6-col com Owner per item: Pentest encontra CRITICAL, Lighthouse customer desiste mid-sprint, SOC 2 gap analysis revela > 50 gaps, Last-minute regression em staging, Legal review DPA atrasa, Marketing pressure to launch antes engineering ready, Lighthouse customer SLA claim miss em 30d, PagerDuty 24/7 schedule infeasible com team size, Pentest retest fails após remediation, All TLA+ green breaks during pentest discovery, Compliance officer sign-off declined, 30d sustained staging breaks mid-sprint).

Mitigações chave: buffer 10 dias para remediation; reputable firm engagement; engage 4-5 lighthouse candidates (signed LOI); Drata/Vanta continuous monitoring early Q3; engineering gate binary + separated from launch (CEO/Founder enforce gate); SLO targets achievable + 30d monitoring + SRE escalation + remediation flow; 3 regions com on-call manager rotation (US + EU at GA; APAC followers); track remediation early + retest scheduling buffer + multiple iteration; TLA+ CI gate continuous; engage compliance officer Q3 + iterate gaps; concurrent S-17 chaos automation provides safety net + immediate root-cause + fix.

## 11. Observability Plan

DASH-GA-READINESS (novo dashboard agregando todos os DASH-* cumulative S-09..S-19):

- 14 canonical sources verification matrix gauge (verified active per source).
- 27+ INVs cumulative status (CRITICAL active green; HIGH active green).
- All SLOs sustained 30d (SLO-AVAIL-* + SLO-LAT-* + SLO-FRESH-* + SLO-INCIDENT-RESPONSE-*).
- Engineering gate WIs SEAL status (7/7 + 1 launch separate).
- 3 lighthouse customers SLA claim met (per-customer panel).
- External pentest findings status (zero HIGH/CRITICAL pending).
- SOC 2 gap analysis completion ratio (Drata/Vanta dashboard verde > 95%).
- ~25 of 47 P0/P1 runbooks dry-run em 90d coverage cumulative.
- TLA+ 4 specs verde em CI sustained.
- SBOM CycloneDX 1.5+ signed published gauge.
- Synthetic page weekly response timeline (≤ 5 min p99 sustained).
- 30d staging zero SEV-1 + < 3 SEV-2 (sustained).

Métricas novas S-20:
- `corelink_synthetic_page_response_seconds_bucket{region, plan}` (histogram p99 ≤ 5 min target).
- `corelink_synthetic_page_acked_total{region, outcome, plan}` (counter; outcome ∈ acked|missed|escalated).
- `corelink_ga_readiness_gate_status_gauge{gate, phase, plan}` (gauge; gate ∈ engineering|launch; phase ∈ implementation|ga_evidence; values blocked|pending|approved|rejected).
- `corelink_lighthouse_customer_sla_claim_met_total{customer_id, plan}` (counter; per-customer panel; cardinality safe — apenas 3 lighthouse customers).
- `corelink_pentest_findings_status_total{severity, status, plan}` (counter; severity ∈ critical|high|medium|low; status ∈ open|remediated|deferred|waived).
- `corelink_canonical_source_verification_status_gauge{source, plan}` (gauge; source ∈ FRAMEWORK-00|SECURITY-MODEL|...|REMOTE-CACHE-PRODUCT-PROFILE; values 0|1).

Cumulative cardinality budget INV-OBS-CARDINALITY-BUDGET respeitado (≤ 20k séries únicas per métrica; ≤ 100k total; **NUNCA per-tenant labels** — `lighthouse_customer_id` apenas 3 customers cardinality safe excepcional canonical limited).

## 12. Security & Privacy

**STRIDE delta** (vs cumulative S-13..S-19 baseline):
- **Spoofing**: cumulative validation across all 14 canonical sources; external pentest validates holistically full system (NÃO scope-limited; AWS KMS provider primary scope expansão de S-14 para full system em S-20).
- **Tampering**: cumulative validation; SBOM CycloneDX 1.5+ signed published prevents supply chain tampering (alinhado S-12); audit chain integrity cumulative S-09..S-19 ratified em 30d staging clean.
- **Repudiation**: PRR-GA-001 doc 13 sign-offs canonical = forensic-grade audit trail; cumulative consent receipts + erasure attestations verifiable post-facto.
- **Information disclosure**: cumulative residency enforced (S-14); BYOK envelope encryption (S-14); cardinality budget INV-OBS-CARDINALITY-BUDGET respected; external pentest finds zero HIGH/CRITICAL pending = SEAL gate.
- **DoS**: cumulative rate limiting + region failover + chaos drill weekly; synthetic page weekly < 5 min response sustained 30d validates incident response readiness.
- **Elevation of privilege**: cumulative CTRL-AUTH-* + CTRL-AUDIT-* enforced; admin operations require dual-approval + WebAuthn UV=1 step-up (S-13); zero active waivers em controles CRITICAL.

**LINDDUN delta** (vs cumulative S-11/S-19 baseline):
- **Linkability**: tenant_id em audit (compliance accountability); cardinality budget respected.
- **Identifiability**: cumulative pseudonymization (S-11 DSR pseudo); BYOK envelope encryption (S-14); cumulative privacy controls.
- **Non-repudiation**: PRR-GA-001 + 13 sign-offs canonical + adversarial summary cumulative; 7y retention; chain integrity preserved.
- **Detectability**: 30d sustained staging zero SEV-1 detected via SLO violations + alerts; synthetic page weekly < 5 min validates detectability.
- **Disclosure of information**: DPA v1 signed 3 lighthouse customers; SLA v1 published; SOC 2 gap analysis covers transparency.
- **Unawareness**: customer notified per DPA + SLA + onboarding outcome; 24/7 incident response 3 regions.
- **Non-compliance**: SOC 2 + LGPD + GDPR + CCPA + EDPB SCCs + NIST SP 800-53 Rev 5 + ISO/IEC 27001:2022 + OWASP ASVS v4.0.3 satisfied via cumulative S-13..S-19 + S-20 evidence pack.

## 13. Post-mortem hooks

Triggers que **automaticamente abrem post-mortem doc** (per `_spec_contract.md §18`):

- Pentest CRITICAL finding pós-GA → CRITICAL post-mortem + Security review.
- 30d staging breaks (SEV-1) → CRITICAL post-mortem + GA gate review.
- Lighthouse customer SLA breach → 5-Why + SRE escalation + customer outreach.
- PagerDuty response > 5 min sustained → post-mortem + oncall reinforce.
- Compliance officer sign-off declined → process review + gap fix iteration.
- Engineering gate bypassed por Marketing pressure → CRITICAL post-mortem + governance reinforce.
- TLA+ CI red → CRITICAL post-mortem + invariant scope review.
- DPA Legal challenge from customer → post-mortem com Legal + customer trust review.
- Pentest retest fails após remediation → CRITICAL + multiple iteration cycle.
- 3 lighthouse customers desiste mid-sprint → contingency plan (reduce to 2 com ADR + plan to add 1 within 60d pós-GA per §19 waiver policy).
- SOC 2 gap analysis revela > 50 gaps → audit rescheduled + Drata/Vanta iterate Q3+Q4.

## 14. Sign-off (HIGH_RISK GA gate 13 canonical)

13 roles per spec contract §5.1 + framework §33.5.4.3 + ADR-0034 (largest of any sprint; HIGH_RISK GA gate canonical):

1. **Owner** (Gustavo Schneiter): final accountability for sprint delivery.
2. **Final Approver** (Gustavo Schneiter): final accountability for GA-go decision binary.
3. **Architect** (com **Crypto SME specialization MANDATORY** para BYOK + KMS + envelope encryption review carried over de S-14 specialization per ADR-0034; folded specialization): cumulative architecture review across 14 canonical sources.
4. **Security Lead**: external pentest report review + STRIDE cumulative + SBOM CycloneDX 1.5+ signed published + zero waivers controles CRITICAL.
5. **SRE Lead**: 30d sustained staging + ~25 P0/P1 runbooks dry-run + PagerDuty 3 regions + synthetic page weekly + on-call manager + escalation matrix + DR test cumulative.
6. **Engineer** (S-20 lead): cumulative engineering review + WI-S20-007 closing PRR.
7. **QA Lead**: TLA+ 4 specs verde em CI sustained + property tests cumulative + matrix tests cumulative + chaos drills cumulative.
8. **Product** (Gustavo Schneiter — solo founder dual-hat): cumulative product review + 3 lighthouse customers attestations + customer journey ratification.
9. **Compliance Officer**: SOC 2 gap analysis Drata/Vanta + GAP-XX roadmap Type I 6m + cumulative compliance evidence pack.
10. **Privacy Officer**: DPA v1 signed 3 lighthouse + Schrems II TIA cumulative + LGPD + GDPR + CCPA + cumulative privacy controls.
11. **AppSec advisor**: external pentest scope review + adversarial summary cumulative + envelope encryption + DEK cache TTL + audit chain integrity holistic.
12. **Legal Counsel** (carryover S-14 + S-19 legal-touching pattern; 12th sign-off canonical): DPA v1 + SLA v1 + sub-processor agreement + breach notification SLA + 3 lighthouse customer signing.
13. **Finance** (billing review per spec contract §5.1): cost regression gate + GA infra ≤ $1500/mês + pentest one-time $50-150k + Legal externo one-time $15-30k + cumulative billing reconciliation < 0.1% drift sustained.

Crypto SME folds into Architect role specialization per ADR-0034 (precedent S-12 SLSA L3 + S-13 secret rotation + S-14 BYOK envelope encryption). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect per framework §33.5.4.3). 13 sign-offs canonical é **largest of any sprint** (HIGH_RISK GA gate justifies full coverage; legal-touching DPA + SLA + Finance billing review carryover).

**Staffing reality** (per ADR-0034 solo-tier waiver Option A):
Sprint S-20 pode-se SEAL apenas com **13 sign-offs canonical** completos. Tier-1 staffing gap = sprint cannot SEAL until staffed OR explicit waiver com expiry + ADR. Recommended path (current state): Option A com ADR-0034 (Owner + Final Approver + Product solo-tier dual-hat) + Option C parallel staffing track (external advisor pool ~$80-200k full S-20 PRR including pentest $50-150k + Legal $15-30k + Compliance/Privacy/SRE/AppSec contract-based).

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-20 (cycle 12.S20.0; spec contract v1.1.0 base; codex findings remediated — 14 canonical sources count corrigido + CycloneDX 1.5+ alinhado S-12 + 4 semanas + Marketing/Launch separated; two-phase SEAL D+30/D+60; 13 sign-offs canonical largest of any sprint; FINAL sprint pré-GA ratificando cumulativamente todos os 14 canonical sources + 27+ INVs + cumulative CTRLs + cumulative SLOs). |

---

**Fim de S-20 sprint contract. Pós-GA: Sprint S-21+ começam Fase 2 (Remote Execution).**
