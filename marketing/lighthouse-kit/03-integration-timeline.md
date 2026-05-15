---
id: "LIGHTHOUSE-KIT-03-INTEGRATION-TIMELINE"
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
tags: ["lighthouse", "marketing", "timeline", "integration", "onboarding", "60d"]
---

# 03 — Lighthouse Integration Timeline (D+0 → D+60)

> **Use:** Shared with the customer immediately after NDA + LOI signature so both sides see the schedule contract.
> **Calendar reference:** D+0 = "intro call + NDA signed" day. All offsets are calendar days.
> **Mapping to state machine:** see `crates/corelink-lighthouse-tracker/src/lib.rs` and `specs/_lighthouse/lighthouse-customer-program.md` §6.

---

## Overview

| Window | Phase | Owner | Customer effort | Exit gate |
|---|---|---|---|---|
| D+0 | Kick-off | CoreLink CS + Legal + Customer signatory | 1 hour | NDA + intro call done |
| D+1..D+7 | Technical scoping | CoreLink Engineer S-20 lead + Customer eng lead | 4 hours | Scope doc agreed |
| D+8..D+10 | Onboarding | CoreLink CS engineer + Customer ops/SRE | 3 hours | First CAS PUT measured + first cache hit observed |
| D+10..D+40 | 30-day observation | CoreLink SRE on-call + CS | 30 min / week (4 check-ins) | No SLA breach, SLO dashboard shared |
| D+40..D+45 | SLA attestation | Customer SRE/ops + procurement; CoreLink countersignatories | 2 hours | Attestation form signed |
| D+45..D+60 | Case study | CoreLink marketing + CS; customer reviewer | 2 hours (interview) + 1 hour (review) | Case study published |

**Total customer effort: ≤ 8 hours of engineering / leadership time across the 60-day engagement** (matches the `lighthouse-customer-program.md` §5.1 commitment).

---

## D+0 — Kick-off

**Goal:** NDA signed, intro call complete, dedicated CoreLink team introduced, schedule contract acknowledged.

**Activities:**
- NDA executed (DocuSign envelope id captured in CRM).
- Intro call (30 min walk-through of `02-intro-deck.md`).
- Customer signatory introduced to: Customer Success engineer (CS), Engineer S-20 lead (technical), Privacy Officer (DPA-related — Enterprise only), Legal Counsel (procurement liaison).
- Shared workspace created (Slack Connect / Teams / email DL — customer's preference).
- This timeline doc shared as schedule contract.

**State machine:** `Recruiting → Engaged`.

**Deliverable:** signed NDA + LOI + this `03-integration-timeline.md` acknowledged.

---

## D+1..D+7 — Technical scoping

**Goal:** Capture use case, integration points, region selection, BYOK provider (Enterprise), and migration plan.

**Activities:**
- Scoping call 1 (60 min, D+1..D+2): use case deep dive — Bazel/Buck2/Pants version, current cache backend, pre-CoreLink baseline metrics captured (cold/warm build times, cache hit ratio, monthly CI minutes, contributor / workspace count).
- Scoping call 2 (60 min, D+3..D+4):
  - **Team tier:** CI integration template selected from `templates/ci/`, region pin chosen (wnam / enam / weur / sam), retention policy reviewed.
  - **Enterprise BYOK:** BYOK provider declared (AWS KMS / GCP KMS / Azure KV / Vault); Schrems II TIA started; data residency confirmed; sub-processor list reviewed.
- Scoping doc drafted and shared by D+5 (`docs/internal/lighthouse-migration-{slot}.md` per WI-S20-004 §2.1.3).
- Customer-side approvals in flight by D+7 (eng lead sign-off; for Enterprise: privacy/security review).

**Customer effort:** 2 calls × 60 min + async doc review ≈ 4 hours.

**State machine:** `Engaged → Migrating` once scoping doc is approved.

**Deliverable:** approved scoping doc + region selection + tier confirmed + BYOK provider declared (Enterprise).

---

## D+8..D+10 — Onboarding

**Goal:** Customer in production with first cache hit measured.

**Activities:**
- D+8: account provisioned in target region; workspace created; tier set.
- D+8..D+9: first PAT issued; customer engineering pairs with CS engineer to integrate `templates/ci/` snippet into one CI job.
- D+9: first CAS PUT recorded (state machine emits audit event with evidence URL).
- D+9..D+10: first cache hit observed; cache hit ratio panel populated; weekly Customer Success cadence scheduled (Tuesdays 10:00 customer-TZ unless otherwise agreed).
- D+10: SLO dashboard URL shared (per-customer Grafana embed of the DASH-LIGHTHOUSE-CUSTOMERS panel filtered to their `customer_id`).
- For Enterprise BYOK: BYOK kill-switch chaos drill scheduled to run weekly for 4 weeks during observation window (drills documented in `specs/_runbooks/RB-BYOK-CHAOS-DRILL.md`).

**Customer effort:** 1 pairing session (90 min) + 2 short async syncs ≈ 3 hours.

**State machine:** `Migrating → Observing` once first CAS PUT and first cache hit both recorded.

**Deliverable:** PAT issued + CI integrated + first cache hit measured + SLO dashboard live.

---

## D+10..D+40 — 30-day observation window

**Goal:** Continuous measurement; no SLA breach; weekly cadence with the customer.

**Activities:**
- SRE on-call samples SLA daily (`SlaSample` rows written to `lighthouse_sla_samples` D1 table; 30 samples accumulated by D+40).
- Weekly Customer Success check-in (30 min, follow `04-weekly-checkin-agenda.md`):
  - W1 (D+15): integration health; any unexpected ergonomics.
  - W2 (D+22): mid-window SLO review; flag any softening.
  - W3 (D+29): incident retrospective if applicable; pre-attestation checklist preview.
  - W4 (D+36): attestation prep; assign customer-side signatory + procurement signer.
- Direct PagerDuty escalation rights active for the customer during this window (P0 priority).
- BYOK chaos drills (Enterprise): one per week during W1–W4; kill-switch p99 observed against ≤ 5 min target.
- Any incident encountered is logged in the attestation form §3.1 in real time.

**Customer effort:** 4 × 30 min check-ins = 2 hours.

**State machine:** stays in `Observing`. If `sla_breach_recorded == true`, remediation cycle is triggered and the 30d window restarts on a fresh `observation_started_at`.

**Deliverable:** 30 SlaSample rows collected; SLO dashboard reflecting full window; pre-attestation checklist passed.

---

## D+40..D+45 — SLA attestation

**Goal:** Customer signs the 30-day SLA attestation; CoreLink countersigns.

**Activities (see `05-sla-attestation-instructions.md` for full instructions):**
- D+40: CoreLink CS instantiates `specs/_lighthouse/sla-attestation-template.md` with measured values, files draft as `specs/_audits/2026-XX-XX-lighthouse-customer-{slot}-attestation.md`.
- D+41..D+42: customer SRE / ops fills §2 actuals (cross-checked against their own observability stack) and §3.1 / §3.2 / §3.3.
- D+42..D+43: customer procurement / Legal reviews §4 aggregate attestation; signs §5.1.
- D+44..D+45: CoreLink countersignatures captured (CS, Engineer S-20 lead, Privacy Officer if DPA-related, Legal Counsel).
- D+45: attestation `audit_status: ACTIVE`; state machine transition recorded.

**Customer effort:** ~2 hours total across SRE/ops, procurement, and signatory.

**State machine:** `Observing → Attested`.

**Deliverable:** signed attestation filed; audit chain row emitted with `event_type = lighthouse_lifecycle_transition`, `to_state = Attested`, `evidence_url = <attestation md path>`.

---

## D+45..D+60 — Case study

**Goal:** Customer-approved case study published; testimonial captured.

**Activities:**
- D+46: 60-min case study interview (script: `06-case-study-interview-script.md`).
- D+48..D+52: marketing drafts the case study using the relevant template from `specs/_lighthouse/case-study-templates/`.
- D+53: draft sent to customer for review (architecture diagram sanitized; quotes pre-approved).
- D+54..D+58: customer review cycle; max 2 round trips.
- D+59..D+60: published — OSS = public on marketing site; Enterprise = NDA-distributed sanitized PDF.

**Customer effort:** 60 min interview + ~1 hour review ≈ 2 hours.

**State machine:** `Attested → CaseStudySigned`.

**Deliverable:** case study live + reference-call commitment captured (up to 2 / quarter for 12 months).

---

## Reference table — deliverables per phase

| Phase | Customer deliverable | CoreLink deliverable |
|---|---|---|
| D+0 | NDA + LOI signed | NDA executed; team introduced |
| D+1..D+7 | Approved scoping doc + region + BYOK declared | Scoping doc; migration plan; templates picked |
| D+8..D+10 | CI integration committed | PAT issued; SLO dashboard URL; first cache hit confirmed |
| D+10..D+40 | Weekly check-ins attended | Daily SLA sample + weekly notes + dashboards live |
| D+40..D+45 | §2 actuals + signatures | Drafted attestation form + countersignatures |
| D+45..D+60 | 60-min interview + review | Drafted + published case study |

---

## Risks + escalation paths

| Risk | Owner | Mitigation |
|---|---|---|
| Customer SRE unavailable for SLA sampling | Customer Success | CoreLink SRE on-call samples and shares dashboard; customer attests against shared data |
| SLA breach mid-window | SRE-lead | Remediation cycle; `Observing` restarts on fresh 30d window after fix; customer informed within 24h |
| Procurement delays signature past D+45 | Legal | Soft deadline D+45, hard deadline D+50; if exceeded, escalate to founder for direct procurement-to-procurement call |
| Case study review stalls | Marketing | 2-round-trip cap; if more, founder escalation; sanitized version still publishable |
| Customer desists mid-engagement | Customer Success + Founder | Backup slot activated (`LH-OSS-02` / `LH-ENT-BYOK-02`) per `lighthouse-customer-program.md` §2 |

---

## Communication channels

- **Async:** shared Slack Connect channel `#corelink-{slot}` (or Teams equivalent).
- **Tickets:** customer-visible Jira project; CoreLink-internal tracking in `corelink-lighthouse-tracker`.
- **Incidents:** PagerDuty direct paging (P0 priority during D+10..D+40).
- **Weekly check-in:** recurring 30-min calendar invite from D+15 onwards.
- **Escalation:** Customer Success engineer → CS lead → Founder. Each step responds within 4 business hours.

---

**Fim 03-INTEGRATION-TIMELINE.**
