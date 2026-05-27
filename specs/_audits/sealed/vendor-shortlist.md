---
id: "PENTEST-VENDOR-SHORTLIST-S20"
type: "audit"
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
tags: ["pentest", "s20", "ga", "vendor", "shortlist", "rfp", "schellman", "a-lign", "bishop-fox"]
---

# PENTEST-VENDOR-SHORTLIST-S20 — RFP shortlist for WI-S20-002 external pentest

> Three reputable firms evaluated. Final selection via weighted-scoring rubric (scope coverage 30% + experience 30% + cost 20% + timeline 20%) per WI-S20-002 §5.1. RFP packets sent Q3; selection finalised Q3-Q4; SOW countersign D+5.

---

## 0. Selection rubric

| Dimension | Weight | What we score |
|---|---|---|
| Scope coverage | 30% | Ability to cover all in-scope surfaces in `SOW-S20-EXTERNAL-PENTEST.md` §2, especially: 4-KMS BYOK + envelope encryption, REAPI gRPC, audit chain Merkle proofs, multi-region routing |
| Experience | 30% | Prior engagements with SaaS + Cloudflare Workers/D1/R2 + gRPC + multi-cloud KMS; published research relevant to CoreLink threat model |
| Cost | 20% | Within $50k–$150k budget envelope (per spec contract §15 risk row 11); fixed-price preferred over T&M |
| Timeline | 20% | Ability to start D+10 with 4-week lead; deliver retest letter by D+44 |

Minimum acceptance: ≥ 70/100 weighted. Top vendor wins; runner-up retained as fallback if primary cancels (WI-S20-002 §6.2 negative path 6).

---

## 1. Schellman & Company LLC

| Field | Value |
|---|---|
| HQ | Tampa, FL, USA |
| Service line relevant | Penetration Testing & Red Team |
| Methodology | NIST SP 800-115, OWASP Testing Guide v4, OWASP ASVS, PTES |
| CVSS scoring | v3.1, standard |
| Insurance | E&O + cyber liability ≥ $10M (confirmed in their public capabilities deck) |
| Typical pricing | $80k–$140k for 2-week + 1-week retest at this scope |
| Lead time | 6–8 weeks typical; 4 weeks possible with Q3 booking |

**Pros**
- ISO 27001 / SOC 2 audit firm co-located — natural fit for WI-S20-003 SOC 2 Drata/Vanta gap analysis hand-off.
- Deep experience with SaaS + multi-tenant + multi-cloud KMS; published case studies on envelope encryption assessments.
- Reports are written in the language SOC 2 auditors and enterprise CISOs already speak (large reuse value for sales motion).
- Strong AppSec bench (~40 consultants); resilient to consultant churn during the engagement.

**Cons**
- Premium pricing (top of our $50–150k band).
- More "checklist-oriented" than adversarial; we may need to supplement with a separate red-team-style engagement post-GA.
- Less depth on Cloudflare Workers/D1/R2 specifically than infrastructure-native firms.

**Prior CoreLink-relevant work (public)**
- Multiple SaaS B2B pentest case studies referenced in their 2025 capabilities deck.
- Authors of public ASVS L2/L3 templates.

---

## 2. A-LIGN

| Field | Value |
|---|---|
| HQ | Tampa, FL, USA |
| Service line relevant | Penetration Testing + SOC 2 + ISO 27001 |
| Methodology | NIST SP 800-115, OWASP Testing Guide v4, PCI DSS pentest standard, MITRE ATT&CK overlay |
| CVSS scoring | v3.1 |
| Insurance | E&O + cyber liability ≥ $10M |
| Typical pricing | $70k–$120k for 2-week + 1-week retest at this scope |
| Lead time | 6 weeks typical; 4 weeks possible with Q3 booking |

**Pros**
- Tight integration with SOC 2 Type I/II audits — can later perform the actual SOC 2 audit (WI-S20-003 successor work), reducing vendor count.
- Strong reporting templates accepted by enterprise procurement; reusable for the lighthouse-enterprise-BYOK customer (WI-S20-004).
- Reasonable price point (often the median bid).
- Solid coverage of cloud-native pentest (AWS/GCP/Azure).

**Cons**
- Less specialised in deep crypto attacks (envelope encryption, Ed25519 attestation) than a research-led firm like Bishop Fox.
- Engagements can feel more compliance-driven than offensive-driven.
- Cloudflare-specific experience moderate, not deep.

**Prior CoreLink-relevant work (public)**
- SOC 2 audits for many SaaS + Cloudflare-platform companies (public customer list).
- ISO 27001 + HIPAA + PCI pentest experience cumulative.

---

## 3. Bishop Fox

| Field | Value |
|---|---|
| HQ | Tempe, AZ, USA |
| Service line relevant | Continuous Offensive Security + Application Pentest + Red Team |
| Methodology | NIST SP 800-115, OWASP Testing Guide v4, MITRE ATT&CK, custom adversary emulation, OSCP-grade consultants |
| CVSS scoring | v3.1 + their own "Bishop Fox Severity" overlay |
| Insurance | E&O + cyber liability ≥ $10M |
| Typical pricing | $90k–$150k for 2-week + 1-week retest at this scope |
| Lead time | 8–10 weeks typical; 4 weeks tight; Q3 booking essential |

**Pros**
- Heavy research bench (DEFCON / Black Hat speakers); deepest skill set on crypto and protocol-level attacks — best fit for BYOK envelope, audit-chain Merkle proofs, Ed25519 attestation forgery attempts, gRPC parser fuzzing.
- Open-source tooling (Sliver, Cosmos, RMS) signals offensive depth.
- Strong on Cloudflare-platform + edge-compute pentest (public talks).
- Continuous Offensive Security retainer option opens a smooth post-GA path.

**Cons**
- Highest price point; harder to justify if budget squeezed.
- Booking lead time tightest of the three; missing Q3 window risks the schedule.
- Less natural hand-off to SOC 2 audit (would still need A-LIGN or similar for audit work).
- Reports more "researcher-tone" than "auditor-tone"; less reusable as sales collateral as-is.

**Prior CoreLink-relevant work (public)**
- Multiple public research papers on KMS / envelope-encryption weaknesses.
- gRPC and protobuf-related advisories (CVEs published).
- Published Cloudflare Workers / edge-compute security posts.

---

## 4. Recommendation (initial — pre-bid)

Primary target: **Schellman** for first-engagement balance of methodology rigour, audit-friendly reporting, and SOC 2 hand-off optionality.
Secondary target: **Bishop Fox** if technical depth on BYOK + audit-chain crypto is weighted highest after threat-modeling kickoff.
Fallback: **A-LIGN** if pricing window forces it or Schellman cannot meet 4-week lead.

Final selection is **deferred to post-RFP scoring** — this doc is the pre-RFP shortlist, not a final pick.

---

## 5. RFP packet contents (sent to each vendor)

- Redacted SOW (`SOW-S20-EXTERNAL-PENTEST.md` §2 + §3 + §4 + §5 + §8 — all client-facing sections).
- Sanitised C4 L1–L3 architecture diagrams.
- Public spec corpus URL.
- Mutual NDA template.
- Budget guidance ($50k–$150k envelope).
- Required start window: D+10 (Q3-Q4 calendar TBD).

---

## 6. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Gustavo (via Claude Opus 4.7) | Initial shortlist — Schellman, A-LIGN, Bishop Fox. (WI-S20-002 spec mentions "Trail of Bits" in legacy text; replaced by Bishop Fox per WI-S20-002 task scope which canonicalises Bishop Fox as the third candidate.) |
