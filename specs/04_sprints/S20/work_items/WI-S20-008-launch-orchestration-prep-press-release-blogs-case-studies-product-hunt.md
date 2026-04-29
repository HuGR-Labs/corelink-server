---
id: "WI-S20-008"
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
  - "FRAMEWORK-00"
  - "REMOTE-CACHE-PRODUCT-PROFILE"
tags: ["wi", "s20", "ga", "launch-orchestration", "marketing", "press-release", "product-hunt", "soft-gate", "high-risk"]
---

# WI-S20-008 — Launch Orchestration Prep (SEPARATED From Engineering Gate Per Spec Contract §1 Codex Finding + §6.2; Soft-Gate; **NÃO Blocking Engineering Gate Binary GA-Go Decision**; CEO/Founder Enforce Gate; Launch Pode Shift Date Sem Affecting Engineering Readiness) — Press Release Reviewed Por PR + Legal (Subject: "CoreLink GA — Multi-Tenant Content-Addressable Cache on Cloudflare; Bazel/Buck2/RBE-Compatible Remote Cache; HuGR Org Launch") + 5 Blog Posts Deep Dives Published em Staging Blog `docs.corelink.dev/blog` (Multi-Tenant Cloudflare Architecture + BYOK Enterprise Schrems II + Customer-Managed Kill Switch & Erasure Attestation + TLA+ Formal Verification em Production + Lighthouse Customers Case Studies) + 3 Case Studies Drafted com Lighthouse Customer Testimonials (Forge + 1 OSS + 1 Enterprise BYOK; Marketing Co-Led com Customer Success) + Product Hunt Launch Prep (Assets: Hero Image + Tagline + 1-min Demo Video + Maker Comment Template + Outreach List) + Maker Outreach Scheduled D+25..D+30 Launch Day

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-20](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S20-008 |
| Título | Launch orchestration prep (separated soft-gate; NÃO blocking engineering gate) — press release + 5 blog posts + 3 case studies + Product Hunt launch prep — Marketing-led |
| Sprint | S-20 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 + FF-HR-009 + FF-HR-010 (lane HIGH_RISK herdado do sprint; **launch orchestration é soft-gate NÃO blocking engineering gate** per spec contract §1 codex finding + §6.2; CEO/Founder enforce gate canonical) |

## 1. Objetivo + JTBD (launch separated; NÃO blocking)

**Objetivo**: Operacionalizar **launch orchestration prep** — press release + 5 blog posts + 3 case studies + Product Hunt launch prep + Maker outreach — **separated from engineering gate canonical** per spec contract §1 codex finding + §6.2; **NÃO blocking engineering gate binary GA-go decision**; Marketing-led + PR-led; CEO/Founder enforce gate; launch pode shift date sem affecting engineering readiness.

**JTBD**: "Como Marketing Lead / PR Manager / CEO/Founder enforcing GA-go binary engineering gate, preciso evidência verificável que: (a) **launch orchestration é separated from engineering gate canonical** (binary GA-go independente); engineering gate WIs 001..007 SEALED → GA-go decision; launch orchestration WI-008 soft-gate; (b) **press release reviewed por PR + Legal** (Legal review per WI-S20-005 cumulative; PR review por external PR firm OR CMO contracted); (c) **5 blog posts deep dives published em staging blog** (`docs.corelink.dev/blog`); (d) **3 case studies drafted com lighthouse customer testimonials** (Forge + OSS + enterprise BYOK; Marketing co-led com Customer Success per WI-S20-004 cumulative); (e) **Product Hunt launch prep** com assets + Maker outreach scheduled D+25..D+30 launch day; (f) **NÃO blocking** — engineering gate binary independente de marketing readiness; CEO/Founder enforce gate."

**SOFT-GATE NÃO BLOCKING ENGINEERING GATE BINARY** per spec contract §1 codex finding + §6.2 + sprint.md §7.

## 2. Scope

### 2.1 In-scope (soft-gate)

1. **Press release** `marketing/press-release-ga.md`:
   - Subject: "CoreLink GA — multi-tenant content-addressable cache on Cloudflare; Bazel/Buck2/RBE-compatible Remote Cache; HuGR org launch".
   - Audience: technical media (TechCrunch, The Register, Hacker News, ArsTechnica) + ecosystem outlets (Bazel community, Buck2 community).
   - **Reviewed por PR**: external PR firm OR CMO contracted.
   - **Reviewed por Legal**: Cooley/DLA Piper/Bird & Bird ~$15-30k 6-week lead reuso S-14 path (per WI-S20-005 cumulative).
   - Version control via Git: draft → PR review → Legal review → final.

2. **5 blog posts deep dives published em staging blog** (`docs.corelink.dev/blog`):
   - **Post 1**: "Multi-tenant Cloudflare architecture: how CoreLink scales to 1000 tenants per region without sacrificing isolation" (TLA+ tenant_isolation.tla + INV-TENANT-ISOLATION evidence).
   - **Post 2**: "BYOK enterprise tier + Schrems II compliance: 4 KMS providers + Ed25519 erasure attestation + DPA amendment Legal-reviewed" (S-14 BYOK + S-20 DPA v1).
   - **Post 3**: "Customer-managed kill switch & erasure attestation: hard-fail crypto sovereignty without operator override" (INV-BYOK-CRYPTO-SOVEREIGNTY + INV-ERASURE-ATTESTATION-SIGNED + NIST SP 800-88 Rev.1).
   - **Post 4**: "TLA+ formal verification em production: 4 specs verde em CI sustained (tenant_isolation + cas_integrity + audit_immutability + gc_correctness)" (PAT-FORMAL-VERIFICATION-001).
   - **Post 5**: "Lighthouse customers case studies: Forge (HuGR org self-attestation) + OSS Bazel/Buck2 ecosystem + enterprise BYOK" (per WI-S20-004 cumulative).
   - Each post: 1500-3000 words; technical deep dive; code samples; references; SEO-optimized.
   - **Marketing-led**: drafted Q3-Q4 + iterated D+0..D+28; published staging D+28; production D+30 (GA day).

3. **3 case studies drafted com lighthouse customer testimonials**:
   - **Forge case study** (team customer-zero): HuGR org self-attestation; pre-CoreLink baseline + migration experience + 30d observation results + future plans.
   - **OSS case study** (team external): OSS maintainer endorsement; Bazel/Buck2 ecosystem fit per REMOTE-CACHE-PRODUCT-PROFILE.
   - **Enterprise BYOK case study** (1 enterprise): sanitized sharable em sales sob NDA; FedRAMP-ready / EU / financial services profile.
   - **Marketing co-led com Customer Success** per WI-S20-004 cumulative.
   - Legal-reviewed + customer-approved per WI-S20-005 + WI-S20-004 cumulative.

4. **Product Hunt launch prep** (`marketing/product-hunt-assets/`):
   - **Hero image** (1280×720 @ 2x; CoreLink branding; tagline overlay).
   - **Tagline** ≤ 60 chars: "CoreLink — content-addressable cache on Cloudflare for Bazel/Buck2/RBE".
   - **1-min demo video** (Loom OR YouTube unlisted; covering: signup → DPA → tier → first PAT → first CAS PUT → CAS GET).
   - **Maker comment template** (Product Hunt convention).
   - **Outreach list**: 50-100 Makers + reviewers + ecosystem influencers.
   - **Maker outreach scheduled D+25..D+30 launch day** (Maker outreach 5-day window pré-launch; standard Product Hunt convention).

5. **Soft-gate criteria** (per spec contract §6.2 + sprint.md §7.2):
   - Marketing ready: press release reviewed por PR + Legal.
   - 5 blog posts published em staging blog.
   - 3 case studies drafted com lighthouse customer testimonials.
   - Product Hunt launch assets prepared + Maker outreach scheduled.
   - **Note canonical**: launch orchestration **não bloqueia engineering gate**; engineering gate aprova GA-go independentemente; CEO/Founder enforce gate; launch pode shift date sem affecting engineering readiness.

### 2.2 Anti-scope

- ❌ **Marketing/Launch orchestration as engineering gate dependency** — separated; engineering gate é binary independente de marketing readiness; CEO/Founder enforce gate.
- ❌ Customer-facing marketing analytics dashboard — pós-GA enterprise.
- ❌ Multi-language press release / blog posts beyond en-US (pós-GA Q1 demand-driven; S-19 cobre 3 locales DPA only).
- ❌ Product Hunt launch sem Maker outreach (Maker outreach mandatory per Product Hunt convention).
- ❌ Press release sem Legal review (Legal review mandatory per spec contract §6.2 + cumulative WI-S20-005 Legal externo path).
- ❌ Case studies sem customer approval (customer-approved mandatory per WI-S20-004 + WI-S20-005 cumulative).
- ❌ Bug bounty program announcement (pós-GA Q1).
- ❌ Apache 2.0 open source release announcement (pós-GA decision; anti-scope per spec contract §10).

## 3. Capability mapping

- **CAP-LAUNCH-001** (Launch orchestration prep): IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.2 R-S20-8 + R-S20-9 + §6.2 + §10 anti-scope (Marketing/Launch separated canonical)` + `WI-S20-004 lighthouse customer attestations testimonial + case study` + `WI-S20-005 Legal externo review path` + `01_vision/remote_cache_product_profile.md (REMOTE-CACHE-PRODUCT-PROFILE Bazel/Buck2/RBE alignment)`.

## 4. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S20-008-D1 | Press release | `marketing/press-release-ga.md` | reviewed por PR + Legal; subject "CoreLink GA — multi-tenant content-addressable cache on Cloudflare"; technical media + ecosystem outlets audience |
| S20-008-D2 | 5 blog posts | `docs.corelink.dev/blog/{1-5}.md` published em staging blog | 1500-3000 words each; technical deep dive; code samples; references; SEO-optimized; production D+30 GA day |
| S20-008-D3 | 3 case studies | `marketing/case-studies/{forge,oss,enterprise-byok}.md` | Marketing co-led com Customer Success; Legal-reviewed; customer-approved |
| S20-008-D4 | Product Hunt launch prep | `marketing/product-hunt-assets/` | hero image + tagline + 1-min demo video + Maker comment template + outreach list 50-100; Maker outreach scheduled D+25..D+30 launch day |
| S20-008-D5 | Soft-gate criteria documented | embedded em sprint.md §7.2 | criteria met; **NÃO blocking engineering gate binary GA-go decision** |

## 5. Detailed design

### 5.1 Engineering vs Launch separation canonical (per spec contract §1 codex finding + §6.2)

**Engineering Gate (WI-S20-001..007 binary GA-go blocking)**:
- 7 WIs SEALED + 13 sign-offs canonical + 30d sustained staging + 3 lighthouse SLA met + synthetic page < 5 min + all SLOs sustained + zero HIGH/CRITICAL pentest pending + zero CRITICAL waivers.
- Decision: APPROVED → GA-GO | REJECTED → remediation cycle.
- **CEO/Founder enforce gate canonical**.

**Launch Orchestration (WI-S20-008 soft-gate NÃO blocking)**:
- Marketing/PR managed.
- Press release + 5 blog posts + 3 case studies + Product Hunt prep.
- **Soft-gate** criteria: marketing ready; **NÃO blocking engineering gate binary**.
- Launch pode shift date sem affecting engineering readiness.
- Risk: marketing pressure to launch antes engineering ready (per spec contract §15 row 6); mitigation = engineering gate binary + separated; CEO/Founder enforce gate.

### 5.2 Press release structure

```markdown
# CoreLink GA — Multi-tenant Content-Addressable Cache on Cloudflare

**FOR IMMEDIATE RELEASE — D+30 (GA day)**

HuGR Labs today announced the General Availability of CoreLink, a multi-tenant content-addressable cache built on Cloudflare's global network, designed for Bazel, Buck2, and Remote Build Execution (RBE) workloads.

## Highlights
- Multi-tenant architecture with TLA+ formal verification (4 specs verde em CI sustained).
- BYOK enterprise tier with 4 KMS providers (AWS KMS, GCP KMS, Azure Key Vault, HashiCorp Vault).
- Customer-managed kill switch ≤ 5 min global hard-fail (INV-BYOK-CRYPTO-SOVEREIGNTY).
- Ed25519 erasure attestation NIST SP 800-88 Rev.1 crypto-erase compliant 7y retention.
- 4 regions WNAM/ENAM/WEUR/SAM enumerated with data localization promises (INV-DATA-RESIDENCY).
- Schrems II TIA + DPA Legal-reviewed (Cooley/DLA Piper/Bird & Bird).
- 3 lighthouse customers attested (Forge customer-zero + OSS Bazel/Buck2 ecosystem + enterprise BYOK).
- External pentest clean (Schellman/A-LIGN/Trail of Bits; zero HIGH/CRITICAL pending).
- 30d sustained staging zero SEV-1 + all SLOs sustained.

## About HuGR Labs
HuGR ("Human Guardrail") is the org behind CoreLink + a portfolio of developer infrastructure products.

## Contact
- press@corelink.dev
- gustavo@humangr.com

###
```

### 5.3 5 blog posts outlines

1. **Post 1**: "Multi-tenant Cloudflare architecture: how CoreLink scales to 1000 tenants per region without sacrificing isolation"
   - TLA+ tenant_isolation.tla evidence + INV-TENANT-ISOLATION + 30k property test + chaos drill weekly.
   
2. **Post 2**: "BYOK enterprise tier + Schrems II compliance: 4 KMS providers + Ed25519 erasure attestation + DPA amendment Legal-reviewed"
   - S-14 BYOK 4 providers FIPS 140-3/140-2 + envelope encryption flow + DEK cache TTL 5 min hard + S-20 DPA v1 Legal externo reviewed.
   
3. **Post 3**: "Customer-managed kill switch & erasure attestation: hard-fail crypto sovereignty without operator override"
   - INV-BYOK-CRYPTO-SOVEREIGNTY + INV-ERASURE-ATTESTATION-SIGNED + NIST SP 800-88 Rev.1 crypto-erase + 7y retention + verify endpoint.
   
4. **Post 4**: "TLA+ formal verification em production: 4 specs verde em CI sustained"
   - PAT-FORMAL-VERIFICATION-001 + tenant_isolation + cas_integrity + audit_immutability + gc_correctness + CI gate ≤ 60s runtime.
   
5. **Post 5**: "Lighthouse customers case studies: Forge + OSS + enterprise BYOK"
   - Per WI-S20-004 cumulative; testimonials; 30d observation results.

### 5.4 Product Hunt outreach list categories

- **Makers** (50): Product Hunt convention 50 Makers outreach.
- **Reviewers** (30): tech press + ecosystem influencers.
- **Ecosystem** (20): Bazel + Buck2 + RBE community maintainers.
- **Total**: ~100.

## 6. Acceptance criteria

### 6.1 Positive paths

1. **Press release reviewed por PR + Legal** committed em `marketing/press-release-ga.md`.
2. **5 blog posts published em staging blog** committed em `docs.corelink.dev/blog/{1-5}.md`.
3. **3 case studies drafted com lighthouse customer testimonials** committed em `marketing/case-studies/{forge,oss,enterprise-byok}.md`.
4. **Product Hunt launch prep complete**: assets + Maker outreach list 50-100 + Maker outreach scheduled D+25..D+30.
5. **Soft-gate criteria documented**: launch orchestration NÃO blocking engineering gate binary canonical.
6. **Engineering vs launch separation explicit** em sprint.md §7 DoD (per spec contract §1 codex finding + §6.2).

### 6.2 Negative paths (≥ 4 mandatory)

1. **Marketing pressure to launch antes engineering ready** (per spec contract §15 row 6) → engineering gate binary + separated from launch; CEO/Founder enforce gate; technical debt + customer trust loss inaceitável.
2. **Press release Legal review atrasa** → buffer 6-week Legal externo lead reuso WI-S20-005; iterate during sprint.
3. **Case study customer não-approved** → iterate; budget multiple revision cycles per WI-S20-004 + WI-S20-005 cumulative.
4. **Product Hunt launch fail** (low Makers outreach response) → soft-gate; launch can shift date sem affecting engineering readiness; iterate Maker outreach pós-launch.
5. **Blog posts não published staging em D+28** → fallback production blog launch on GA day D+30; sustained iteration pós-GA.
6. **PR firm contract não staffed** → fallback Owner + Final Approver dual-hat self-PR (solo founder context per ADR-0034 Option A); Legal review via Cooley/DLA Piper/Bird & Bird mandatory.

## 7. Test plan

### 7.1 Press release validation

- Subject + content reviewed por PR + Legal.
- Audience targeted (technical media + ecosystem outlets).

### 7.2 Blog posts validation

- 5 posts × 1500-3000 words each.
- Technical deep dive + code samples + references + SEO-optimized.
- Published em staging blog `docs.corelink.dev/blog`.

### 7.3 Case studies validation

- 3 case studies × Marketing co-led com Customer Success.
- Legal-reviewed + customer-approved.

### 7.4 Product Hunt launch prep validation

- Hero image + tagline + 1-min demo video + Maker comment template + outreach list 50-100 prepared.
- Maker outreach scheduled D+25..D+30.

## 8. Failure modes

- **FM-MARKETING-PRESSURE-LAUNCH-EARLY** (per spec contract §15 row 6): mitigation = engineering gate binary + separated from launch + CEO/Founder enforce gate.
- **FM-PRESS-RELEASE-LEGAL-ATRASA**: mitigation = buffer 6-week Legal externo lead reuso WI-S20-005.
- **FM-CASE-STUDY-CUSTOMER-NOT-APPROVED**: mitigation = iterate + multiple revision cycles per WI-S20-004 cumulative.
- **FM-PRODUCT-HUNT-LAUNCH-FAIL**: mitigation = soft-gate; launch can shift; iterate Maker outreach pós-launch.

## 9. Invariants (cumulative ratification)

ALL 27+ INVs from S-13..S-19 + canonical sources active; launch orchestration NÃO violates invariants (marketing material does not introduce new INV obligations).

S-20 NÃO introduz novos INVs.

## 10. Controls (cumulative)

CTRL-XXX cumulative referenced em marketing material:
- CTRL-PRIV-* (case study customer-approved + sanitized).
- SOC 2 + LGPD + GDPR + CCPA + EDPB SCCs cumulative.

## 11. Resilience patterns (cumulative)

PAT-XXX cumulative referenced em blog posts:
- PAT-FORMAL-VERIFICATION-001 (TLA+ 4 specs verde em CI; Post 4 deep dive).
- PAT-REGION-FAILOVER-001 (Post 1 multi-tenant Cloudflare architecture).
- PAT-DUAL-APPROVAL-001 (Post 3 customer-managed kill switch).

## 12. Observability (cumulative)

Métricas Prometheus snake_case underscored (label `plan` aplicável):

- `corelink_marketing_blog_post_published_total{post_id, plan}` (counter; cardinality safe — 5 posts).
- `corelink_marketing_case_study_approved_total{customer_id, plan}` (counter; cardinality safe — 3 customers).
- `corelink_product_hunt_outreach_status_gauge{phase, plan}` (gauge; phase ∈ planning|outreach|launch_day|complete).

DASH-LAUNCH-ORCHESTRATION panel embedded em DASH-GA-READINESS dashboard (soft-gate; NÃO blocking).

## 13. Security & Privacy

**STRIDE delta**: marketing material does not introduce new security obligations beyond cumulative; case studies sanitized + customer-approved per Legal review.

**LINDDUN delta**:
- **Linkability**: customer_id em case study apenas 3 customers; cardinality safe.
- **Identifiability**: customer name disclosed only per customer-approved attestation.
- **Non-repudiation**: case study customer-approved + Legal-reviewed = forensic-grade.
- **Detectability**: marketing analytics tracked separately (NÃO blocking engineering observability).
- **Disclosure**: press release + blog posts + case studies intentional disclosure transparency.
- **Unawareness**: customer notified per attestation approval per WI-S20-004 + WI-S20-005 cumulative.
- **Non-compliance**: SOC 2 + LGPD + GDPR + CCPA + EDPB SCCs cumulative (case studies sanitized + customer-approved).

## 14. Dependencies

### Hard blockers (soft-gate; NÃO blocking engineering gate)
- WI-S20-004 lighthouse customer attestations testimonials + case studies (Marketing co-led com Customer Success).
- WI-S20-005 Legal externo review path (Cooley/DLA Piper/Bird & Bird) for press release + case studies.
- 14 canonical sources verification matrix (per WI-S20-001) for blog posts technical references.

### Soft blockers
- Cumulative S-13..S-19 SEALED (technical content depth para blog posts).

### Outbound
- Sprint S-20 launch orchestration soft-gate (não blocking engineering gate).
- GA announcement D+30..D+60 launch day (per CEO/Founder decision; launch pode shift date sem affecting engineering readiness).

## 15. PERT estimate

| Estimate | Hours | Notes |
|---|---|---|
| **Optimistic (O)** | 16h | Press release + 5 blog posts + 3 case studies + Product Hunt assets smooth. |
| **Most Likely (M)** | 24h | Press release + 5 blog posts + 3 case studies Marketing co-led + Product Hunt assets + Maker outreach. |
| **Pessimistic (P)** | 38h | Legal review iteration + customer-approved iteration + PR firm contract iteration. |
| **PERT** | (16 + 4×24 + 38) / 6 = **25.0h** | Per spec contract §12. |
| **Variance (σ²)** | ((38-16)/6)² = 13.4 | Std dev ≈ 3.7h. |

## 16. Sign-off canonical (HIGH_RISK 13)

13 roles per spec contract §5.1; **lane HIGH_RISK herdado do sprint** (launch orchestration soft-gate mas WI mantém HIGH_RISK lane consistency).

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD_ | _pending_ | _pending_ |
| 6 | Engineer (S-20 lead; blog posts technical references review) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product (Marketing-led; PR firm OR CMO contracted; Customer Success co-led case studies primary) | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD_ | _pending_ | _pending_ |
| 10 | Privacy Officer (case studies sanitized + customer-approved review) | _TBD_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD_ | _pending_ | _pending_ |
| 12 | Legal Counsel (press release + case studies Legal-reviewed primary; reuso WI-S20-005 Cooley/DLA Piper/Bird & Bird ~$15-30k 6-week lead) | _TBD via Cooley/DLA Piper/Bird & Bird_ | _pending_ | _pending_ |
| 13 | Finance (PR firm contract budget OR CMO contracted; Product Hunt assets production budget) | _TBD_ | _pending_ | _pending_ |

## 17. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-29 | Gustavo (via Claude Opus 4.7) | Criação WI-S20-008 (cycle 12.S20.0; launch orchestration prep — press release + 5 blog posts + 3 case studies + Product Hunt launch prep + Maker outreach; **separated from engineering gate canonical** per spec contract §1 codex finding + §6.2; **soft-gate NÃO blocking engineering gate binary GA-go decision**; CEO/Founder enforce gate; launch pode shift date sem affecting engineering readiness). |

---

**Fim WI-S20-008.** **S-20 sprint full WI corpus completo (8/8 WIs SOTA HIGH_RISK; engineering gate WIs 001..007 binary GA-go blocking + WI-S20-008 launch orchestration soft-gate NÃO blocking).**
