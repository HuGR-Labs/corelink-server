---
id: "RB-LIGHTHOUSE-PHASE-MANAGEMENT"
type: "runbook"
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
tags: ["runbook", "lighthouse", "phase-management", "s20", "r5-2", "30d-observation", "state-machine", "operational"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §1.

# RB-LIGHTHOUSE-PHASE-MANAGEMENT — Lighthouse Customer Phase Management (Internal)

> **Sibling to** `RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md`. That runbook handles **P0 fire**. This runbook handles the **steady-state lifecycle** — who owns each phase, when state machine transitions flip, the standard comms cadence, and what to do when a soft escalation happens (i.e. not a P0 page, but the engagement still needs intervention).
>
> **Audience:** Customer Success engineer (primary), Engineer S-20 lead, DevRel, Founder.
> **Pairs with customer-facing playbook** at `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md` (the document the account exec hands the customer Day 1).
>
> **State machine canonical** lives in `crates/corelink-lighthouse-tracker/src/lib.rs`. **Never** flip a state from anywhere else — all transitions go through the tracker so the audit chain emits the `lighthouse_lifecycle_transition` event.

---

## 1. Phases at a glance

The 6 lifecycle states map 1:1 onto operational phases. Each phase has a single accountable role (DRI), backup, and standard cadence.

| Phase | State (tracker) | Window | DRI | Backup | Cadence |
|---|---|---|---|---|---|
| 0. Pre-engagement | `Recruiting` | D−60..D+0 | DevRel / Founder | Customer Success lead | Outreach as needed; intro call once booked |
| 1. Onboarding | `Engaged` → `Migrating` | D+0..D+10 | Customer Success engineer | Engineer S-20 lead | D+0 kick-off, D+3 scoping, D+8 onboarding |
| 2. Observation | `Observing` | D+10..D+40 (30d fixed window) | SRE on-call + Customer Success | Engineer S-20 lead | Daily SLA sample (automated), weekly check-in (live) |
| 3. Attestation | `Observing` → `Attested` | D+40..D+45 | Customer Success engineer + Legal | Engineer S-20 lead | Daily sync until signed |
| 4. Case study | `Attested` → `CaseStudySigned` | D+45..D+60 | DevRel / Marketing | Customer Success | Interview D+46, draft D+48..D+52, review D+53..D+58 |
| 5. Steady-state reference | `CaseStudySigned` (terminal happy) | D+60..D+360 | DevRel | Customer Success | Quarterly reference call (up to 2/quarter) |

Withdrawal (`Withdrawn`) is reachable from `Recruiting`, `Engaged`, `Migrating`. From `Observing` onwards, a withdrawal flips the **slot** (not the record) back to `Engaged` for the backup candidate per `lighthouse-customer-program.md` §2 and `corelink-lighthouse-tracker` `FM-LIGHTHOUSE-CUSTOMER-DESISTS-MID-SPRINT`.

---

## 2. Per-phase responsibilities

### Phase 0 — Pre-engagement (`Recruiting`)

| Role | Responsibility |
|---|---|
| **Founder** | Direct outreach to enterprise BYOK candidate (`LH-ENT-BYOK-01`); sign LOI. |
| **DevRel** | OSS Bazel/Buck2 candidate outreach (`LH-OSS-01`); shortlist maintenance in `specs/_lighthouse/recruitment-shortlist.md`. |
| **Customer Success lead** | Track replies, book intro calls, manage CRM. |
| **Engineering** | Out of loop. |
| **Legal** | NDA template ready; on-call for signature. |

**Exit criteria:** NDA + LOI signed → flip `Recruiting → Engaged` via `tracker.transition(slot, LifecycleState::Engaged)`.

### Phase 1 — Onboarding (`Engaged` → `Migrating`)

| Role | Responsibility |
|---|---|
| **Customer Success engineer (DRI)** | Owns the relationship. Schedules calls. Maintains `docs/internal/lighthouse-migration-{slot}.md`. |
| **Engineer S-20 lead** | Technical scoping calls. Region + tier choice. CI template selection. BYOK provider validation (Enterprise). |
| **Privacy Officer** | Enterprise BYOK only: DPA + Schrems II TIA review with customer. |
| **Legal** | Enterprise BYOK only: countersigning DPA addendum if customer has redlines. |
| **Customer Success lead** | Schedule weekly check-in calendar invites from D+15. |

**Exit criteria — `Engaged → Migrating`:** scoping doc approved (D+5..D+7) AND tier confirmed AND region pinned AND (Enterprise: BYOK provider declared).
**Exit criteria — `Migrating → Observing`:** first CAS PUT recorded AND first cache hit observed in `corelink_lighthouse_customer_audit` chain.

Both transitions emit `lighthouse_lifecycle_transition` audit events with `evidence_url` pointing at the audit-chain row of the proof event.

### Phase 2 — Observation (`Observing`, 30 days)

| Role | Responsibility |
|---|---|
| **SRE on-call (DRI for samples)** | Daily `SlaSample` insertion (auto via CF Cron, manual fallback). Page on any sample miss. |
| **Customer Success engineer** | Weekly check-in (30 min) using `marketing/lighthouse-kit/04-weekly-checkin-agenda.md`. Maintains `incident_log` field on the lighthouse record. |
| **Engineer S-20 lead** | On-call escalation target for technical questions from customer SRE. |
| **Privacy Officer** | Enterprise BYOK only: monitor BYOK chaos drill outcomes weekly. |
| **DevRel** | Pre-interview prep: collect quotable moments from check-ins. |

**Daily check (automated):** `lighthouse_sla_samples` row inserted by `corelink-cron` reading per-customer SLO metrics. If any SLO not met → `RB-LIGHTHOUSE-CUSTOMER-INCIDENT` fires + `observation_status_gauge` flips `2 → 0`.

**Weekly check (manual, Customer Success):** review with customer technical lead. Document any soft concerns (latency feels off, ergonomic gripes, doc gaps) in the slot's `incident_log` even if no SLO breach — these feed §3 retro template.

**Exit criteria — `Observing → Attested`:** 30d elapsed AND zero open SLA breach AND (Enterprise: `byok_key_health_ok == true`) AND attestation form signed both sides. **Never** auto-flip on `lighthouse_sla_samples` ≥ 30; operator must explicitly transition with countersigned attestation in hand.

### Phase 3 — Attestation (`Observing` → `Attested`)

| Role | Responsibility |
|---|---|
| **Customer Success engineer (DRI)** | Instantiate `specs/_lighthouse/sla-attestation-template.md` with measured values. File as `specs/_audits/2026-MM-DD-lighthouse-customer-{slot}-attestation.md`. |
| **Customer SRE/ops** (customer-side) | Cross-check §2 actuals against their own observability. Fill §3.1 / §3.2 / §3.3. |
| **Customer procurement / Legal** (customer-side) | Sign §5.1. |
| **CoreLink Engineer S-20 lead** | Countersign §5.2 (technical lead). |
| **CoreLink Privacy Officer** | Countersign §5.3 (DPA-related, Enterprise BYOK only). |
| **CoreLink Legal Counsel** | Countersign §5.4 (binding signature). |
| **Customer Success engineer** | Flip `Observing → Attested` once all signatures captured. Set `audit_status: ACTIVE` on the attestation md. |

**Hard deadline:** D+50. If procurement signature stalls past D+45, escalate to founder for procurement-to-procurement call. If exceeds D+50, file `FM-LIGHTHOUSE-ATTESTATION-STALL` and reconsider slot.

### Phase 4 — Case study (`Attested` → `CaseStudySigned`)

| Role | Responsibility |
|---|---|
| **DevRel (DRI)** | Run 60-min interview (`marketing/lighthouse-kit/06-case-study-interview-script.md`). Draft case study from `marketing/lighthouse-kit/case-study-template/{variant}.md`. |
| **Marketing** | Visual design, sanitized architecture diagrams, publication. |
| **Customer Success engineer** | Customer-side review wrangling. Max 2 round-trips before founder escalation. |
| **Legal** | Enterprise BYOK only: NDA-compliant sanitization review before any public excerpt. |
| **Founder** | Final sign-off on publication. |

**Exit criteria:** customer-approved case study published (OSS → public; Enterprise → NDA-distributed sanitized PDF) AND reference-call commitment captured (up to 2/quarter for 12 months). Flip to `CaseStudySigned`.

### Phase 5 — Reference (post-`CaseStudySigned`)

| Role | Responsibility |
|---|---|
| **DevRel (DRI)** | Quarterly check-in. Coordinate reference calls (up to 2/quarter). |
| **Customer Success engineer** | Account-management continuity for product feedback channel. |
| **Marketing** | Refresh case study with current metrics annually. |

---

## 3. State transitions — when to flip + how

All transitions MUST go through `corelink-lighthouse-tracker::transition()`. **Never** UPDATE the D1 row directly — the tracker emits the audit-chain entry and increments `corelink_lighthouse_lifecycle_transition_total{from_state, to_state}`. Direct DB writes break the audit chain and are caught by INV-AUDIT-APPEND-ONLY TLA+ checker.

| Transition | Trigger | Operator | Pre-flight check | Audit evidence required |
|---|---|---|---|---|
| `Recruiting → Engaged` | NDA + LOI countersigned | Customer Success lead | DocuSign envelope id captured in CRM | DocuSign envelope id in `evidence_url` |
| `Engaged → Migrating` | Scoping doc approved (D+5..D+7) | Engineer S-20 lead | Scoping doc at `docs/internal/lighthouse-migration-{slot}.md` exists + approved | Doc commit hash in `evidence_url` |
| `Migrating → Observing` | First CAS PUT + first cache hit recorded | SRE on-call | `corelink_cas_put_total{customer_id=LH-*} > 0` AND `corelink_cache_hit_total{customer_id=LH-*} > 0` | Audit-chain rows for both events |
| `Observing → Attested` | 30d elapsed + attestation signed both sides + (Enterprise) BYOK key health OK | Customer Success engineer | Attestation md at `specs/_audits/2026-MM-DD-...{slot}-attestation.md` with `audit_status: ACTIVE` | Attestation md path |
| `Attested → CaseStudySigned` | Case study published + reference commitment captured | DevRel | Case study URL + signed reference agreement on file | Case study commit hash |
| `* → Withdrawn` (from `Recruiting/Engaged/Migrating`) | Customer pulls out before observation | Customer Success engineer | Written withdrawal notice from customer | Withdrawal email/letter in `evidence_url` |
| Slot rotation (from `Observing/Attested` if customer pulls reference) | Customer revokes reference agreement | Founder | Written revocation | Revocation notice; new slot record created from backup candidate (`LH-OSS-02` / `LH-ENT-BYOK-02`) starting at `Engaged` |

**Forbidden transitions** (the tracker will return `TrackerError::IllegalTransition`):
- `Observing → Engaged` (no rollback — start a fresh observation window via remediation cycle)
- `Attested → Observing` (post-attestation regressions are P0 incidents, not lifecycle rollbacks)
- `CaseStudySigned → *` (terminal happy state; reference loss is a slot rotation, not a state revert on this record)

If you find yourself wanting a forbidden transition: STOP. File an ADR. Likely you actually want to rotate the slot to a backup candidate while keeping the original record `CaseStudySigned` or `Withdrawn`.

---

## 4. Incident triage protocol (P1 on a lighthouse customer)

For **P0** see `RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md`. This section covers P1 — degraded but not breached.

**P1 trigger conditions:**
- Customer-reported friction that is **not** an SLO miss (ergonomics, doc gap, perceived perf within SLO budget).
- SLO sample at burn rate ≥ 50% of 30d error budget for a single SLO.
- One-off CAS PUT failure that succeeded on retry but customer noticed.
- BYOK chaos drill kill-switch latency ≥ 4 min (target ≤ 5 min — soft signal).

**P1 triage steps:**

1. **Acknowledge** within 4 business hours. Reply on the shared Slack Connect channel within 1 hour even if just to say "acknowledged, investigating."
2. **Classify** within 24 hours:
   - **A — Product gap:** file ticket in customer-visible Jira project; assign to Engineer S-20 lead. NO state machine change.
   - **B — SLO burn warning:** SRE on-call investigates. If burn rate stabilizes, document in `incident_log`; otherwise escalate to P0.
   - **C — Comms/ergonomics:** Customer Success engineer owns. Add to weekly check-in agenda explicitly so customer sees it tracked.
3. **Update the customer** at the **next check-in** at minimum; for class B, update within 24h regardless.
4. **Document** in the lighthouse slot's `incident_log` field. P1s do not page but DO count toward the §5 retro.

**Promote to P0** if any of:
- Same root cause appears 3× in 7 days.
- Burn rate crosses 100% of 30d error budget for any SLO.
- Customer requests a status call outside the normal cadence.
- Customer mentions the word "consider pulling out" in any form.

---

## 5. Comms templates — 5 most-likely escalations

Each template is a starting point. Tone: factual, concrete, no apologies for things outside our control. **Always** include: what happened, what we did, what we'll do, when we'll update again.

### Template 5.1 — Daily SLA sample missed (single SLO, single day)

**To:** customer technical lead (DM + email)
**Subject:** `CoreLink — {SLO_NAME} sample miss on {date}, observation window unaffected`

```
{Name},

Heads-up: our daily SLA sweep flagged a brief SLO miss for {customer_id}
on {date}:

  • SLO: {slo_id} ({e.g. p99 cache GET latency ≤ 300ms})
  • Sample value: {value}  (threshold: {threshold})
  • Duration: {n} samples in the window {start..end} UTC

What we did:
  • {1-line root cause if known, else "investigating root cause"}
  • {1-line mitigation taken}

Impact on your engagement:
  • Your 30-day observation window is preserved as long as the breach does
    not recur. The full state machine rule is in
    crates/corelink-lighthouse-tracker (FM-LIGHTHOUSE-SLA-CLAIM-MISS-30D).
  • If we see a recurring breach pattern, we will reset the observation
    start date and notify you within 24h — that decision is yours to make
    as well; we will not silently reset.

Next update: {next check-in date} or sooner if conditions change.

— {your name}, Customer Success Engineer
```

### Template 5.2 — Customer-reported ergonomic friction (P1 class C)

**To:** customer technical lead (Slack Connect channel)
**Subject:** N/A (in-channel reply)

```
Thanks for flagging this, {Name}. To make sure we have it captured the way
you experience it:

  • Friction: {paraphrase customer's exact words}
  • Reproduction: {what they were doing}
  • Frequency: {one-off | recurring | always}

I've logged this against your slot ({customer_id}) in our internal tracker
and queued it for the next weekly check-in agenda. If it gets in the way
before then, page us via PagerDuty (your P0 paging rights are active
through {observation_end_date}).

I'll bring a concrete answer or proposed fix to {next check-in date}.
```

### Template 5.3 — Mid-window SLA breach (P0 paging)

**To:** customer technical lead **by phone** within 1 hour of detection; written follow-up within 4 hours.

**Phone script (≤ 90 seconds):**

```
Hi {Name}, this is {your name} from CoreLink. I'm calling because we have
an active P0 on our side that has impacted your {customer_id} slot in the
last {N} minutes.

In short:
  • {1-sentence impact statement, no jargon}
  • Detected at {time}; engineering lead engaged at {time}
  • Mitigation underway: {1 sentence}

I'm sending a written summary right now. We'll update you again within
the next hour. Anything you need from us in the meantime — including a
direct line to our on-call engineer?
```

**Written follow-up (within 4 hours):** use the structure from `RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` §3 (customer-notify template).

### Template 5.4 — Attestation signature stalled past D+45

**To:** customer procurement / Legal counterpart (email; cc customer technical lead)
**Subject:** `CoreLink lighthouse attestation — countersigning request for {customer_id}`

```
{Name},

Per our schedule contract (D+40..D+45), we expected the attestation form
to be countersigned this week. We're currently {N} days past the soft
deadline.

The attestation captures objective metrics from the 30-day observation
window — there is nothing in §1..§4 that requires negotiation; §5.1 is
the only signature block. Form location: {attestation_md_path}.

Our hard deadline is D+50 ({date}). After that, we will need to either:
  (a) extend the observation window by another 30d, OR
  (b) re-slot to a backup candidate from our shortlist.

Both options are fine — we just need a decision. Can we get 15 minutes
on the phone this week to unblock?

— {your name}
```

### Template 5.5 — Customer indicates they may withdraw

**To:** customer technical lead (Slack DM or phone), with founder cc'd on any written follow-up.

```
{Name}, I want to make sure I'm hearing you right and we have a real
conversation rather than a process one.

You said {paraphrase customer's exact concern}. Before we talk next steps,
can I ask:

  1. Is there a specific thing CoreLink could do (in product, in support,
     in comms) that would change the answer?
  2. Is there a hard deadline driving this — internal pressure, budget
     cycle, alternative vendor pitch — that we should know about?
  3. If you do withdraw, would you be open to keeping the engagement
     informal (Slack channel stays open, you flag bugs as you see them)?

I'm asking these in this order because we'd rather fix the actual problem
than save face on a marketing milestone. If you do decide to withdraw,
that's an OK outcome and we won't push back.

Can we get 30 minutes this week — your agenda, your terms?
```

If withdrawal is confirmed: `tracker.transition(slot, LifecycleState::Withdrawn)` with the customer's written notice as `evidence_url`. Activate backup slot per `lighthouse-customer-program.md` §2.

---

## 6. 30-day-end retrospective template

Run within 5 business days of `Observing → Attested` for every lighthouse customer. 60 minutes, internal only.

**Attendees:** Customer Success engineer (facilitator), Engineer S-20 lead, SRE on-call rep, DevRel, Privacy Officer (Enterprise BYOK only).

**Pre-read:**
- The signed attestation md.
- The slot's `incident_log` (all P1+P0 events during observation).
- The 4 weekly check-in notes.
- `corelink_lighthouse_customer_slo_violation_total` + `corelink_lighthouse_customer_observation_status_gauge` for the window.

**Agenda (60 min):**

| Section | Time | Output |
|---|---|---|
| 1. Numbers walk-through | 10 min | Confirm attestation §2 actuals match what each role observed. |
| 2. What went well | 10 min | 3–5 bullets. Quote-worthy moments captured for case study. |
| 3. What broke (P1 + soft signals) | 15 min | Each incident: root cause, did our runbook handle it, did the customer notice. |
| 4. Customer signals | 10 min | Direct quotes from check-ins; sentiment trend across 4 weeks. |
| 5. Process improvements | 10 min | Concrete proposals — to runbooks, to templates, to onboarding kit. File ADRs / runbook patches inline. |
| 6. Decisions + owners | 5 min | What changes for the next slot? Who owns each? |

**Output artifact:** `specs/_audits/2026-MM-DD-lighthouse-{slot}-retro.md` with `audit_status: ACTIVE`. Cross-reference from the slot's case study draft.

**Decisions are binding** — any process improvement filed in §5 lands as a runbook PR within 14 days; if it doesn't land, the founder is on the hook to either kill the proposal or unblock the PR.

---

## 7. Cross-references

- Customer-facing playbook: `marketing/lighthouse-kit/CUSTOMER-PLAYBOOK.md`.
- Integration timeline (schedule contract): `marketing/lighthouse-kit/03-integration-timeline.md`.
- Incident response (P0): `specs/_runbooks/RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md`.
- State machine source of truth: `crates/corelink-lighthouse-tracker/src/lib.rs`.
- Program canonical: `specs/_lighthouse/lighthouse-customer-program.md`.
- Attestation form template: `specs/_lighthouse/sla-attestation-template.md`.
- Case study skeletons (marketing): `marketing/lighthouse-kit/case-study-template/`.
- Case study skeletons (spec canonical): `specs/_lighthouse/case-study-templates/`.
- Work item: `specs/04_sprints/_sealed/S20/work_items/WI-S20-004-3-lighthouse-customers-2-team-1-enterprise-byok-30d-sla-attestations.md`.
- ROADMAP entry: `ROADMAP-TO-GA.md` §5 R5-A1 + §9 H-12.

---

**Fim RB-LIGHTHOUSE-PHASE-MANAGEMENT.**
