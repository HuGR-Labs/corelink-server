---
id: "LIGHTHOUSE-RECRUITMENT-SHORTLIST"
type: "governance"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-S20-004"
tags: ["lighthouse", "s20", "recruitment", "shortlist", "outreach", "oss", "enterprise", "governance"]
---

# Lighthouse Customer Recruitment Shortlist (WI-S20-004)

> **Slots to fill:** 2 Team (Forge confirmed + 1 OSS) + 1 Enterprise BYOK
> **LOI target:** D-60 (Q3) before sprint S-20 start
> **Backup target:** 2 backup candidates per open slot (LOI-engaged but not migrated)

> ⚠️ **Confidentiality**: company names below are PUBLICLY visible projects (OSS) or generic ICP descriptions (Enterprise). Specific contacts and signed-LOI status live in the Sales CRM, NOT in this repo.

---

## 1. Selection rubric

Per `lighthouse-customer-program.md` §5, candidates are scored on five axes (1-5 scale, 25 max):

| Axis | What we measure | Weight |
|---|---|---|
| **Fit** | Active Bazel/Buck2/Pants + existing remote cache need | × 1.5 |
| **Reach** | Engineering team size + GitHub stars (OSS) or ARR class (Enterprise) | × 1.0 |
| **Engagement** | Maintainer/CTO responsiveness in initial outreach | × 1.0 |
| **Story** | Migration narrative compelling for case study | × 1.0 |
| **Risk** | DPA/compliance/legal complexity (lower = higher score) | × 0.5 |

Minimum aggregate score for outreach: **18/25** (≈ 70%). Below threshold = pass.

---

## 2. Team-tier candidates (OSS Bazel/Buck2 ecosystem)

Target: 1 confirmed (slot `LH-OSS-01`) + 2 backups. Outreach via GitHub issue tracker, maintainer email from `MAINTAINERS.md`, and OSS community Discord/Slack.

| # | Candidate | Build tool | Why | Outreach channel | Owner |
|---|---|---|---|---|---|
| 1 | **Buildfarm** (`bazelbuild/bazel-buildfarm`) | Bazel RBE | Reference Bazel RBE impl; their maintainers are the ecosystem authority. CoreLink as managed cache backend = clean story. | GitHub Discussions + maintainer email | Customer Success |
| 2 | **rules_rust** maintainers (`bazelbuild/rules_rust`) | Bazel | Heavy CI build minutes; clear pain point. | GitHub issue + maintainer email | Customer Success |
| 3 | **Buck2 ecosystem project** (e.g. `facebook/buck2`-adjacent) | Buck2 | Validates Buck2 cas adapter path. | Discord (Buck2 community) | Customer Success |
| 4 | **Tilt** (`tilt-dev/tilt`) | Bazel + custom | Dev-loop tool with shared cache narrative. | GitHub + Slack | Customer Success |
| 5 | **Tweag rules_haskell maintainers** | Bazel | Polyglot ecosystem signal. | GitHub + email | Customer Success |
| 6 | **Wix Open Source** (`wix/exodus`) | Bazel | Enterprise OSS that has talked publicly about cache pain. | LinkedIn + GitHub | Customer Success |
| 7 | **Lyft engineering** (Bazel monorepo) | Bazel | High-profile monorepo case (if engagement realistic). | Eng-to-eng warm intro | Founder |

Initial outreach (D-60..D-45): top 5 contacted with the standard Lighthouse Program brief (6mo free + dedicated engineer + roadmap influence in exchange for attestation + case study).

Backup pool: any candidate that returns a non-rejecting response but doesn't sign LOI by D-30 is kept warm as a backup.

---

## 3. Enterprise BYOK candidates (ICP pre-revenue Q3)

Target: 1 confirmed (slot `LH-ENT-BYOK-01`) + 2 backups. Outreach via founder warm intros, conference networking, and outbound from Sales SDR funnel.

| # | ICP profile | Vertical | Why BYOK | Stage | Contact owner |
|---|---|---|---|---|---|
| 1 | Series B/C **fintech** with EU customers | Financial services | EBA + Schrems II → BYOK mandatory | Founder warm intro | Founder |
| 2 | Series B **healthtech** with HIPAA + GDPR posture | Healthcare | HIPAA BAA + GDPR Art. 32 | Founder warm intro | Founder |
| 3 | Series C **dev-tools** with self-hosted enterprise tier | DevOps | Customer demand for BYOK on their own infra | Outbound SDR | Sales |
| 4 | Pre-IPO **AI/ML platform** (model checkpoint storage) | AI/ML | Provenance + sovereignty concerns | Founder warm intro | Founder |
| 5 | **Government contractor** with FedRAMP-Moderate ambitions | GovTech | FedRAMP → BYOK with FIPS 140-2 backed KMS | Conference networking | Founder |
| 6 | **EU automotive** with autonomous-driving compute | Automotive | EU sovereignty + data-residency | Founder warm intro | Founder |

Engagement budget: 4 weeks per candidate (DPA review + Schrems II TIA + technical fit assessment + pricing approval).

---

## 4. Outreach plan (D-60 → D+0)

| Week | Activity | Owner | Output |
|---|---|---|---|
| D-60 | Send Lighthouse Program brief to top-5 OSS + top-5 Enterprise candidates | Customer Success + Founder | 10 brief responses tracked in CRM |
| D-55 | Scoping calls with respondents (30 min) | Customer Success + Founder | Fit assessment per candidate |
| D-50 | Send LOI draft to top-3 OSS + top-3 Enterprise | Customer Success + Legal | LOIs in legal review per candidate |
| D-45 | Backup outreach for any non-respondents | Customer Success | 2 backup candidates in scoping |
| D-30 | LOI signature deadline | Founder | 1 OSS + 1 Enterprise LOI signed (target); 2 backups Engaged |
| D-21 | DPA review (Enterprise) starts | Privacy Officer + Legal | DPA amendment Schrems II TIA in flight |
| D-14 | Technical fit assessment (BYOK provider, region, tier) | Engineer S-20 lead + Customer | Migration plan drafted per customer |
| D-7 | Pre-migration kick-off call | Engineer + Customer Success + Customer | Migration day scheduled |
| D+0 | Sprint S-20 starts; Forge slot already at `Migrating` | All | LH-FORGE state = Migrating |

---

## 5. Risk register (recruitment-specific)

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| No OSS maintainer responds within 14d | M | H | Multiple outreach channels (GitHub + email + Discord); 5 candidates not 1 |
| Enterprise DPA review > 6 weeks | M | H | Start D-45 (well before sprint); have 2 backups engaged |
| Top candidate desists post-LOI | L | H | 2 backups per slot; state machine supports backfill `Engaged → Migrating` |
| Pricing not approved at Enterprise BYOK (custom contract) | M | M | Founder + Finance pre-approve a pricing band before outreach |
| Legal review of case study takes > 30d post-attestation | M | M | Start Legal review at D+30 in parallel with observation final week |

---

## 6. Status tracking

Slot state is persisted in D1 `lighthouse_customers` (migration `0042`). This document is the *human-readable* shortlist; the *machine-readable* truth is in the Rust state machine + D1 table.

Status snapshot (filled by Customer Success weekly during recruitment phase):

| Slot | Candidate | State | LOI signed | Backup engaged |
|---|---|---|---|---|
| `LH-FORGE` | Forge (HuGR org) | _Engaged_ | n/a (internal) | n/a |
| `LH-OSS-01` | _TBD from §2_ | _Recruiting_ | _pending_ | _TBD_ |
| `LH-ENT-BYOK-01` | _TBD from §3_ | _Recruiting_ | _pending_ | _TBD_ |

---

**Fim LIGHTHOUSE-RECRUITMENT-SHORTLIST.**
