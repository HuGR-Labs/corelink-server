---
id: "TEMPLATE-IR-TABLETOP-EVIDENCE"
type: "template"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["template", "ir", "tabletop", "evidence", "soc2-cc7-3", "soc2-cc7-4", "soc2-cc7-5", "nist-800-61", "gap-03", "wt-gap-03"]
---

# IR Tabletop Evidence — Auditor-ready form (per-session)

> **Template.** Copy to `specs/_audits/2026-MM-DD-ir-tabletop-TT-<id>.md` for each tabletop run. **MUST** be sealed (`doc_status: FROZEN`) within 24h of retro completion. Auditor-facing under SOC 2 CC7.3 (event evaluation) + CC7.4 (incident response) + CC7.5 (recovery activities) + ISO/IEC 27035-1 §6–7.
>
> **Parent:** `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` (master procedure, NIST 800-61 Rev.2).
>
> **Frontmatter for runs:** when this template is instantiated, the resulting evidence file MUST set `type: "audit"`, keep all required fields, and add `parent_tabletop: "TT-<id>"` + `tabletop_cycle: "Q<n>-YYYY"` + `sprint: "<current-sprint-or-steady-state>"`.

---

## 0. Header (fill all)

| Field | Value |
|---|---|
| Tabletop run ID | `2026-MM-DD-ir-tabletop-TT-<id>` |
| Scenario | `TT-<id> — <title>` |
| Scenario doc | `specs/_compliance/ir-scenarios/TT-<id>-<slug>.md` |
| Date / start time (UTC) | `YYYY-MM-DD HH:MM` |
| Duration (planned / actual) | `90 min / NN min` |
| Severity simulated | SEV0 / SEV1 |
| Facilitator | `<name>` |
| Quarter | `Q<n>-YYYY` |
| Parent playbook version | `1.0.0` |

## 1. Pre-tabletop readiness (lead-up checklist)

> Mirrors `IR-TABLETOP-PLAYBOOK.md` §4. Each `[ ]` becomes `[x]` once completed; unchecked items at T-0 → IC notes go/no-go decision in §3.

### T-2 weeks
- [ ] Scenario chosen
- [ ] Facilitator assigned
- [ ] Quorum confirmed
- [ ] PD calendar reminder created
- [ ] Linear epic `IR-TT-<YYYY-Qx>-<NN>` opened

### T-1 week
- [ ] Injects localised (3-5 narrative injects with current realistic anchors)
- [ ] Evidence form instantiated (this doc)
- [ ] FM / RB-* / ONCALL anchors verified
- [ ] Dry-run injects with co-facilitator (first-time scenarios only)

### T-24h
- [ ] Participants briefed (Slack async post)
- [ ] PD test-mode page sent (synthetic page drill)
- [ ] Status-page maintenance window drafted (not published)

### T-0
- [ ] Roll-call captured (§2)
- [ ] Inject 1 read aloud; clock started

## 2. Participants & roles

| Role | Participant | Present | Notes |
|---|---|---|---|
| Incident Commander (IC) | `<name>` | yes / no | |
| Scribe | `<name>` | yes / no | (author of this doc) |
| Comms Lead | `<name>` | yes / no | |
| Tech Lead | `<name>` | yes / no | |
| Legal Liaison | `<name or N/A>` | yes / no / n/a | (required if scenario tags `legal-required`) |
| Customer Comms Lead | `<name or N/A>` | yes / no / n/a | (required if customers in blast radius) |
| Privacy Officer | `<name or N/A>` | yes / no / n/a | (required if PII / DSR / consent in scope) |
| Security Lead | `<name or N/A>` | yes / no / n/a | (required for breach / supply-chain / insider scenarios) |
| Observer(s) | `<names>` | — | silent; may include external auditor |

**Quorum met:** yes / no
**If no:** facilitator decision (proceeded / aborted): `<rationale>`

## 3. Timeline (real-time scribe capture)

> Capture each event with absolute UTC time AND relative T+ time. Don't editorialise — paste injects verbatim, transcribe decisions verbatim.

| UTC time | T+ | Phase (NIST) | Event | Owner |
|---|---|---|---|---|
| `HH:MM:SS` | `T+00:00` | Detection | Inject 1 delivered: `<inject text>` | Facilitator |
| `HH:MM:SS` | `T+NN:NN` | Detection | IC declares severity `SEV<n>` | IC |
| `HH:MM:SS` | `T+NN:NN` | Detection | Channel opened (simulated): `#inc-<id>` | IC |
| `HH:MM:SS` | `T+NN:NN` | Detection | [SIMULATED PAGE: L<n> @ T+NN] | IC |
| `HH:MM:SS` | `T+NN:NN` | Containment | Decision: `<containment action>` (reversible/irreversible) | Tech Lead |
| `HH:MM:SS` | `T+NN:NN` | — | Inject 2 delivered: `<inject text>` | Facilitator |
| ... | ... | ... | ... | ... |

## 4. Decisions

| # | Decision | Owner | Rationale | Reversibility | Audit-event type | Minority opinion (if any) |
|---|---|---|---|---|---|---|
| D-1 | `<decision>` | IC / Tech Lead / etc. | `<why>` | reversible / irreversible | `pat.revoke.bulk` (example) | `<name + dissent>` or `none` |
| D-2 | ... | ... | ... | ... | ... | ... |

## 5. Containment / Eradication / Recovery actions

### 5.1 Containment (T+15..T+45)
- Action: `<short-term containment>` — owner `<name>` — verified by `<name>` — audit-event `<type>`.

### 5.2 Eradication (T+45..T+65)
- Root-cause hypothesis: `<text>`
- Eradication action: `<remove artefact / rotate / revoke / yank dep>` — owner `<name>` — verification: `<how confirmed>`.

### 5.3 Recovery (T+65..T+80)
- Service-restore plan: `<text>`
- Monitoring uplift: `<dashboards / alerts watched>`
- Customer-comms trigger: `<who notifies whom when>`
- Recovery acceptance criteria: `<metric thresholds + observation window>`

## 6. Comms artefacts (drafted in-session)

### 6.1 Internal Slack post (T+5)
```
<paste full text drafted during exercise>
```

### 6.2 Status-page update (initial @ T+15)
```
<paste full text>
```

### 6.3 Status-page update (resolution @ T+NN)
```
<paste full text>
```

### 6.4 Customer email (enterprise tier; if customers affected)
```
Subject: <line>
Body: <paste full text>
```

### 6.5 Regulator notification draft (if `legal-required`)
- LGPD ANPD (Art. 33): `<full draft or N/A>`
- GDPR DPA (Art. 33): `<full draft or N/A>`
- GDPR data-subject (Art. 34): `<full draft or N/A>`
- CCPA notification (§1798.82): `<full draft or N/A>`

> All regulator drafts MUST cross-reference `specs/05_quality/runbooks/RB-BREACH-NOTIF.md` decision tree.

## 7. Lessons learned (retro, within 24h)

### 7.1 What went well (3 bullets max)
- ...
- ...
- ...

### 7.2 What was unclear / gaps
| Gap | Affected doc(s) | Severity | Owner |
|---|---|---|---|
| `<gap>` | `<RB-* / FM / CTRL>` | blocker / major / minor | `<name>` |

### 7.3 Action items (SMART)

| # | Action | Owner | Due | Tracker link | Label |
|---|---|---|---|---|---|
| AI-1 | `<specific action>` | `<name>` | `YYYY-MM-DD` | `IR-TT-<id>-1` | `gap-03-followup` |
| AI-2 | ... | ... | ... | ... | ... |

## 8. Cross-references

- **NIST phases covered:** Detection [yes/no] · Analysis [yes/no] · Containment [yes/no] · Eradication [yes/no] · Recovery [yes/no] · Post-Incident [yes/no]
- **FM IDs exercised:** `FM-XXX, FM-YYY` (per scenario doc)
- **RB-* invoked (simulated):** `RB-XXX, RB-YYY`
- **CTRL IDs touched:** `CTRL-XXX-NNN, CTRL-YYY-NNN`
- **SOC 2 controls evidenced:** CC7.3 · CC7.4 · CC7.5 (always); plus scenario-specific (see scenario doc §"controls").
- **External frameworks:** ISO/IEC 27035-1 §6, §7 · LGPD Art. 33 · GDPR Art. 33, 34 · CCPA §1798.82 (when applicable).

## 9. Sign-off

| Role | Name | Signature (initials) | Date |
|---|---|---|---|
| Incident Commander | `<name>` | `<initials>` | `YYYY-MM-DD` |
| Facilitator | `<name>` | `<initials>` | `YYYY-MM-DD` |
| Scribe (author) | `<name>` | `<initials>` | `YYYY-MM-DD` |
| Compliance review (post-seal) | `<Compliance Officer>` | `<initials>` | `YYYY-MM-DD` |

**Status at seal:** `doc_status: FROZEN` · `audit_status: ACTIVE` · uploaded to Drata via `incident_response` EvidenceStream daily run.

---

## Appendix A — Auditor cross-walk

| Question an auditor may ask | Answer location in this form |
|---|---|
| Was the tabletop scheduled in advance? | §1 (T-2 weeks checkboxes) |
| Who participated? | §2 |
| Was severity declared and within SLA? | §3 (Detection-phase rows) |
| Were containment decisions documented and reversible? | §4 + §5.1 |
| Was customer / regulator notification drafted? | §6 |
| Were action items captured and tracked? | §7.3 |
| Were lessons fed back into the program? | §7.2 + parent playbook §7 meta-retro |
| Where is this evidence in Drata? | Daily `incident_response` EvidenceStream sync; URL `/v1/evidence/incident-response` |
