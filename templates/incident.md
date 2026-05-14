---
id: "INC-YYYY-MM-DD-NNN"
type: "incident"
severity: "SEV-2"                                # enum: SEV-1 | SEV-2 | SEV-3
status: "open"                                   # enum: open | mitigated | resolved | closed
started_at: "YYYY-MM-DDTHH:MM:SSZ"
closed_at: null                                  # null while open; ISO-8601 UTC on close
services_affected: []                            # e.g. ["api", "audit", "dsr"]
owner: "TEMPLATE_INCIDENT_COMMANDER"
summary: "<one-line incident summary; no blame language>"
correlation_id: "TEMPLATE_CORRELATION_ID"        # PAT-CORRELATION-ID-001 propagation
postmortem_link: null                            # PM-YYYY-MM-DD-NNN once produced
---

# INC-YYYY-MM-DD-NNN — {{title}}

> **Template Version:** 1.0.0
> **doc_status:** DRAFT (initial) → REVIEW → SEALED
> **Parent Spec:** WI-S17-004 (incident + postmortem templates blameless)
>
> *Blameless culture note: this template captures **events**, not personal
> attributions. The narrative voice is third-person systemic
> ("the service did X", "the alert fired Y"). Never write "Alice failed
> to do X" — write "the deploy step lacked X validation".*

---

## 1. Header

| Field | Value |
|---|---|
| Incident ID | INC-YYYY-MM-DD-NNN |
| Severity | SEV-{1\|2\|3} |
| Started at (UTC) | YYYY-MM-DDTHH:MM:SSZ |
| Closed at (UTC) | YYYY-MM-DDTHH:MM:SSZ \| _open_ |
| Services affected | [list] |
| Owner / Incident Commander | {{name + role}} |
| Correlation ID | {{uuid; PAT-CORRELATION-ID-001}} |
| Customer impact estimate | {{e.g. "≤ N tenants saw degraded reads for D minutes"}} |

---

## 2. Timeline

All timestamps **UTC, ISO-8601**. Each row carries the same `correlation_id`
so audit / observability stacks can join evidence end-to-end.

| ts (UTC) | actor | event | source / link |
|---|---|---|---|
| YYYY-MM-DDTHH:MM:SSZ | system / oncall / customer | trigger / alert / page / detection | dashboard / log / runbook |
| YYYY-MM-DDTHH:MM:SSZ | oncall | acknowledged | PagerDuty event id |
| YYYY-MM-DDTHH:MM:SSZ | oncall | mitigation step N applied | runbook RB-XXX §N.N |
| YYYY-MM-DDTHH:MM:SSZ | oncall | verified mitigation | metric / probe link |
| YYYY-MM-DDTHH:MM:SSZ | IC | resolved | — |

---

## 3. Customer Impact

- **Tenants affected**: count + IDs (or "internal only").
- **User-visible symptoms**: latency / errors / data loss / feature unavailable.
- **Duration of impact**: minutes from first error to verified mitigation.
- **Workarounds advised**: yes / no + link to status page entry.
- **SLA / SLO breach?** which SLO, by how much.

> Privacy: do **NOT** copy customer PII into this document. Reference
> tenant IDs and aggregate counts only (CTRL-PRIV-001).

---

## 4. Root Cause

State the **technical** root cause in system / process terms. No "who".

- Trigger event (the change / load / failure that lit the fuse).
- Failure mode in the system (the latent issue that the trigger surfaced).
- Why detection / safeguards did not stop it earlier.

This section is the input to the postmortem's deeper 5-Why analysis.

---

## 5. Detection

- How was the incident discovered? (alert name / customer report / synthetic).
- Time-to-detect (TTD) = first_signal_ts − started_at.
- Was detection automatic or human?
- Gaps: should we have detected this earlier? (action item → postmortem).

---

## 6. Response

- Incident commander + scribe.
- Runbooks executed: RB-XXX, RB-YYY (with sections).
- Communications: status page, internal Slack, customer comms.
- Time-to-acknowledge (MTTA) = ack_ts − first_signal_ts.
- Bridge / war-room link (if any).

---

## 7. Resolution

- Mitigation applied (rollback / flag flip / capacity add / failover / patch).
- Verification: which probe / metric / customer signal confirmed recovery.
- Time-to-mitigate (MTTM) = mitigated_ts − ack_ts.
- Time-to-resolve (MTTR) = resolved_ts − started_at.
- Residual risk (anything still degraded after closure).

**Targets** (per spec contract §9.8):

| Severity | MTTA target | MTTR target |
|---|---|---|
| SEV-1 | < 300 s | < 1800 s |
| SEV-2 | < 900 s | < 7200 s |
| SEV-3 | < 3600 s | < 86400 s |

---

## 8. Action Items

Track here; copy authoritative version into the postmortem. Each item must
have an owner team + due date + status. Sprint owner accepts / rejects
per R-S17-11.

| ID | Item | Owner team | Due | Status |
|---|---|---|---|---|
| AI-1 | … | … | YYYY-MM-DD | open |

---

## 9. References

- Postmortem doc: PM-YYYY-MM-DD-NNN (link once drafted within 5 days).
- Runbooks executed: RB-XXX.
- Dashboards / alerts: …
- Related incidents (recurrence?): INC-…

---

**End INC-YYYY-MM-DD-NNN.**
