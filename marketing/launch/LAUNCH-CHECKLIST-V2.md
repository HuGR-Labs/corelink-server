---
id: "LAUNCH-CHECKLIST-V2"
type: "marketing"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "2.1.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: "marketing/launch/COORDINATION/LAUNCH-RUNBOOK.md (T-24h..T+72h slice only)"
superseded_by: null
parent: "ROADMAP-TO-GA.md §8 (R-8 Launch)"
tags:
  - "marketing"
  - "launch"
  - "checklist"
  - "r8"
  - "ga"
  - "war-room"
  - "wt-r8-1"
---

# CoreLink Launch Checklist V2 — T-7d to T+72h

> **Scope:** the 7-day-before through 72-hour-after window around the GA announcement (v2.1.0 expanded the upstream edge to T-7d to cover the demo-asset pre-stage row L0). This is the executable extract of `marketing/launch/COORDINATION/LAUNCH-RUNBOOK.md`, zoomed into the moment-of-truth window, with explicit **owner / action / success metric / fallback** columns so the war room can drive from a single page.
> **Cross-references:** `STATUS-PAGE-SPEC.md` (status page operations), `CRISIS-COMMS-TEMPLATES.md` (incident comms), `DAY-1-DASHBOARD-SPEC.md` (metrics), `specs/_runbooks/RB-LAUNCH-WAR-ROOM-COORDINATION.md` (operator playbook), `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` (escalation).
> **Time zone:** all times Pacific Time (PT). T-0 is the moment the press release wire releases at 06:00 PT on launch day.
> **Hard rule:** **Engineering Gate** (see `ROADMAP-TO-GA.md` §R-7) must already be APPROVED before any row in this checklist executes. If gate is not APPROVED at T-24h, the launch is **deferred** — see fallback row at T-24h L1.

---

## 1. Roles glossary

| Code | Role | Default occupant |
|---|---|---|
| **CEO** | Founder / executive spokesperson | Gustavo Schneiter |
| **CTO** | Technical lead / SEV-1 escalation tier | Gustavo Schneiter (dual-hat at GA) |
| **VPMkt** | Marketing lead | Marketing Lead |
| **VPSec** | Security lead | Security Lead |
| **SRE-OC** | SRE on-call (primary + secondary) | Per PagerDuty rotation |
| **PR** | External PR firm point-of-contact | Per H-9b contract (or CEO dual-hat) |
| **CS-OC** | Customer Success on-call | CS Lead |
| **WR-COORD** | War room coordinator (single-throated decision logger) | Marketing Lead (delegate possible) |
| **DPO** | Data Protection Officer (LGPD/GDPR notice authority) | Security Lead (dual-hat) |

---

## 2. T-7d to T-24h — Demo-asset pre-stage

| Row | Time | Phase | Owner | Action | Success metric | Fallback if action fails |
|---|---|---|---|---|---|---|
| L0 | T-7d 09:00 PT | Demos recorded | VPMkt + CTO | Record 5 demo assets per `marketing/launch/demos/`: 60-sec elevator, 5-min deep dive, BYOK 7-min, competitive 3-min, plus asciinema cast from `cli-asciinema-script.sh`. Capture 15 admin-UI screenshots per `admin-ui-screenshot-guide.md` with redaction checklist applied. | All 5 demo videos uploaded to CF Stream + 15 screenshots committed under `marketing/launch/demos/screenshots/` (light + dark variants for shots #06, #07, #10, #15). | Slip 24h max; below 4 of 5 demos shippable → escalate to CEO for go/no-go on launch posture (demos are not Engineering Gate blockers but are press-asset blockers). |

## 3. T-24h to T-1h — Pre-launch (Wednesday eve through Thursday 05:00 PT)

| Row | Time | Phase | Owner | Action | Success metric | Fallback if action fails |
|---|---|---|---|---|---|---|
| L1 | T-24h 06:00 PT | Engineering Gate revalidation | CEO + CTO | Reconfirm `PRR-S20-GA` work_status = `APPROVED`; no SEV-1 in last 48h staging; pentest letter still on file. | Written GO recorded in war room log. | **DEFER** — invoke `CRISIS-COMMS-TEMPLATES.md` §F (defer comms). Marketing reverts press kit recall. No further row executes. |
| L2 | T-24h 07:00 PT | Final DR drill | SRE-OC | Run `tests/disaster-recovery/dr-quarterly-drill.sh` against staging mirror; RTO ≤ 15min, RPO ≤ 5min. | Drill report committed `specs/_audits/DR-T-MINUS-24H-<date>.md`; both targets met. | If RTO/RPO breach: SRE Lead + CEO decide DEFER vs. proceed-with-known-degradation. Document decision in war room log. |
| L3 | T-24h 09:00 PT | Staging traffic verification | SRE-OC | Confirm staging traffic = projected launch-day load (`tests/load/launch-day-projection.rs`); zero new SEV-2 since dry-run. | Load test green; staging error budget burn < 5%. | Scale staging fleet +25%; if still failing → DEFER. |
| L4 | T-24h 10:00 PT | GA-Gate sign-off pack archived | WR-COORD | Pull all 13 canonical sign-offs from DocuSign; archive snapshot to `specs/_audits/GA-SIGNOFFS-FROZEN-<date>/`. | Archive contains 13 (or 5/8 minimum per ADR-0034 Option C) signatures, none revoked. | Sign-off revoked → DEFER (legal blocker). |
| L5 | T-24h 11:00 PT | Press kit final lock | PR + VPMkt | Freeze press kit; no edits past this point. Re-confirm embargo with journalist list. | Embargo confirmations received from ≥ 80% of journalist list. | Below 80% → ping holdouts directly; tolerate down to 50% before flagging risk. |
| L6 | T-24h 13:00 PT | Lighthouse customer pre-notification | CS-OC | Email 3 lighthouse customers: "we're launching at 06:00 PT tomorrow; expect press; your case studies go live; account contact reachable 24/7 launch window". | All 3 acknowledgments received. | Missing ack: phone call escalation; if still silent, exclude that customer's case study from T-0 publish set. |
| L7 | T-24h 14:00 PT | Internal all-hands brief | CEO | 30-min company-wide stand-up: confirm posture, brief on crisis comms, remind on social media discipline. | Recorded; attendance ≥ 90% of FTE. | Skip is acceptable if war room is intact; brief is shipped async via Loom. |
| L8 | T-12h 18:00 PT | Status page live | SRE-OC | Push status page banner: "CoreLink GA launches tomorrow at 06:00 PT. Subscribe for updates." (See `STATUS-PAGE-SPEC.md` §6 maintenance template; provisioning playbook `STATUSPAGE-INIT.md`; T-7d acceptance evidence `STATUSPAGE-PRE-LAUNCH-TEST.md`; lighthouse subscribers imported per `STATUSPAGE-SUBSCRIBER-IMPORT.md`.) | Banner live; subscriber count snapshot captured. | Statuspage.io outage → fallback to `corelink.dev/status` static page (pre-staged HTML). |
| L9 | T-12h 19:00 PT | Ops team holiday cancellations confirmed | SRE Lead | Verify no SRE-OC, CTO, VPSec on PTO during T-24h..T+72h window; PagerDuty rotation locked. | Rotation locked; secondary coverage explicit. | Any gap → cover with paid on-call from advisor pool (`H-15`) or DEFER. |
| L10 | T-12h 20:00 PT | Scheduled content draft staged | VPMkt | All 5 blog posts staged in CF Pages preview; press release queued in BusinessWire; LinkedIn + Twitter drafts in Buffer; Show HN body in CEO clipboard. | Preview URLs valid; scheduled-post UIs show queued. | Manual fallback: VPMkt + CEO publish from local clipboards at T-0 with stopwatch. |
| L11 | T-12h 21:00 PT | Final go/no-go check | CEO | Live 15-min sync: SRE-OC, CTO, VPMkt, VPSec, WR-COORD. Engineering gate APPROVED? On-call staffed? No new SEV-1? | Written **GO** in war room log. | **DEFER** — invoke `CRISIS-COMMS-TEMPLATES.md` §F. |
| L12 | T-6h 00:00 PT | War room booked & opened | WR-COORD | Open dedicated Slack channel `#launch-war-room-<date>`; Zoom bridge active 24h; physical war room (if Bay Area FTE present) opened. | Channel live; ≥ 4 attendees online at open. | If Slack outage: fallback to Signal group (pre-staged invite); document Slack failure in incident log. |
| L13 | T-6h 00:00 PT | Executive presence confirmed | CEO | CEO, CTO, VPMkt, VPSec all check in to war room. Names logged. | All 4 checked-in. | Any absence → designate dual-hat. CEO absence is hard-DEFER. |
| L14 | T-6h 01:00 PT | PagerDuty silence period check | SRE-OC | Verify no maintenance windows scheduled in PagerDuty for next 24h; verify all integrations green. | All integrations green. | Disable conflicting maintenance windows; re-test page-out in < 5min. |
| L15 | T-1h 05:00 PT | Press list final confirmation | PR | Final list of tier-1 + ecosystem journalists; embargo lift signal pre-staged in BusinessWire portal. | Wire portal shows "Scheduled 06:00:00 PT". | If portal misbehaves: PR firm phone-in to BusinessWire ops; 06:00 ± 5 min acceptable. |
| L16 | T-1h 05:00 PT | Lighthouse customers reminder | CS-OC | Final email: "we're 1h from announce; case studies will publish at T-0; your account contact is `<name>` reachable at `<phone>`." | All 3 emails sent; read receipts (where supported) checked. | No-receipt customer: 1 phone call attempt; if silent, proceed (case study still publishes). |
| L17 | T-1h 05:30 PT | Scheduled tweet drafts staged | VPMkt | Twitter thread + Maker comment in Buffer scheduled or in clipboard. LinkedIn post staged in CEO scheduler. | All staged; preview screenshots in war room channel. | Buffer outage → manual publish from CEO + VPMkt phones. |
| L18 | T-1h 05:45 PT | Final dashboard check | SRE-OC + WR-COORD | Day-1 dashboard (`DAY-1-DASHBOARD-SPEC.md`) loaded; all 5 metric families green. | Dashboard live in war room. | Dashboard down → fallback to direct queries documented in dashboard spec §8. |

---

## 4. T-0 — Announce (Thursday 06:00 PT)

| Row | Time | Phase | Owner | Action | Success metric | Fallback if action fails |
|---|---|---|---|---|---|---|
| L19 | T-0 00:01 PT | Product Hunt go-live | PH Hunter | Hunter posts CoreLink to Product Hunt. Maker comment from CEO at +5min. | Listing visible; first 5 upvotes within 10 min (organic). | Hunter no-show: CEO submits as Maker directly; lose Hunter visibility tag. |
| L20 | T-0 06:00:00 PT | Press release wire | PR | BusinessWire fires. PR Newswire scheduled +2h. | Wire receipt confirmation email. | Wire delay → manual distribution to top 10 journalists via direct email (pre-staged drafts). |
| L21 | T-0 06:00:00 PT | Blog posts publish (5 simultaneously) | VPMkt | Trigger CF Pages deploy publishing all 5 posts at once (`01-introducing-corelink.md` ... `05-fast-cache-hit-economics.md`). | All 5 URLs return HTTP 200; sitemap.xml updated; no broken images. | Single post fails → revert that post to draft; do not block the other 4. CF Pages outage → static S3 fallback bucket. |
| L22 | T-0 06:00:00 PT | Status page → "All systems operational" | SRE-OC | Flip status page banner from "Preparing" to "All Systems Operational"; publish launch announcement post. | Banner green; RSS feed updated. | Statuspage.io outage → manual `corelink.dev/status` static page update; raise SEV-2 internally. |
| L23 | T-0 06:00:30 PT | Trust center unlock | VPSec | Unlock TLA+ specs, SBOM, pentest letter (NDA-gated download), DPA package at `corelink.dev/trust`. | All 4 assets behind their gates load on incognito. | If NDA-gated download breaks: temporarily route to email-on-request; do not expose pentest letter publicly. |
| L24 | T-0 06:01 PT | Social cascade tier 1 | VPMkt | Twitter thread + CEO LinkedIn post + HuGR org LinkedIn re-share fire in sequence (90s apart to avoid bot flags). | First 3 posts live; preview cards render. | If LinkedIn rate-limits: stagger an extra 5 min; do not retry-spam. |
| L25 | T-0 06:05 PT | First press follow-up handler | PR | PR firm begins fielding journalist DMs / replies. Hot-list of 5 reporters monitored for inbound. | Inbound tracked in PR firm CRM. | Solo PR overwhelm: CEO joins inbound; max 3-hr CEO-on-inbound before VPMkt rotates. |
| L26 | T-0 09:00 PT | CEO LinkedIn (long form) | CEO | Publishes the long-form LinkedIn essay (`SOCIAL/LINKEDIN-POST.md`). | Post live; first 10 comments organic. | If LinkedIn editor breaks: fallback to LinkedIn article via mobile app; do not delay > 30 min. |
| L27 | T-0 10:00 PT | Show HN | CEO | Submits Show HN per `SOCIAL/HACKERNEWS-SHOW-HN.md`. **Single submission; do not resubmit.** | Live within 5 min of submit. | Front-page miss: do **not** resubmit; let it ride. Resubmission = HN community-rules violation. |

---

## 5. T+15min to T+1h — Stabilization

| Row | Time | Phase | Owner | Action | Success metric | Fallback if action fails |
|---|---|---|---|---|---|---|
| L28 | T+15min 06:15 PT | Primary SLO check | SRE-OC | Pull last 15 min of: API p99 latency, CAS read p99, CAS write p99, error rate, BYOK envelope-encrypt p99. | All 5 within SLO; error budget burn < 2% in 15-min window. | Any breach: page CTO; status page → "Investigating"; pause T-0 social tier 2 until cleared. |
| L29 | T+15min 06:15 PT | Support inbox check | CS-OC | Open support@corelink.dev; categorize first 10 tickets. | Inbox triaged; no SEV-1 customer reports. | SEV-1 customer ticket → invoke `CRISIS-COMMS-TEMPLATES.md` §A; status page update. |
| L30 | T+15min 06:15 PT | Signup flow live verification | CS-OC + Engineer | Run end-to-end signup with fresh email (`tests/e2e/signup-launch-day.spec.ts`). Verify Clerk → Stripe → first CAS write path. | E2E green; new account visible in admin console. | E2E red → page CTO; status page → "Partial Outage / Signups Affected"; halt T-0 social tier 2. |
| L31 | T+30min 06:30 PT | Signup conversion snapshot | WR-COORD | Snapshot to Day-1 dashboard: signups in first 30 min vs. projection. | Signups ≥ 50% of 30-min projection. | < 50%: not a fallback action, a tracking note. Continue. |
| L32 | T+1h 07:00 PT | War room sync 1 | WR-COORD | 15-min sync: SRE posture, CS posture, PR inbound, signup count, PH/HN rank. | Sync recorded in war room log. | Missing attendee: log the gap; sync proceeds with remaining quorum. |
| L33 | T+1h 07:00 PT | CSAT spot-check on first 10 signups | CS-OC | Manually inspect first 10 successful signups: did each complete first CAS write? Any error logs against their tenant? | ≥ 8 of 10 completed first CAS write. | < 8: investigate activation gap; do **not** crisis-comms unless > 50% activation gap. |
| L34 | T+1h 07:05 PT | Press pickup first read | PR + VPMkt | Manual sweep + ahrefs query: which tier-1 press has published? | ≥ 1 tier-1 mention live (TechCrunch / Ars / Register / similar). | Zero mentions → not a fallback; press cycles run 24-72h. Continue. |

---

## 6. T+6h to T+24h — Sustaining

| Row | Time | Phase | Owner | Action | Success metric | Fallback if action fails |
|---|---|---|---|---|---|---|
| L35 | T+6h 12:00 PT | Rolling traffic check | SRE-OC | Hourly traffic chart: signups, CAS writes, API RPS, region distribution. | Traffic monotonically rising or stable; no region imbalance > 2x. | Region imbalance: investigate residency-router; consider tactical traffic shedding. |
| L36 | T+6h 12:00 PT | Abuse signature scan | SRE-OC + VPSec | Check for: signup-bot patterns, CAS-write spam, suspicious tenant creation bursts, WAF flagged IPs > expected. | No bot signup > 5%; no WAF cluster anomaly. | Bot spike: enable Clerk bot-protection step-up; rate-limit signup endpoint; document in war room log. |
| L37 | T+6h 12:15 PT | War room sync 2 | WR-COORD | 15-min sync: SLOs, support, press pickup, social engagement, PH/HN rank. | Posture recorded. | Same as L32. |
| L38 | T+12h 18:00 PT | Mid-launch posture review | CEO + CTO + VPMkt + VPSec + SRE-OC | 30-min sync: any incidents? press tone? customer reports? continue posture or shift? | Posture decision recorded. | Posture-shift to "stabilize" → pause T+24h additional comms; focus on inbound. |
| L39 | T+12h 19:00 PT | Day-2 prep | VPMkt | Stage Day-2 social posts (testimonial re-shares, customer quote tweets, blog post 02 hero re-share). | Drafts in Buffer for 09:00 PT next day. | Skip if SEV-1 posture: do not stage; revisit at T+24h. |
| L40 | T+24h 06:00 PT | Launch retro 1 | CEO + WR-COORD + all key roles | 60-min retro: what worked, what didn't, what surprised us. Output: `marketing/launch/retros/RETRO-T-PLUS-24H.md` (created post-launch). | Retro doc committed. | Retro deferred 24h max; do not skip. |
| L41 | T+24h 06:30 PT | Metrics capture | WR-COORD | Snapshot Day-1 dashboard to immutable archive: `marketing/launch/metrics/SNAPSHOT-T+24H.md`. | All 5 metric families captured. | Manual screenshot fallback if dashboard pipeline broken; commit screenshots. |

---

## 7. T+48h to T+72h — Wind-down

| Row | Time | Phase | Owner | Action | Success metric | Fallback if action fails |
|---|---|---|---|---|---|---|
| L42 | T+48h 06:00 PT | Press wave-2 monitoring | PR + VPMkt | Wave-2 press pickups (analysts, ecosystem newsletters, second-tier press). Identify response opportunities. | ≥ 3 additional substantive mentions vs. T+24h. | Below threshold: not actionable; ride the wave. |
| L43 | T+48h 12:00 PT | Customer activation review | CS-OC | Of all T-0..T+48h signups: what % completed first CAS write? Cohort dashboard. | Activation ≥ 40% (target; not hard gate). | Below 40%: schedule activation-improvement work; not a launch-day fallback. |
| L44 | T+72h 06:00 PT | Launch retro 2 | CEO + WR-COORD + all key roles | 90-min retro: full launch debrief, learnings for next product launch, decide post-launch comms. Output: `marketing/launch/retros/RETRO-T-PLUS-72H.md`. | Retro doc committed; post-launch decision logged. | Retro deferred 48h max; do not skip. |
| L45 | T+72h 09:00 PT | Decision: post-launch comms | CEO + VPMkt | Decide: ship "CoreLink at 72h — what we learned" blog post + tweet? Or hold until 30d? | Written decision in retro doc. | Default to **hold** if any open SEV-2; ship if all green. |
| L46 | T+72h 12:00 PT | War room dissolution | WR-COORD | Close `#launch-war-room-<date>` channel; archive Zoom bridge; rotate oncall back to standard PagerDuty schedule. | Channel archived; rotation back to baseline. | If still in SEV-1: do not dissolve. Convert war room to incident war room. |

---

## 8. Hard rules

1. **Engineering Gate veto.** Any row above is voidable by the Engineering Gate. Marketing never pressures engineering to proceed.
2. **No retry-spam.** Show HN, BusinessWire, Product Hunt are one-shot. Failed submissions are not re-submitted within 24h.
3. **No coordinated upvoting / engagement-farming** on PH, HN, LinkedIn. Reputation cost > visibility upside. (See `marketing/launch/COORDINATION/LAUNCH-RUNBOOK.md` §T-0 00:15 PT.)
4. **War room log is single-throated.** WR-COORD is the only writer of canonical decisions. Discussions in Slack; outcomes in the log.
5. **Crisis comms requires CEO sign-off** unless the SEV-1 instant-publish carve-out applies — see `STATUS-PAGE-SPEC.md` §5 approval workflow.
6. **All retros are blameless.** Per `specs/_runbooks/RB-POSTMORTEM-PROCESS.md`.

---

## 9. Cross-references

- `marketing/launch/COORDINATION/LAUNCH-RUNBOOK.md` — full T-7d..T+7d superset (this checklist is the T-24h..T+72h zoom).
- `marketing/launch/demos/` — demo asset bundle (5 scripts + asciinema + 15 screenshot targets); recorded per L0 above.
- `marketing/launch/STATUS-PAGE-SPEC.md` — companion: status page configuration and severity mapping.
- `marketing/launch/STATUSPAGE-INIT.md` — companion: Statuspage.io provisioning playbook (click-through, secrets, components).
- `marketing/launch/STATUSPAGE-SUBSCRIBER-IMPORT.md` — companion: lighthouse + internal bulk-subscribe with opt-in evidence.
- `marketing/launch/STATUSPAGE-PRE-LAUNCH-TEST.md` — companion: T-7d acceptance test plan + rollback for the status page.
- `marketing/launch/CRISIS-COMMS-TEMPLATES.md` — companion: ready-to-send incident comms for 5 scenarios.
- `marketing/launch/DAY-1-DASHBOARD-SPEC.md` — companion: launch-day metrics.
- `specs/_runbooks/RB-LAUNCH-WAR-ROOM-COORDINATION.md` — companion: operator playbook for the war room itself.
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` — companion: oncall escalation paths during launch window.
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — referenced for blameless retros.
- `specs/_runbooks/RB-CUSTOMER-SUPPORT-T-90.md` — companion: T+0..T+90 customer-support runbook (comms-side ticket triage + SLA + escalation).
- `marketing/launch/SUPPORT-RESPONSE-TEMPLATES.md` — companion: 15 ticket-level response templates.
- `specs/_runbooks/RB-DSR-TICKET-TRIAGE.md` — companion: DSR inbound triage + legal escalation.
- `marketing/launch/SUPPORT-DASHBOARD-SPEC.md` — companion: support team's operational dashboard (T+0..T+90).
- `ROADMAP-TO-GA.md` §8 — parent: R-8 Launch wave.
