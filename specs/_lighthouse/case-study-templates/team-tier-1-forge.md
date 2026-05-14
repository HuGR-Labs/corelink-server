---
id: "CASE-STUDY-TEMPLATE-TEAM-FORGE"
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
tags: ["lighthouse", "s20", "case-study", "template", "team-tier", "forge", "customer-zero", "governance"]
---

# Case Study Template — Team Tier #1 (Forge, Customer-Zero)

> **Slot:** `LH-FORGE` · **Tier:** Team · **Customer-zero per spec contract §5.1**
> **Audience:** internal-first (HuGR org self-attestation); excerpts re-usable in external marketing with caveat.

---

## 1. Problem statement (≤ 200 words)

_Describe the pre-CoreLink state of the Forge engineering org: build minutes per developer per day, cache miss ratio on shared CI, dev-loop friction. Anchor to specific numbers from Forge's pre-baseline measurements (run for ≥ 2 weeks before migration)._

Suggested skeleton: "Forge's engineering team of N runs Bazel builds X times per day across L laptops + M CI shards. Pre-CoreLink, each developer experienced ~Y minutes of cache-miss wall time per build, costing Z developer-hours per week..."

---

## 2. Before-state metrics (filled from baseline measurements)

| Metric | Pre-CoreLink baseline |
|---|---|
| Build minutes per dev per day | _XX min_ |
| Cache hit ratio (shared) | _XX%_ |
| Cache backend | _S3 + bazel-remote-cache (self-hosted in `us-east-1`)_ |
| GB stored at peak | _XX GiB_ |
| Cache invalidation incidents / month | _X_ |
| Engineering team size | _N engineers_ |

---

## 3. Integration story (≤ 400 words)

_Narrate the migration day (D+10..D+15 of Forge cohort): the signup flow, DPA acceptance, tier selection (Team), first PAT issuance, first CAS PUT. Specifically call out:_

- _Friction points (what surprised the customer-zero team)._
- _Self-service quality (did Forge need engineering help, or was the docs path sufficient?)._
- _Time-to-first-CAS-PUT from signup click._
- _Any bugs filed during the first week + how fast they were resolved._

Customer-zero status means this section doubles as **internal QA evidence** for the onboarding flow — capture honest friction, not just the polished narrative.

---

## 4. Results (30d observation)

| Metric | Pre-CoreLink | Post-CoreLink (30d) | Delta |
|---|---|---|---|
| Build minutes per dev per day | _XX min_ | _YY min_ | _-Z%_ |
| Cache hit ratio | _XX%_ | _YY%_ | _+Z pp_ |
| Cache invalidation incidents / month | _X_ | _Y_ | _-Z_ |
| Developer NPS (build experience) | _X / 10_ | _Y / 10_ | _+Z_ |

### 4.1 SLA evidence

(Pulled from `lighthouse_sla_samples` table — see `sla-attestation-template.md` §2.)

| SLO | Target | Forge actual (30d) |
|---|---|---|
| SLO-AVAIL-CAS-PUT | ≥ 99.9% | _99.9X%_ |
| SLO-AVAIL-CAS-GET | ≥ 99.9% | _99.9X%_ |
| SLO-LAT-CAS-GET p99 | < 300 ms | _XXX ms_ |
| SLO-FRESH-BILLING | < 0.1% drift / 24h | _0.0X%_ |

---

## 5. Customer quote (slot)

> _"[Quote from Forge engineering lead — 2-3 sentences endorsing CoreLink quality + production-grade posture]"_
>
> — _Name, Role_, Forge

---

## 6. Technical architecture diagram (slot)

```
[ insert Mermaid / SVG diagram here ]

Forge devs ──► Bazel CLI ──► CoreLink CAS (wnam primary)
                                      │
                                      ├─► R2 storage (encrypted)
                                      ├─► Dedup + chunk store
                                      └─► Audit chain emit
```

Customer-zero diagram is intentionally simpler than Enterprise BYOK (no KMS provider sidebar, no DPA Schrems II overlay).

---

## 7. Future plans (≤ 100 words)

_What does Forge plan to do next with CoreLink? (Multi-region expansion, ML model checkpoint storage, BYOK upgrade, etc.)_

---

## 8. Publication metadata

| Field | Value |
|---|---|
| Customer-approved | _yes / no_ |
| Legal review status | _approved / in review_ |
| Sanitization required | _no (internal customer-zero)_ |
| Publication venue | _CoreLink website / docs / internal-only_ |
| Embargo | _none / lift after GA D+60_ |
| Co-authors | _CoreLink Marketing, Forge engineering lead_ |

---

**Template for case study; ALWAYS instantiate per real customer attestation. Internal customer-zero status warrants tighter copy review but no Legal-sensitive sanitization.**
