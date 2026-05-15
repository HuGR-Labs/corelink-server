---
id: "IR-TABLETOP-SCHEDULE-2026"
type: "compliance_schedule"
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
tags: ["compliance", "ir", "tabletop", "schedule", "2026", "soc2-cc7-3", "soc2-cc7-4", "soc2-cc7-5", "gap-03", "wt-gap-03"]
---

# IR Tabletop Schedule — 2026 / Q1-2027

> **doc_status:** DRAFT · **scope:** annual schedule for IR tabletop sessions covering the 6 canonical scenarios catalogued in `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` §9. Closes the **execution-cadence** portion of **GAP-03**.
>
> **Parent:** `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` (master procedure).
>
> **SOC 2:** CC7.3 + CC7.4 + CC7.5 operating-effectiveness evidence (Type II observation window).

## 1. Annual calendar

| Quarter | Window (UTC) | Scenario A | Scenario B | Facilitator (A / B) |
|---|---|---|---|---|
| **Q3-2026** | Jul 1 – Sep 30 | **TT-01** SEV0 data breach | **TT-02** SEV1 cascading failure | SRE Lead / Security Lead |
| **Q4-2026** | Oct 1 – Dec 31 | **TT-03** Stripe webhook compromise | **TT-04** Insider threat (PAT bulk issuance) | Security Lead / Privacy Officer |
| **Q1-2027** | Jan 1 – Mar 31 | **TT-05** Supply-chain compromise | **TT-06** DDoS / abuse storm | Security Lead / SRE Lead |

> **Pairing rationale:** each quarter pairs one heavy-Legal-Comms scenario (TT-01, TT-04, TT-05) with one heavy-SRE-recovery scenario (TT-02, TT-03, TT-06). This balances the rotation load and rehearses both axes of the IR program per quarter.

## 2. Default session slots (target dates)

| ID | Target date | Time (UTC) | Duration | Backup date |
|---|---|---|---|---|
| TT-01 | 2026-07-22 (Wed) | 14:00 | 120 min | 2026-08-05 (Wed) |
| TT-02 | 2026-09-09 (Wed) | 14:00 | 90 min | 2026-09-23 (Wed) |
| TT-03 | 2026-10-21 (Wed) | 14:00 | 90 min | 2026-11-04 (Wed) |
| TT-04 | 2026-12-02 (Wed) | 14:00 | 120 min | 2026-12-16 (Wed) |
| TT-05 | 2027-02-04 (Wed) | 14:00 | 90 min | 2027-02-18 (Wed) |
| TT-06 | 2027-03-18 (Wed) | 14:00 | 90 min | 2027-04-01 (Wed) |

**Why Wednesday 14:00 UTC?** Same rationale as `BCP-DR-DRILL-CADENCE.md` §1 — maximises US-East + EU-West rotation overlap; avoids Mon (deploy day), Fri (recovery exhaustion), and weekend (off-rotation).

**Why 6+ weeks between sessions?** Allows action items from session N to land before session N+1, so the meta-retro signal (`IR-TABLETOP-PLAYBOOK.md` §7) actually reflects improvement velocity.

## 3. Lead-up checklist per session

> Canonical detailed checklist is in `IR-TABLETOP-PLAYBOOK.md` §4. Summary timeline below.

| Lead time | Owner | Activity | Artefact |
|---|---|---|---|
| **T-2 weeks** | SRE Lead | Confirm scenario choice; assign facilitator; check quorum availability; open Linear epic `IR-TT-<YYYY-Qx>-<NN>`; create PD reminder | Calendar invite + Linear epic |
| **T-1 week** | Facilitator | Localise injects (current realistic anchors); instantiate evidence form `specs/_audits/2026-MM-DD-ir-tabletop-TT-<id>.md`; verify FM / RB-* / ONCALL tabs; dry-run injects with co-facilitator (first-time scenarios) | Localised inject script + DRAFT evidence form |
| **T-24h** | Facilitator + IC | Brief participants via Slack async; send PD test-mode page (synthetic page drill); draft (but do not publish) status-page maintenance window | Slack post + PD ack + draft SP window |
| **T-0** | IC | Roll-call; quorum confirm; clock starts when Inject 1 read | Filled §2 + §3 of evidence form |
| **T+0..90 (or 120)** | All | Execute per playbook §5 rules | Live evidence capture |
| **T+24h** | IC + Facilitator | Retro (30 min); action items into Linear; seal evidence form (`doc_status: FROZEN`) | Sealed evidence form + tracked action items |
| **T+7d** | Action-item owners | Begin closing items (especially RB-* PRs flagged in §7.2 retro) | PRs / Linear updates |
| **T+30d** | SRE Lead | Action-item closure review; carry-over items rolled to next session if not closed | Review note in Linear epic |

## 4. Roster (rotating)

Pre-GA staffing (small team; many roles concentrate on Gustavo + future hires):

| Role | 2026-Q3..Q4 | 2027-Q1+ |
|---|---|---|
| IC pool | EM-on-call rotation (pre-GA: Gustavo as L2 backstop) | EM rotation + hired senior eng |
| Facilitator pool | SRE Lead → Security Lead → Privacy Officer (rotate by session) | Same rotation; CTO may facilitate quarterly meta-retro |
| Scribe pool | Any engineer not on rotation that week | Same |
| Comms Lead | Product / Docs lead | Same (or dedicated PMM hire if available) |
| Legal Liaison | Outside counsel (Gustavo + counsel-on-call) | Outside counsel + in-house if hired |
| Customer Comms | CS Manager (pre-GA: Gustavo) | CS Manager |

**Conflict-of-interest rule:** if a real incident is active when a tabletop is scheduled, the tabletop is **immediately rescheduled** within 14 days (per playbook §8 — failure-to-execute protocol). Do **not** combine real-incident response with a simulated exercise on the same day.

## 5. Evidence flow (post-session)

1. Sealed evidence forms land in `specs/_audits/` per playbook §6.
2. `incident_response` EvidenceStream picks them up daily at 03:00 UTC (per `SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §4.1) and ships to Drata via `/v1/evidence/incident-response`.
3. After each quarter's two sessions, SRE Lead files `specs/_audits/2026-MM-DD-ir-tabletop-Q<n>-summary.md` (quarterly roll-up): coverage of NIST phases, action-item closure %, themes.
4. After Q1-2027 (all 6 scenarios run once), SRE Lead files a **meta-retro** (playbook §7) — this is the artefact a SOC 2 Type II auditor will ask for to evidence continuous improvement (CC7.4).

## 6. SOC 2 Type II observation-window alignment

Assuming Type II observation window opens **T+3m post-Type-I letter** (~2026-08-15 per `ROADMAP-TO-GA.md` §7), the 6-scenario rotation completes within the first 12 months of Type II observation:

| Type II month | Tabletop within month | Note |
|---|---|---|
| M1 (Aug 2026) | — | Setup; Drata pulling evidence |
| M2 (Sep 2026) | TT-02 (Sep 9) | First Type II tabletop |
| M3 (Oct 2026) | TT-03 (Oct 21) | |
| M5 (Dec 2026) | TT-04 (Dec 2) | |
| M6 (Jan 2027) | — | Quarter break; Q4 summary filed |
| M7 (Feb 2027) | TT-05 (Feb 4) | |
| M9 (Mar 2027) | TT-06 (Mar 18) | |
| M9 (Mar 2027) | Q1 summary + 6-scenario meta-retro | Continuous-improvement artefact |

TT-01 was executed pre-Type-II observation (Jul 22, 2026) — counted as Type I letter preparation evidence and as **bridge evidence** for the gap between Type I sealing and Type II observation start.

## 7. Cross-references

- `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` — master procedure.
- `specs/_compliance/templates/IR-TABLETOP-EVIDENCE.md` — per-session evidence form.
- `specs/_compliance/ir-scenarios/TT-01..TT-06` — scenario library.
- `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §3.4 GAP-03 row, §4.1 `incident_response` stream.
- `specs/_compliance/BCP-DR-DRILL-CADENCE.md` — companion BCP/DR cadence (operational vs IR focus).
- `ROADMAP-TO-GA.md` §9 (GAP-03 progress marker).

## 8. Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo Schneiter | Initial schedule (GAP-03 closure). 6 scenarios paired across Q3-2026, Q4-2026, Q1-2027 with Wednesday 14:00 UTC default slots and backup dates. |
