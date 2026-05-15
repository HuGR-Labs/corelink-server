---
id: "RB-LAUNCH-WAR-ROOM-COORDINATION"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "WR-COORD (Marketing Lead by default)"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "marketing/launch/LAUNCH-CHECKLIST-V2.md"
tags:
  - "runbook"
  - "launch"
  - "war-room"
  - "coordination"
  - "r8"
  - "wt-r8-1"
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §1.

# RB-LAUNCH-WAR-ROOM-COORDINATION — Operator playbook for the GA launch war room

> **Lightweight** runbook for the war room coordinator (WR-COORD). The canonical content lives in `marketing/launch/LAUNCH-CHECKLIST-V2.md`. This runbook captures the **operator-level mechanics** of running the war room — what to set up, what to log, when to escalate, when to dissolve.

---

## 1. When to invoke

- Activate at **T-6h** of GA launch (per `LAUNCH-CHECKLIST-V2.md` row L12).
- Stand down at **T+72h** (per `LAUNCH-CHECKLIST-V2.md` row L46), unless converted to incident war room.

## 2. Pre-requisites (T-7d to T-24h)

- [ ] Book a dedicated Slack channel name: `#launch-war-room-<YYYYMMDD>`. Do not pre-create > 24h ahead (avoid leakage).
- [ ] Reserve a Zoom bridge with 24h continuous capability. Pin the URL in the war room channel.
- [ ] If Bay Area FTE: book a physical war room with a 4K monitor and reliable wifi.
- [ ] Pre-stage Signal fallback group with all key roles (`LAUNCH-CHECKLIST-V2.md` §1).
- [ ] Pre-stage `corelink.dev/status` static fallback page (per `STATUS-PAGE-SPEC.md` §2).

## 3. Setup at T-6h (15-min checklist)

1. Open `#launch-war-room-<date>` and invite: CEO, CTO, VPMkt, VPSec, SRE-OC primary + secondary, CS-OC, PR, WR-COORD.
2. Post pinned message with:
   - Zoom bridge URL.
   - Link to `LAUNCH-CHECKLIST-V2.md`.
   - Link to live Day-1 dashboard (per `DAY-1-DASHBOARD-SPEC.md`).
   - Link to `STATUS-PAGE-SPEC.md` + `CRISIS-COMMS-TEMPLATES.md`.
   - Link to `ONCALL-ESCALATION-MATRIX.md`.
3. Start the **war room log** as a single thread (canonical decisions only; not chatter).
4. Confirm every key role checks in within 30 min of channel open.
5. Sound-check Zoom bridge.

## 4. During-launch operating rhythm

- **Every 15 min, T-1h to T+1h:** WR-COORD posts a 1-line posture summary to the war room log.
- **Every 30 min, T+1h to T+6h:** same.
- **Every 60 min, T+6h to T+24h:** same.
- **Every 6h, T+24h to T+72h:** same.
- **At each scheduled war room sync** (rows L32, L37, L38, L40, L44): WR-COORD captures decisions in the log.

### What goes in the log (canonical decisions only)

- All GO/DEFER decisions.
- All status page transitions.
- All crisis-comms invocations and approvals.
- All press inbound that requires a CEO response.
- All SEV1/SEV2 declarations + resolutions.
- All fallbacks invoked from `LAUNCH-CHECKLIST-V2.md`.

### What does NOT go in the log

- Chat. Speculation. Real-time discussion. (Those happen in the Slack channel.)

## 5. Escalation tree

| Situation | First action | Escalate to |
|---|---|---|
| SLO burn red (per `DAY-1-DASHBOARD-SPEC.md` §M4) | Already PagerDuty-paged | SRE-OC + CTO; status page transition per `STATUS-PAGE-SPEC.md` §5 |
| Privacy incident suspected | Pause status-page auto-publish (privacy uses different path) | VPSec + CTO + Legal + DPO; invoke `CRISIS-COMMS-TEMPLATES.md` §B |
| BYOK vendor down | SRE-OC paged | CTO + VPSec; status page → C4 partial outage |
| Press hostile inbound | PR firm responds first | CEO informed; CEO decides direct response |
| Engineering Gate revoked mid-launch | Halt all forward T-0 actions immediately | CEO + CTO; invoke `CRISIS-COMMS-TEMPLATES.md` §F (defer comms) |
| Status page itself down | Invoke static fallback `corelink.dev/status` + tweet from `@corelinkdev` | SRE-OC + VPMkt |

## 6. Wind-down at T+72h

1. Run launch retro 2 per `LAUNCH-CHECKLIST-V2.md` row L44. Output: `marketing/launch/retros/RETRO-T-PLUS-72H.md`.
2. Confirm no open SEV-1 / SEV-2. If open: do not dissolve the war room — convert it to an incident war room.
3. Archive Zoom bridge.
4. Rename Slack channel to `#launch-archive-<date>` and set to read-only.
5. Rotate PagerDuty back to standard schedule.
6. Snapshot the Day-1 dashboard to `marketing/launch/metrics/SNAPSHOT-T+72H.md`.
7. WR-COORD writes a one-page "lessons learned" appendix into the retro doc.

## 7. Cross-references

- `marketing/launch/LAUNCH-CHECKLIST-V2.md` — canonical T-24h..T+72h checklist.
- `marketing/launch/STATUS-PAGE-SPEC.md` — status page operations.
- `marketing/launch/CRISIS-COMMS-TEMPLATES.md` — crisis comms templates (5 scenarios).
- `marketing/launch/DAY-1-DASHBOARD-SPEC.md` — launch-day metrics dashboard.
- `marketing/launch/COORDINATION/LAUNCH-RUNBOOK.md` — broader T-7d..T+7d runbook.
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` — oncall escalation tree.
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — blameless retro template.
- `ROADMAP-TO-GA.md` §8 — R-8 Launch wave parent.
