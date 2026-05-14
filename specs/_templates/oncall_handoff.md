---
template_id: "oncall_handoff"
version: "1.0.0"
canonical_ref: "WI-S17-005 §6.1.5 + RB-ONCALL-POLICY §3"
---

# Oncall Handoff — Shift change {YYYY-MM-DD} {HH:MM} UTC

> **Canonical:** 15-min sync per shift change (knowledge transfer
> per spec contract §15 row 9). Populated end-of-shift; archived
> in the audit chain via `corelink.oncall.shift_ended`.

## Header

| Field                                  | Value                                |
| -------------------------------------- | ------------------------------------ |
| Outgoing oncall                        | `eng-{id}` ({display_name})          |
| Incoming oncall                        | `eng-{id}` ({display_name})          |
| Tier                                   | `tier-1` / `tier-2` / `tier-3`       |
| Shift end (UTC)                        | `{YYYY-MM-DDTHH:MM:SSZ}`             |
| Sync timestamp                         | `{YYYY-MM-DDTHH:MM:SSZ}`             |
| Active correlation_ids                 | `[ ... ]`                            |

## 1. Active incidents

| Incident ID | Severity | Status | correlation_id | Owner | Notes |
| ----------- | -------- | ------ | -------------- | ----- | ----- |
|             |          |        |                |       |       |

## 2. Active alerts (firing, not yet ack'd or resolved)

| Alert name | Severity | Fired (UTC) | runbook_url | Notes |
| ---------- | -------- | ----------- | ----------- | ----- |
|            |          |             |             |       |

## 3. Pending follow-ups (from prior shifts; not yet closed)

| Follow-up | Owner | Target close ts | Notes |
| --------- | ----- | --------------- | ----- |
|           |       |                 |       |

## 4. Knowledge transfer notes

- Current production state context.
- Recent deploys / config rollouts in flight.
- Known issues + workarounds.
- Anything weird / unexplained the incoming oncall should be aware of.

## 5. Sign-off

- [ ] Outgoing oncall confirms all active incidents/alerts/follow-ups documented.
- [ ] Incoming oncall confirms understanding + asked clarifying questions.
- [ ] 15-min sync executed (canonical knowledge transfer per spec contract §15 row 9).
- [ ] PagerDuty schedule shows incoming oncall in the active slot.
- [ ] Audit chain emitted `corelink.oncall.shift_ended` (outgoing) + `corelink.oncall.shift_started` (incoming).

| Signoff           | Engineer ID  | Timestamp (UTC) |
| ----------------- | ------------ | --------------- |
| Outgoing oncall   |              |                 |
| Incoming oncall   |              |                 |
| Manager (witness) |              |                 |
