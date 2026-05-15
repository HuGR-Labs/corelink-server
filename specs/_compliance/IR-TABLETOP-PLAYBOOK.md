---
id: "IR-TABLETOP-PLAYBOOK"
type: "compliance_playbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-5"
parent_wi: "WT-GAP-03"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["compliance", "ir", "tabletop", "nist-800-61", "soc2-cc7-3", "soc2-cc7-4", "soc2-cc7-5", "gap-03", "wt-gap-03"]
---

# IR Tabletop Playbook — Master Procedure (NIST 800-61 Rev.2)

> **doc_status:** DRAFT · **scope:** canonical procedure to plan, run, evidence, and improve Incident-Response (IR) tabletop exercises for CoreLink. Closes **GAP-03** of `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` ("IR plan documented but not tested end-to-end with paging + comms simulation").
>
> **Methodology anchor:** NIST SP 800-61 Rev. 2 *Computer Security Incident Handling Guide* — six-phase lifecycle (Preparation / Detection & Analysis / Containment / Eradication / Recovery / Post-Incident Activity).
>
> **Companion docs:**
> - Scenarios: `specs/_compliance/ir-scenarios/TT-01..TT-06`
> - Evidence template: `specs/_compliance/templates/IR-TABLETOP-EVIDENCE.md`
> - 2026 schedule: `specs/_compliance/IR-TABLETOP-SCHEDULE-2026.md`
> - 3-tier escalation: `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md`
> - Breach decision tree: `specs/05_quality/runbooks/RB-BREACH-NOTIF.md` + `legal/breach-notification/rb-breach-notif-decision-tree.yaml`
> - BCP/DR cadence (companion): `specs/_compliance/BCP-DR-DRILL-CADENCE.md`
> - Game-day template (60-min/4-h SRE focus): `specs/_runbooks/RB-TABLETOP-TEMPLATE.md` — IR tabletops differ in that they always include Legal + Comms + Privacy seats and exercise end-to-end *human* response, not just SRE recovery.
>
> **SOC 2 control coverage:**
> - **CC7.3** — Evaluates security events to determine whether they could result in a failure of the entity to meet its objectives. Tabletops are the operating-effectiveness evidence.
> - **CC7.4** — Responds to identified security incidents by executing a defined incident-response program. The playbook + scenarios + evidence form the documented program.
> - **CC7.5** — Identifies, develops, and implements activities to recover from identified security incidents. Recovery-phase rehearsals in each scenario.
>
> **Cross-framework coverage:** ISO/IEC 27035-1 §6 (IR plan), §7 (lessons-learned); LGPD Art. 33 (controller notification) + GDPR Art. 33/34 (DPA + data-subject notification) operating-effectiveness rehearsal; CCPA §1798.82 cure-period drill.

## 1. Methodology — NIST 800-61 Rev.2 mapping

Each tabletop exercises the full NIST 800-61 Rev.2 lifecycle. Facilitator narrates the timeline; team makes real decisions; scribe captures evidence in real time.

| NIST phase | Tabletop sub-phase | Time-share (90-min) | Time-share (120-min) | Output captured |
|---|---|---|---|---|
| **Preparation** | T-24h brief + T-0 readback | (out-of-clock) | (out-of-clock) | Brief acknowledgment per participant |
| **Detection & Analysis** | T+0..T+15 — inject delivered, IC declares severity, scribe opens evidence form | 15 min | 20 min | Severity declared; channel opened; first PD page (simulated) |
| **Containment** | T+15..T+45 — short-term containment decisions (block traffic, revoke key, disable feature flag, isolate tenant) | 30 min | 40 min | Decisions list w/ owner + rationale + reversibility |
| **Eradication** | T+45..T+65 — root-cause hypothesis + remove artefacts (CMK rotation, dep yank, PAT revoke, etc.) | 20 min | 25 min | Eradication actions list + verification step |
| **Recovery** | T+65..T+80 — service-restore plan, monitoring uplift, customer-comms trigger | 15 min | 20 min | Recovery acceptance criteria + monitoring window |
| **Post-Incident Activity** | T+80..T+90 — retro queue, action items, lessons learned, sign-off | 10 min | 15 min | Action items in tracker (Linear epic `IR-TT-<id>`) |

> **Note:** Preparation is exercised pre-session (T-2 weeks .. T-0 lead-up checklist §4) — the in-session clock starts at Detection.

## 2. Cadence

- **Quarterly** sessions, 2 scenarios per quarter (alternating SEV0/SEV1 mix and security/operational mix).
- Each session **90 min minimum** (single scenario) or **120 min** when a scenario explicitly carries Legal Liaison + Customer Comms Lead seats (TT-01, TT-04 default to 120 min; others may stretch if injects surface a notification decision).
- Steady-state post-GA: cadence calendarised in `corelink-runbook-tracker` (WI-S17-003) with PD reminder T-2 weeks. Pre-GA window: see `specs/_compliance/IR-TABLETOP-SCHEDULE-2026.md`.
- Quorum: IC + Scribe + Comms Lead + Tech Lead (4 humans). If a scenario is privacy/security tinted, Legal Liaison or Privacy Officer also required (5 humans). Below quorum → reschedule.

## 3. Roles & responsibilities

| Role | Default participant | Authority during exercise | Real-incident counterpart |
|---|---|---|---|
| **Incident Commander (IC)** | Rotating L2 EM (per `ONCALL-ESCALATION-MATRIX.md` §1) | Owns decisions, severity, escalation tier, comms sign-off | L2 EM on real SEV1; L3 CTO on real SEV0 |
| **Scribe** | Engineer not on rotation that week | Real-time evidence capture into `IR-TABLETOP-EVIDENCE.md` instance; timestamps every decision | Same — scribe role formalised in `RB-POSTMORTEM-PROCESS.md` |
| **Comms Lead** | Product / Docs lead | Drafts status-page copy, customer email, internal Slack post; reviews timing against SLA matrix | Comms officer per `RB-LAUNCH-WAR-ROOM-COORDINATION.md` |
| **Tech Lead** | Senior SRE or Architect | Surfaces RB-* references, FM IDs, CTRL IDs, audit-event types; drives Containment + Eradication phases | Same in real incident |
| **Legal Liaison** | Outside counsel on retainer (pre-GA: Gustavo + counsel-on-call) | Owns regulator-notification decision (LGPD Art. 33 / GDPR Art. 33 / CCPA §1798.82); ToS-invocation calls | Legal-on-call per `ONCALL-ESCALATION-MATRIX.md` §1 L3 |
| **Customer Comms Lead** | Customer Success Manager | Owns enterprise-customer outreach copy + cadence; coordinates with Comms Lead | CS Manager on-call per `RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md` |
| **Privacy Officer** (conditional) | Privacy Officer (Gustavo interim pre-GA) | Required for any scenario touching PII / DSR / consent | Per `RB-DSR-ERASURE-INCOMPLETE.md` + `RB-CONSENT-TAMPERING.md` |
| **Security Lead** (conditional) | AppSec | Required for breach / supply-chain / insider scenarios | Per `RB-SECURITY-VULNERABILITY-INTAKE.md` + `RB-PENTEST-FINDING-RESPONSE.md` |
| **Observer** (optional) | Customer success advisor or external auditor | Silent; takes side-notes; offers feedback in retro | Auditor walkthrough per `AUDITOR-WALKTHROUGH-SCRIPT.md` |

**Conflict rule:** if IC and Tech Lead disagree, IC decides and Tech Lead's dissent is captured in the evidence form §"Decisions" column "minority opinion". Same protocol as real incidents (`ONCALL-ESCALATION-MATRIX.md` §6 tie-breaker).

## 4. Lead-up checklist (per session)

> **Discipline:** every item is a checkbox in the evidence form §1 "Pre-tabletop readiness". Missing items at T-0 → facilitator may abort and reschedule.

### T-2 weeks
- [ ] **Scenario chosen** from `specs/_compliance/ir-scenarios/TT-*` per quarterly plan (or ad-hoc by SRE Lead).
- [ ] **Facilitator assigned** (rotates: SRE Lead → Security Lead → Privacy Officer → CTO).
- [ ] **Quorum confirmed** in calendar with required roles per §3; observer optional.
- [ ] **PD calendar reminder created** (`pd-reminder-tt-<id>`) with 2-week + 1-week + 24h pings.
- [ ] **Linear epic** opened: `IR-TT-<YYYY-Qx>-<NN>` for retro action items.

### T-1 week
- [ ] **Injects prepared** by facilitator: each scenario lists 3-5 narrative injects with preset timestamps; facilitator localises them (replace placeholder customer IDs, region, dep names with current realistic values from staging).
- [ ] **Evidence form instantiated** as `specs/_audits/2026-MM-DD-ir-tabletop-TT-<id>.md` from template `specs/_compliance/templates/IR-TABLETOP-EVIDENCE.md`. Frontmatter `doc_status: DRAFT`.
- [ ] **Realistic anchors verified**: open FM catalog + ONCALL matrix + relevant RB-* in browser tabs the facilitator will switch to during injects.
- [ ] **Dry-run injects with co-facilitator** if first time running this scenario.

### T-24h
- [ ] **Participants briefed** via async Slack post: scenario codename + assumed severity + roles + Zoom link + reminder that the exercise is a tabletop (no live actions, simulated paging only).
- [ ] **PD test-mode page sent** to confirm rotation alerts route correctly (synthetic page per `specs/_runbooks/RB-SYNTHETIC-PAGE-DRILL.md`).
- [ ] **Status page maintenance window** drafted but **not** published (simulation only).

### T-0
- [ ] **Roll-call** captured in evidence form §2; missing roles → IC decides go/no-go (must keep quorum).
- [ ] **Inject 1 read aloud** by facilitator; clock starts.

## 5. Execution rules

1. **Facilitator narrates only.** Does not solve the problem; injects facts, blocks rabbit-holes by saying "out of scope for this session, scribe note it".
2. **Decisions are real.** IC makes binding (simulated) decisions; "I would page L3" counts as decision evidence; actual paging is replaced by scribe writing `[SIMULATED PAGE: L3 CTO @ T+13]`.
3. **Customer-comms drafted in full.** Comms Lead writes the actual status-page paragraph + customer email body in shared doc; scribe attaches to evidence form §6.
4. **External notifications drafted but not sent.** Legal Liaison drafts LGPD ANPD notice / GDPR DPA notice; scribe attaches; nothing is filed.
5. **Reversibility flagged.** Each containment decision tagged `reversible | irreversible` in evidence form §4. Irreversible decisions (e.g., BYOK CMK revoke, sub-processor key rotation) get a 2nd-person sanity check before "executing".
6. **Audit-event types named.** Tech Lead must call out exact audit-event type for every action ("`pat.revoke.bulk` audit event would fire here") — this is a CC7.4 evidence signal.
7. **Time-checks every 15 min.** Scribe says "T+15", "T+30", etc., aloud so the team paces.
8. **No real production touches.** Confirmed by facilitator at T-0 readback + reinforced at any moment a participant ventures near a live system.

## 6. Post-tabletop (within 24h)

1. **Retro meeting** (30 min) within 24h; same participants if possible. Format:
   - **What went well?** (3 bullets max)
   - **What was unclear?** (gaps in RB-*, FM catalog, escalation matrix, comms templates)
   - **Action items** (SMART: owner + due date + tracker link)
2. **Evidence form sealed** (`doc_status: FROZEN`) within 24h of retro completion. IC + facilitator co-sign §9 sign-off block.
3. **Action items linked** in Linear epic `IR-TT-<YYYY-Qx>-<NN>`; each item carries label `gap-03-followup` + `tt-<id>`.
4. **Runbook updates** triggered: if a gap in any RB-* surfaced, file PR within 7 days (per `RB-FM-202-runbook-stale.md` meta-drill cadence).
5. **Roll-up to Drata.** Evidence form lives under `specs/_audits/`; the `incident_response` EvidenceStream (per `SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §4.1) picks it up daily at 03:00 UTC and ships to Drata via `/v1/evidence/incident-response`.

## 7. Lessons-learned discipline

After 2 quarters of executed tabletops, the SRE Lead runs a **meta-retro** comparing themes across all sessions:
- Which RB-* gaps repeat?
- Which decision points consistently exceed SLA?
- Which roles consistently struggle (suggests training need)?

Meta-retro output is a quarterly `specs/_audits/2026-MM-DD-ir-tabletop-meta-retro-<NN>.md` document — also SOC 2 CC7.4 evidence (continuous-improvement signal).

## 8. Failure to execute

If a scheduled tabletop is **skipped** (no quorum, blocked by real incident, etc.):
- Reschedule within 14 days.
- File `specs/_audits/2026-MM-DD-ir-tabletop-SKIP-<id>.md` with reason + new date.
- Three skipped sessions in a row → SOC 2 audit risk; auto-escalate to L3 (CTO + Compliance Officer).

## 9. Scenario catalog (this release)

| ID | Title | Severity | Duration | Phase mix | Quarter |
|---|---|---|---|---|---|
| **[TT-01](./ir-scenarios/TT-01-data-breach.md)** | SEV0 data breach — suspicious access patterns, possible BYOK envelope leak | SEV0 | 120 min | Heavy Detection + Legal | Q3-2026 |
| **[TT-02](./ir-scenarios/TT-02-cascading-failure.md)** | SEV1 cascading failure — D1 region-A read-only, replica lag, stale audit heads | SEV1 | 90 min | Heavy Containment + Recovery | Q3-2026 |
| **[TT-03](./ir-scenarios/TT-03-webhook-compromise.md)** | Stripe webhook compromise after key rotation | SEV1 | 90 min | Detection + Eradication | Q4-2026 |
| **[TT-04](./ir-scenarios/TT-04-insider-threat.md)** | Insider threat — bulk PAT issuance by privileged employee | SEV0 | 120 min | Heavy Legal + HR | Q4-2026 |
| **[TT-05](./ir-scenarios/TT-05-supply-chain.md)** | Supply-chain compromise — cargo-audit hit on transitive dep with active exploit | SEV1 | 90 min | Eradication + Customer Comms | Q1-2027 |
| **[TT-06](./ir-scenarios/TT-06-ddos-abuse.md)** | DDoS / abuse storm — 10k IPs, 5 min, legitimate-looking traffic | SEV1 | 90 min | Containment + Edge Filtering | Q1-2027 |

## 10. Cross-references

- `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §3.4 GAP-03 row.
- `ROADMAP-TO-GA.md` §9 (GAP-03 progress marker).
- `specs/_compliance/SOC2-GAP-ANALYSIS.md` GAP-03 register row.
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` §3 SEV1/2/3 SLA matrix.
- `specs/05_quality/runbooks/INDEX.md` §2 FM-anchored runbooks (referenced per scenario).
- `specs/03_architecture/failure_modes.md` (FM catalog, 75 entries).

## 11. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo Schneiter | Initial playbook (GAP-03 closure). NIST 800-61 Rev.2 mapping; 6 scenarios catalogued; quarterly cadence with 2026 schedule companion. |
