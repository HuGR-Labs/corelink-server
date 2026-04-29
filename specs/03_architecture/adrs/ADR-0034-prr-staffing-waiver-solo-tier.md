---
id: "ADR-0034"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
title: "PRR Staffing Waiver Path for Solo-Tier Sprints (HIGH_RISK 11 sign-offs (canonical lane 10–12 framework §33.5.4.3; Lote 10.6 cycle 4 alignment))"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
deciders: ["Gustavo Schneiter (Owner)", "Architect (TBD)"]
context_links:
  - "specs/00_framework.md §33.5.4.3 (HIGH_RISK lane sign-off rules)"
  - "specs/04_sprints/S04/_spec_contract.md §6 DoD"
  - "specs/04_sprints/S04/work_items/WI-S04-006-reapi-conformance-prr-ship-gate.md"
tags: ["adr", "prr", "staffing", "waiver", "solo-tier", "governance"]
---

# ADR-0034 — PRR Staffing Waiver Path for Solo-Tier Sprints

## Context

CoreLink HIGH_RISK sprints require 11 PRR sign-offs (10 mandatory + 1 advisory Crypto SME; canonical lane 10–12 framework §33.5.4.3) per framework §33.5.4.3. CoreLink is currently solo-tier (single engineer + Architect + Crypto SME contracted advisory). Strict 11/11 sign-off requirement BLOCKS sprint SEAL when reviewers are unavailable. Without a documented waiver path, the program ships with `_TBD_` slots accumulating across sprints (persistent S-04/S-05/S-06 carry-forward defect Sonnet R5 flagged).

## Decision

Adopt **3 waiver paths** for PRR sign-off slots, with explicit boundaries and audit trail:

### Option A — Architect Compensation
- **Applies to**: SRE Lead, Compliance, Privacy, QA, Engineer (peer 1 OR 2).
- **Mechanism**: Architect (or substitute architect with equivalent expertise per the lane) signs off in addition to their own role, attesting that they reviewed the relevant evidence (chaos suite for QA, audit chain for Compliance, PII flow for Privacy, etc.).
- **Audit**: Architect signs both rows in PRR meeting notes; secondary attestation included in the row body explicitly: "Architect compensation per ADR-0034 Option A; reviewed [evidence list]; outcome [pass/conditional]".
- **Limit**: at most 3 Option A waivers per sprint to prevent rubber-stamping.

### Option B — Owner + 1 Peer (Engineer Roles Only)
- **Applies to**: Engineer (peer 1) AND Engineer (peer 2) — not both via this path.
- **Mechanism**: Owner + 1 external peer (any qualified engineer; documented by name) provide combined sign-off in lieu of 2 distinct peer reviewers.
- **Audit**: Both names recorded; specific code-review checklist completed.

### Option C — Sprint Extension via ADR
- **Applies to**: Crypto SME (NON-WAIVABLE — must be added by sprint extension if booking slips), Architect (when no compensating architect available), AppSec (mandatory emphatic per Lote 10.6bis P1-W7-2).
- **Mechanism**: open new ADR documenting the extension reason + booking calendar + new PRR target date; sprint promotion delayed; non-Crypto-SME items proceed with documented reason.
- **Crypto SME exception**: Crypto SME slot is **NON-WAIVABLE** for cripto-load-bearing WIs per Lote 10.4bis lesson (sprint contract §19 affirms). If Crypto SME unavailable, sprint extends; never ship without.

## Decision matrix

| Sign-off Slot | Option A allowed? | Option B allowed? | Option C allowed? | Non-waivable |
|---|---|---|---|---|
| 1-2 Owner + Final Approver | n/a (self) | n/a | yes | n/a |
| 3 SRE Lead | yes | n/a | yes | no |
| 4 Security Lead | yes | n/a | yes | no |
| 5-6 Engineer ×2 | n/a | yes | yes | no |
| 7 QA | yes | n/a | yes | no |
| 8 Product | n/a (self) | n/a | yes | n/a |
| 9 Compliance | yes | n/a | yes | no |
| 10 Privacy | yes (DPO interim) | n/a | yes | no |
| 11 Architect | n/a | n/a | yes | for cripto WIs |
| 12 AppSec | n/a (mandatory emphatic per Lote 10.6bis) | n/a | yes | yes (cripto WIs) |
| 13 Crypto SME | NO | NO | yes (extension) | YES (cripto WIs) |

## Consequences

### Positive
- Sprint SEAL is achievable in solo-tier reality.
- Persistent "10-of-13 TBD" defect closes structurally.
- Audit trail captures every waiver explicitly (no silent skip).

### Negative
- Architect compensation could become rubber-stamp if abused (mitigated by 3-waiver-per-sprint cap).
- Crypto SME non-waivability creates hard sprint extension risk (sprint contract §19 enforces — accepted cost of cripto baseline).

### Neutral
- Sign-off booking calendar (per WI-S06-007 §30.1; Lote 10.6bis P0-W7-1 lesson generalized) becomes mandatory across HIGH_RISK sprints.

## CI Gate

`validate_signoff_calendar.py` (forward; ST-022 in WI-S06-007) reads sprint `_signoff_calendar.yaml`; FAILS if any TBD slot is within D-3 of PRR AND no waiver_path documented; WARNS at D-7. Cross-references this ADR.

## References

- WI-S06-007 §30.1 (sign-off booking calendar template).
- specs/04_sprints/S06/_signoff_calendar.yaml (template instance).
- Sprint contract §19 (waiver policy; cripto-baseline non-waivables).

## Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.4-tris P1-R5-016 ADR-creation) | ADR file created; 3 waiver paths (Architect compensation; Owner+peer; sprint extension via ADR); Crypto SME non-waivable for cripto WIs; CI gate `validate_signoff_calendar.py` cross-ref. |
