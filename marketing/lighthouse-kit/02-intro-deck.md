---
id: "LIGHTHOUSE-KIT-02-INTRO-DECK"
type: "marketing"
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
tags: ["lighthouse", "marketing", "deck", "intro", "pitch", "slides"]
---

# 02 — 12-Slide Intro Deck Outline

> **Format:** Pitch.com or Google Slides; 16:9; one talking point per slide; ≤ 30 words on slide, the rest in speaker notes.
> **Run-time:** 18 min walk-through + 12 min Q&A in the 30-min intro call.
> **Source of facts:** every number must be traceable to a spec doc or live observability panel. Placeholders explicit when the metric is pending real measurement.

---

## Slide 1 — What is CoreLink

**Header:** "CoreLink — managed content-addressable storage for build, package, and ML pipelines."

**One-liner:** "BLAKE3-keyed, multi-region, audit-chain-backed cache that drops into Bazel and OCI/ML toolchains in under an hour — Turborepo and sccache too, with Buck2 and Pants on the roadmap."

**Speaker notes:**
- We are the cache layer your CI already wants — but with multi-region failover, BYOK, and an Ed25519-signed audit chain.
- Customer-zero is HuGR Forge (our own monorepo); the same control plane serves Team and Enterprise tiers.

---

## Slide 2 — Customer-zero stats (HuGR Forge)

**Header:** "We run on our own product."

**Stats table (placeholders — pending real numbers at slide-instantiation time):**

| Metric | Forge (placeholder) |
|---|---|
| Cache hit ratio (30d avg) | _XX.X%_ |
| P99 cache-GET latency | _XXX ms_ |
| Monthly GiB stored | _X XXX GiB_ |
| CI minutes saved / week | _XX,XXX min_ |
| Availability (rolling 30d) | _99.9X%_ |

**Speaker notes:**
- Numbers are pulled fresh from the DASH-FORGE Grafana panel each time the deck is exported.
- "Customer-zero" means HuGR's own engineering team is on the production GA control plane, not a staging fork.

---

## Slide 3 — SOTA differentiators

**Header:** "What we ship that the rest of the cache market does not."

**Four-quadrant slide:**

| | |
|---|---|
| **BYOK** — AWS KMS / GCP KMS / Azure KV / Vault, ≤ 5-min kill-switch chaos-drilled weekly | **Audit chain** — Ed25519-signed, append-only, externally verifiable |
| **WCAG 2.2 AA** — admin UI accessible end-to-end (axe-core 0 violations) | **3 locales** — en/pt/es first-class i18n with ICU MessageFormat |

**Speaker notes:**
- These four are the differentiators the lighthouse program is calibrated against.
- BYOK kill-switch is operationally tested, not just documented.
- WCAG 2.2 AA matters for public-sector and EU procurement gates.

---

## Slide 4 — Team tier feature deep-dive (1 of 2)

**Header:** "Team tier — built for Bazel OSS and product teams (Buck2/Pants on the roadmap)."

**Bullets:**
- Native bazel-remote-cache over REST (Bazel REAPI v2 — no gRPC; workerd has no HTTP trailers support)
- Turborepo REST integration
- sccache over WebDAV
- Buck2 CAS adapter and Pants v2 lifted-cache shim — **roadmap, not yet shipped**
- Multi-region (wnam, enam, weur, sam) with geo-aware routing
- Sub-300 ms P99 cache-GET globally

**Visual:** simple architecture diagram — CI runner → CoreLink edge → R2-backed CAS, with one arrow showing failover to second region.

---

## Slide 5 — Team tier feature deep-dive (2 of 2)

**Header:** "Team tier — operational properties."

**Bullets:**
- 99.9% availability SLO on PUT and GET (cas)
- Per-PAT scoped credentials with rotation API
- 30-day default retention; configurable per workspace
- Cache hit ratio dashboard out-of-the-box (Prometheus + Grafana export)
- Self-serve onboarding: CI integration template in `templates/ci/`

---

## Slide 6 — Enterprise BYOK feature deep-dive (1 of 2)

**Header:** "Enterprise BYOK — when your auditor reads the cache config."

**Bullets:**
- BYOK matrix: AWS KMS, GCP KMS, Azure KV, HashiCorp Vault (4/4 tested)
- ≤ 5-minute kill-switch chaos drill executed weekly in production
- Per-tenant key isolation; tenant cannot read another tenant's CAS even with root-level platform credentials
- Ed25519-signed audit chain over every CAS PUT, every PAT issue, every DSR action
- DPA amendment with Schrems II TIA; sub-processor list maintained at `compliance/sub-processors/`

---

## Slide 7 — Enterprise BYOK feature deep-dive (2 of 2)

**Header:** "Enterprise BYOK — compliance posture."

**Bullets:**
- SOC 2 Type 1 audit kickoff in flight (auditor engagement letter on request)
- External pentest report available under NDA (last pass D-30; remediation log in `compliance/pentest/`)
- DSR (data subject request) erasure SLO ≤ 30 days with Ed25519-signed attestation
- Data residency by region pin; no cross-region copy without explicit tenant approval
- Sub-processor change notification: 30 days advance notice contractually committed

**Speaker note:** explicitly say "Type 1, not Type 2 yet — Type 2 audit window opens post-GA". Do not promise Type 2.

---

## Slide 8 — Lighthouse program offer

**Header:** "The lighthouse customer offer."

**Three-column slide:**

| You give us | We give you | Timeline |
|---|---|---|
| Signed 30-day SLA attestation | 6 months free on your tier | D+0 intro → D+45 attestation |
| Co-authored case study | Dedicated onboarding engineer for the first 30 days | 60-day kit |
| Up to 2 reference calls / quarter for 12 months | Direct PagerDuty escalation rights during observation window | First-year roadmap influence |
| Public testimonial (OSS) OR sanitized testimonial (Enterprise) | Co-marketing on launch | Engagement begins on NDA signing |

---

## Slide 9 — 30-day SLA attestation requirement

**Header:** "What you actually sign at D+45."

**Anatomy of the attestation:**
- §1 Customer profile (slot, tier, region, baseline)
- §2 SLO actuals measured over 30 days (availability, P99 latency, billing reconciliation drift, DSR latency)
- §2.2 BYOK-specific (Enterprise only — kill-switch p99, key-health-at-attestation, matrix-test result)
- §3 Incidents disclosed + DSR results + weekly Customer Success notes
- §4 Aggregate attestation checkboxes
- §5 Signatures (customer signatory + CoreLink Customer Success + Engineer S-20 lead + Privacy Officer + Legal)

**Speaker note:** "We do not ask you to sign a marketing claim. You sign your own observability stack's numbers." Reference `specs/_lighthouse/sla-attestation-template.md`.

---

## Slide 10 — Roadmap visibility (post-GA)

**Header:** "What's after GA."

**Phase 2 (post-GA, 6–12 months out):**
- Remote Build Execution (RBE) — not just cache, full RBE worker pool
- Bring-your-own-worker for compliance-restricted compute
- ML checkpoint store with content-addressed model versioning
- Buck2 native integration (CAS adapter) and Pants v2 support: Skylark cache hooks, plus the base CAS adapters themselves

**Lighthouse benefit:** "You get quarterly roadmap-review calls and your feedback is committed to the RFC pipeline before public disclosure."

**Speaker note:** roadmap items are directional, not contractual. Do not commit dates.

---

## Slide 11 — Security & compliance posture

**Header:** "Where we stand today (and where we don't)."

**Two-column slide:**

| In place today | Honest about |
|---|---|
| SOC 2 Type 1 audit kickoff in flight | Type 2 audit window opens after GA, not before |
| External pentest pass (under NDA on request) | Remediation log open for verification |
| BYOK on 4 providers, chaos-drilled weekly | Single-tenant isolation only — multi-tenant BYOK key sharing is out of scope |
| Ed25519-signed audit chain, append-only | Audit chain verification CLI ships at GA; pre-GA verification is internal |
| DPA + Schrems II TIA template ready | Country-specific data residency beyond our 4 regions is roadmap, not GA |

---

## Slide 12 — Next steps / call to action

**Header:** "What happens next."

**Three columns — pick your path:**

| Path A — OSS Team | Path B — Enterprise BYOK | Path C — Not yet |
|---|---|---|
| 1. NDA signed (1 week) | 1. NDA + DPA exchanged (2-3 weeks) | 1. Stay in touch — quarterly newsletter |
| 2. LOI for lighthouse slot | 2. Schrems II TIA review | 2. Reference call when post-GA |
| 3. Technical scoping (1 week) | 3. BYOK provider declared (AWS KMS / GCP KMS / Azure KV / Vault) | |
| 4. Migration day D+0 | 4. Migration day D+0 | |
| 5. 30-day observation begins | 5. 30-day observation begins | |

**Closing line on slide:** "If A or B sounds right, let's pick a migration day before you leave this call."

**Closing speaker note:** ask for the day. The single most effective close is "what week in the next two months works for D+0 migration?"

---

## Appendix slides (optional; only used if relevant in Q&A)

| # | Topic | Trigger |
|---|---|---|
| A1 | Detailed BYOK architecture | Enterprise asks about key derivation / rotation |
| A2 | Audit-chain proof verification CLI | Auditor or compliance person in the room |
| A3 | Multi-region failover sequence diagram | Platform / SRE asks about RTO |
| A4 | Billing reconciliation model | Finance / procurement in the room |
| A5 | OSS-friendly pricing comparison | OSS maintainer asks about post-6mo cost |

---

## Slide-build checklist (before sending to a candidate)

- [ ] Slide 2 numbers populated from latest DASH-FORGE export
- [ ] Slide 4 architecture diagram has the candidate's build tool circled
- [ ] Slide 8 customizes "your tier" to match candidate (Team vs Enterprise)
- [ ] No mention of Type 2 anywhere
- [ ] No specific dollar amounts on any slide (placeholders ok)
- [ ] No real customer names beyond Forge (HuGR's own org)

---

**Fim 02-INTRO-DECK.**
