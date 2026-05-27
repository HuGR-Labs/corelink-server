---
id: "AUDIT-S20-PREFLIGHT"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Adversarial Pre-flight (Sonnet)"
tags: ["audit", "preflight", "s20", "ga", "high-risk"]
---

# AUDIT-S20-PREFLIGHT — Adversarial Pre-flight Review of S-20 GA Readiness

S-20 is the **FINAL SPRINT** (HIGH_RISK lane, 4 weeks, 8 WIs anticipated with 7 engineering gate + 1 launch deferred to wave-12). This audit interrogates the spec corpus at `specs/04_sprints/_sealed/S20/_spec_contract.md` + 7 engineering gate WI specs (WI-001..007). WI-008 (launch orchestration marketing) is explicitly deferred from the engineering gate per spec contract §5.2 R-S20-9 and §6.2.

---

## 1. Pre-flight verdict

**Verdict: NEEDS-SCOPE-FIX (downgradable to READY-TO-BUILD after P0 fixes)**

Rationale: spec corpus is **substantively complete and internally consistent on the largest GA mechanics** (14 canonical sources, 13 sign-offs, pentest retest gate, 30d sustained staging, 3 lighthouse customers, engineering/launch split, CycloneDX 1.5+ non-waivable, TLA+ 4 specs CI gate). Lote 10.20 codex remediations have closed most prior canonical drift. However, there are **3 P0 gaps** that affect either auditor-grade defensibility or implementation feasibility, plus several P1 ambiguities that will create REJECTED→remediate cycles during the sign-off ceremony if not pre-resolved. No FF-HR-ESCALATE needed: lane HIGH_RISK + 3 FFs are correct and exhaustive for what is being built; the gaps are coverage/clarity issues, not lane miscategorization.

---

## 2. Spec gaps P0/P1

### P0 (must fix before WI kickoff)

1. **P0-S20-001 — 13 sign-off roster has structural inconsistency between §0/§5.1 of the spec contract and the WI sign-off tables.** Spec contract §5.1 R-S20-1 enumerates **12 roles** ("SRE lead + Security lead + Privacy officer + DPO interim + Compliance officer + Engineer + QA + Product + Architect + AppSec + Crypto SME (BYOK) + Finance (billing)") but explicitly labels the count as "13 sign-offs canonical". The WI sign-off tables (WI-001..007) all list a *different* roster of 13: Owner + Final Approver + Architect (Crypto SME folded) + Security Lead + SRE Lead + Engineer + QA Lead + Product + Compliance Officer + Privacy Officer + AppSec advisor + Legal Counsel + Finance. Mismatches: (a) spec contract lists "DPO interim" as separate role; WIs fold DPO into Privacy Officer; (b) spec contract lists Crypto SME as 12th separate role; WIs fold Crypto SME into Architect per ADR-0034 precedent; (c) spec contract omits Owner/Final Approver/Legal Counsel explicitly; WIs treat them as canonical sign-offs. **Required fix**: align spec contract §5.1 R-S20-1 enumeration to match WI sign-off tables verbatim, including ADR-0034 folding note, and explicitly list all 13 roles with numbering. Risk if left unfixed: any one of the 13 sign-off ceremonies (one per WI = 7 ceremonies × 13 reviewers = 91 sign-offs total) can be challenged on canonical roster grounds.
2. **P0-S20-002 — No coverage of OWASP ASVS L3 for admin/billing surfaces.** Spec contract §17 references OWASP ASVS v4.0.3 generically; WI-S20-002 §4.4 methodology references "OWASP ASVS v4.0.3 baseline coverage" — but **nowhere is the L1/L2/L3 tier scoped per surface**. Industry SOTA for GA-grade pentest of regulated-data systems is **ASVS L3 for admin/billing/key-management surfaces** (privileged + financial + cryptographic material) **and ASVS L2 for customer-facing CAS/AC surfaces**. Without tier scoping, the pentest firm will default to ASVS L1, which a SOC 2 Type I auditor or enterprise CISO can fault as inadequate. **Required fix**: WI-S20-002 §5.2 scope table must add ASVS tier column (admin/BYOK/billing/audit chain = L3; CAS/AC customer surfaces = L2; ingress/marketing = L1).
3. **P0-S20-003 — No coverage of CIS Benchmark compliance for underlying CF Workers/D1/R2 surfaces.** Zero matches for "CIS Benchmark" or equivalent (CIS Hardening, CIS Controls) across all 7 WIs and security_model.md. While CF Workers/D1/R2 are PaaS (not IaaS) and CIS Benchmarks for serverless are limited, **CIS Controls v8** (e.g., 6.7 access management, 14.6 inventory, 16.x application security) and **CIS Cloud Provider mappings** are standard expectations in enterprise security questionnaires. The Drata/Vanta dashboard in WI-S20-003 will surface this gap day-1 of integration. **Required fix**: either add explicit CIS Controls v8 coverage map under WI-S20-003 SOC 2 gap analysis OR add an explicit anti-scope entry to §10 stating "CIS Benchmark/Controls coverage = pós-GA Q1 (enterprise pivot)" with risk acceptance. The current silent omission is the worst of both worlds.

### P1 (fix before SEAL)

4. **P1-S20-001 — Launch embargo coordination between WI-008 (marketing disclosure) and WI-002 (pentest disclosure) is undefined.** WI-008 prepares press release + blog posts + case studies + Product Hunt assets. WI-002 §5.4 specifies "report sanitized sharable em sales conversations sob NDA + raw findings restricted". There is **no spec text governing the disclosure embargo** — i.e., when a sanitized pentest summary may be cited in marketing copy, who signs off, and whether MEDIUM/LOW deferred findings (waived per WI-002 §6.2) are disclosed in marketing materials. This is the standard "responsible disclosure" friction point at GA launch. **Required fix**: add to WI-S20-005 or WI-S20-008 an embargo coordination clause: pentest sanitized summary marketing inclusion gated by AppSec advisor + Legal Counsel co-sign; deferred MEDIUM/LOW findings excluded from marketing pre-GA.
5. **P1-S20-002 — 24/7 PagerDuty 3-region staffing realism is acknowledged but not staffed.** WI-S20-006 §2.2 mandates 3-region coverage day-1 (US Pacific + US Eastern + EU) with response < 5 min p99. The fallback "Option A solo-tier ADR-0034 (Owner + Final Approver dual-hat)" is **mathematically impossible to sustain 24/7 across 3 regions with one human** (would require single human awake 24/7 across UTC-8 + UTC-5 + UTC+0 = full 16-hour wake window minimum, not sustainable for the 30d synthetic-page-weekly observation). Option C external advisor pool ($80-200k) is hand-waved as "Recommended path" but no contract is signed at audit time. **Required fix**: WI-S20-006 must commit to Option C contract signed by D+5 (parallel to WI-S20-002 pentest engagement) OR explicit ADR documenting that 3-region staffing falls to 2-region (US/EU compressed) as MVP with APAC follower fast-track; current ADR-0034 Option A is not credible for 24/7.
6. **P1-S20-003 — "External Auditor" role ambiguity.** The question of whether the pentest firm (Schellman/A-LIGN/Trail of Bits) constitutes a separate sign-off role is **not addressed in any sign-off table**. The 13 canonical sign-offs are all internal/contracted-internal roles; the pentest firm produces an artifact (the pentest report) but is not a sign-off party. This is defensible (pentest is evidence, not approval) but the spec should state this explicitly to prevent reviewer confusion at the closing PRR. **Required fix**: WI-S20-001 §5.3 must add note: "External pentest firm produces evidence (report + retest); is NOT a 14th sign-off role per framework §33.5.4.3 canonical."
7. **P1-S20-004 — SOC 2 Type 1 6-month roadmap vs GA timeline parallelism is correctly stated but the dependency direction is implicit.** WI-S20-003 §2.4 + §10 anti-scope state Type I cert is 6m pós-GA. Spec contract §10 anti-scope confirms. Verdict: **parallel track is correctly modeled — GA does not wait for Type I cert**; only the gap analysis + GAP-XX roadmap + Drata/Vanta > 95% controls is GA-blocking. Verify is OK.

---

## 3. 14 canonical sources inventory (verified)

Cross-referenced against `specs/03_architecture/` + `specs/01_vision/` + `specs/00_framework.md` + `specs/05_quality/`. Spec contract §3 inherits_from block enumerates exactly 14:

| # | Source ID | Location | Verified Exists |
|---|---|---|---|
| 1 | FRAMEWORK-00 | `specs/00_framework.md` | yes (implicit via `.claude` memory; canonical root) |
| 2 | SECURITY-MODEL | `specs/03_architecture/security_model.md` | yes |
| 3 | PRIVACY-MODEL | `specs/03_architecture/privacy_model.md` | yes |
| 4 | OBSERVABILITY-MODEL | `specs/03_architecture/observability_model.md` | yes |
| 5 | SLO-CATALOG | `specs/05_quality/slo_catalog.md` (per WI-001 §5.2; note: ALSO present at `specs/03_architecture/slo_catalog.md` — see ambiguity below) | yes |
| 6 | FAILURE-MODES | `specs/03_architecture/failure_modes.md` | yes |
| 7 | RESILIENCE-PATTERNS | `specs/03_architecture/resilience_patterns.md` | yes |
| 8 | DATA-MODEL | `specs/03_architecture/data_model.md` | yes |
| 9 | STORAGE-SEMANTICS-MATRIX | `specs/03_architecture/storage_semantics_matrix.md` | yes |
| 10 | AUTH-MODEL | `specs/03_architecture/auth_model.md` | yes |
| 11 | KEY-MANAGEMENT | `specs/03_architecture/key_management.md` | yes |
| 12 | COMPLIANCE-MATRIX | `specs/03_architecture/compliance_matrix.md` | yes |
| 13 | INVARIANT-REGISTRY | `specs/03_architecture/invariant_registry.md` | yes |
| 14 | REMOTE-CACHE-PRODUCT-PROFILE | `specs/01_vision/remote_cache_product_profile.md` (per WI-001 §5.2) OR `specs/03_architecture/remote_cache_product_profile.md` (file exists in arch dir) | yes (file exists in 03_architecture; WI-001 §5.2 links to 01_vision path which may not exist) |

**Count: exactly 14, verified.** Codex remediation from Lote 10.20 (prior "10 canonical sources" → "14") is applied consistently across spec contract §3 + WI-001 §5.2.

**Note (P1-S20-005, minor)**: WI-S20-001 §5.2 matrix lists SLO-CATALOG path as `specs/05_quality/slo_catalog.md` and REMOTE-CACHE-PRODUCT-PROFILE as `specs/01_vision/remote_cache_product_profile.md`, but the actual repo has these at `specs/03_architecture/slo_catalog.md` and `specs/03_architecture/remote_cache_product_profile.md` respectively. **Fix**: update WI-001 §5.2 evidence link column to match actual repo layout, or move sources to the cited paths.

---

## 4. 13 sign-off roster (verified, with P0-S20-001 ambiguity flagged)

The **canonical roster per WI sign-off tables (WI-001..007 are all consistent with each other)** is:

| # | Role | Staffing path |
|---|---|---|
| 1 | Owner | Gustavo Schneiter (solo founder dual-hat per ADR-0034) |
| 2 | Final Approver | Gustavo Schneiter (solo founder dual-hat per ADR-0034) |
| 3 | Architect (Crypto SME folded — ADR-0034 precedent S-14 BYOK) | External advisor pool ($80-200k) |
| 4 | Security Lead | External advisor pool |
| 5 | SRE Lead | External advisor pool OR Option B Tier-1 SRE hire |
| 6 | Engineer (S-20 lead) | TBD (internal) |
| 7 | QA Lead | External advisor pool |
| 8 | Product | Gustavo Schneiter (dual-hat) |
| 9 | Compliance Officer (SOC 2 + LGPD + GDPR + CCPA) | External advisor pool |
| 10 | Privacy Officer (DPA + Schrems II TIA; DPO interim folded) | External advisor pool |
| 11 | AppSec advisor (pentest scope + adversarial cumulative) | External advisor pool |
| 12 | Legal Counsel (carryover S-14+S-19; DPA + SLA + breach notification) | Cooley/DLA Piper/Bird & Bird ~$15-30k, 6-week lead |
| 13 | Finance (cost regression gate + billing reconciliation) | TBD |

**External Auditor (pentest firm) is NOT a sign-off role.** The pentest firm produces evidence (sanitized report + retest result + remediation tracker = EVT-040) consumed by the AppSec advisor (#11) sign-off, but does not itself sign the PRR. This is defensible and correct for the engineering gate — pentest completion is a separate gate item (§19 waiver policy: "zero HIGH/CRITICAL pending = SEAL gate hard"), not a sign-off. **Recommend P1-S20-003 fix to state this explicitly in spec contract.**

Solo-tier ADR-0034 Option A is explicitly **NOT acceptable** for the GA gate per WI-S20-001 §6.2 negative path 4 (Lote 10.20 codex P0 canonical fix: "Mandatory minimum 5 of 10 pending roles preenchidos via external advisor antes de GA-go (Compliance Officer + Privacy Officer + Security Lead + Legal Counsel + Crypto SME canonical priority)"). This is correct and adversarially defensible.

---

## 5. GA-blocker registry

Items that MUST close before GA. Status at audit time (2026-05-14):

| # | GA-blocker | Owner WI | Status | Notes |
|---|---|---|---|---|
| GB-01 | PRR-GA-001 doc APPROVED binary (13 sign-offs collected; CONDITIONALLY = block GA) | WI-S20-001 | READY (spec); NOT STARTED (impl) | External advisor pool not engaged at audit time. |
| GB-02 | External pentest report clean (zero HIGH/CRITICAL pending; retest passed) | WI-S20-002 | READY (spec); firm bid Q3 status unknown | $50-150k budget; 4-week firm lead time. Timeline corrected by Lote 10.20: pentest D+10..D+24 → remediation D+27..D+37 → retest D+37..D+44. |
| GB-03 | SOC 2 gap analysis + GAP-XX roadmap + Drata/Vanta > 95% controls | WI-S20-003 | READY (spec); Drata/Vanta not selected | Q3 selection assumed; not at audit. |
| GB-04 | 3 lighthouse customers SLA met sustained 30d (2 team + 1 enterprise BYOK) | WI-S20-004 | READY (spec); 4-5 candidates engagement Q3 status unknown | Non-waivable per Lote 10.20 codex P0 fix; prior "3→2 / 30d→21d" allowance REMOVED. |
| GB-05 | SLA v1 published + DPA v1 signed 3 lighthouse | WI-S20-005 | READY (spec) | Legal Counsel external engagement Cooley/DLA Piper. |
| GB-06 | Incident response 24/7 PagerDuty 3 regions + synthetic page weekly < 5 min sustained 30d | WI-S20-006 | READY (spec); **P1-S20-002 staffing risk** | 3-region day-1 mandate; APAC pós-GA Q1. |
| GB-07 | 30d sustained staging zero SEV-1 + < 3 SEV-2 + all SLOs sustained | WI-S20-007 | READY (spec); concurrent S-17 chaos 4w pre-req | Sequential ~50d coverage pré-GA (S-17 4w + S-20 30d, no overlap). |
| GB-08 | ~25 of 47 P0/P1 runbooks dry-run em 90d cumulative S-17+S-20 | WI-S20-007 | READY (spec) | Lote 10.20 codex P1 math fix; prior "all 40" inconsistent. |
| GB-09 | TLA+ 4 specs verde em CI sustained (tenant_isolation + cas_integrity + audit_immutability + gc_correctness) | WI-S20-007 | READY (spec) | 4 specs = correct count; verifies the 4 CRITICAL TLA+-amenable INVs (TENANT-ISOLATION + CAS-INTEGRITY + AUDIT-APPEND-ONLY + GC-001/004). Highest-risk INVs covered. |
| GB-10 | SBOM CycloneDX 1.5+ signed published customer-accessible | WI-S20-007 | READY (spec) | Non-waivable per Lote 10.20 codex P1 fix; prior "fallback to 1.4 via ADR" REMOVED. Aligned S-12 R-S12-3. |
| GB-11 | Zero active waivers em controles CRITICAL | WI-S20-007 | READY (spec) | Security baseline gate. |
| GB-12 | All 27+ cumulative INVs active | WI-S20-001 §9 + WI-S20-007 §9 | READY (spec) | Cumulative ratification S-13..S-19. |
| GB-13 | Cost regression gate ≤ $1500/mês GA infra | WI-S20-001 + WI-S20-007 | READY (spec) | Finance sign-off (#13). |

**Anti-scope correctly excluded from GA blockers**: Fase 2 (Remote Execution), Apache 2.0 OSS release, SOC 2 Type I cert (parallel 6m pós-GA), FedRAMP Moderate, APAC GA expansion, multi-cloud federation BYOK, AI features, mobile, pre-GA paying customers beyond 3 lighthouse, **marketing/launch orchestration as engineering gate dependency** (correctly separated per Lote 10.20 P0 §10.s20.3 fix).

---

## 6. Targeted verification of pre-flight checklist questions

1. **Lane HIGH_RISK + 3 FF-HR justified?** Yes. FF-HR-005 (controle final), FF-HR-009 (production contracts), FF-HR-010 (regulatory live em escala) all apply. **No further FF-HR missed**: FF-HR-001..004 (early-stage scaffolding) not applicable; FF-HR-006..008 (Fase 2 / multi-cloud federation / AI) are anti-scope.
2. **14 canonical sources count.** Verified exactly 14 in §3. P1-S20-005 minor path discrepancy.
3. **13 canonical sign-offs.** Verified per WI tables (13 roles). **P0-S20-001 ambiguity** between spec contract §5.1 (lists 12 distinct roles labeled "13") and WI tables (13 explicit). External Auditor (pentest firm) is NOT a sign-off role — defensible but should be stated explicitly (P1-S20-003).
4. **30d staging vs S-17 chaos 4w.** Correctly distinguished. Spec contract §5.1 R-S20-7 + WI-S20-007 §5.1 specify sequential coverage (S-17 4w → S-20 30d, ~50d cumulative, no overlap). No conflict.
5. **Engineering Gate vs Launch Orchestration.** §1 + §4 + §6.1/§6.2 + §10 anti-scope + §10.s20.3 cleanly separate them. WI-008 explicitly deferred from engineering gate; §10 anti-scope item "Marketing/Launch orchestration as engineering gate dependency" is non-negotiable. **Clean.**
6. **External pentest retest gate (zero HIGH/CRITICAL non-waivable).** Verified §19 first bullet: "External pentest report clean (zero HIGH/CRITICAL pending) — security baseline — NÃO PODE waive". Aligned WI-S20-002 §6.2.
7. **3 lighthouse SLA 30d unwaivable.** Verified §19 third bullet (Lote 10.20 codex P0 canonical fix explicitly REMOVED prior "3→2 / 30d→21d" allowance).
8. **SOC 2 Type 1 6m roadmap vs GA timeline.** Correctly modeled as **parallel track**: gap analysis + GAP-XX + Drata/Vanta > 95% controls is GA-blocking; Type I cert itself is anti-scope §10 (6m pós-GA). GA does not wait for Type I.
9. **24/7 PagerDuty 3-region day-1.** Spec mandates day-1 (US Pacific + US Eastern + EU; APAC pós-GA Q1). **P1-S20-002 staffing realism flag**: Option A solo-tier dual-hat not credible 24/7; Option C external advisor pool not yet contracted at audit time.
10. **TLA+ 4 runbooks (correctly: 4 specs).** tenant_isolation + cas_integrity + audit_immutability + gc_correctness. These cover the 4 highest-risk CRITICAL INVs (INV-TENANT-ISOLATION + INV-CAS-INTEGRITY + INV-AUDIT-APPEND-ONLY + INV-GC-001/004). **Highest-risk coverage confirmed.** Note: prompt referenced "TLA+ 4 runbooks" — actually 4 *specs* (not runbooks). Terminology drift in prompt, not in spec.

---

## 7. Recommendation

**Proceed to WI kickoff after closing the 3 P0 items** (sign-off roster alignment + ASVS L3 tier scoping + CIS Benchmark coverage decision). P1 items (4–7) can close during sprint execution but P1-S20-002 (24/7 staffing) must close by D+5 alongside pentest firm engagement contract. The spec corpus is otherwise auditor-grade and reflects accumulated S-13..S-19 SOTA discipline + Lote 10.20 canonical tightening. No FF-HR-ESCALATE required.

---

**Fim AUDIT-S20-PREFLIGHT.**
