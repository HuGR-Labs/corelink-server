---
id: "GA-GATE-GO-NOGO-TEMPLATE"
type: "compliance_decision_template"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-7"
parent_wave: "R-7"
parent: "GA-GATE-CRITERIA"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["ga", "gate", "go-nogo", "decision", "r7", "ceo-cto-vpsec-vpproduct", "veto", "rollback-trigger"]
---

# GA-GATE-GO-NOGO-TEMPLATE — Go / No-Go Decision Meeting Template

> **Purpose.** The structured **60-minute decision meeting** template the **CEO / CTO / VPSec / VPProduct** use **24 h before public GA launch** to render the binary GA decision: **GO** / **GO-WITH-WAIVER** / **DEFER**. Driven by the 59 criteria in `GA-GATE-CRITERIA.md` plus the PRR-S20-GA evidence pack.
>
> **Companion.** `GA-GATE-CRITERIA.md` (the checklist) · `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md` (D+1/D+7/D+30 rollback triggers if the decision goes south).
>
> **Instantiation.** Each GA attempt copies this template to `specs/_audits/2026-MM-DD-ga-go-nogo-attempt-N.md` (`type: audit`); the audit doc is **sealed** within 24 h of the meeting and stored permanently as part of the GA evidence pack per ROADMAP-TO-GA Wave R-7.

---

## 0. Meeting metadata

| Field | Value |
|---|---|
| **GA attempt #** | 1 (or N if rollback / re-attestation cycle) |
| **Target T-0 date/time UTC** | YYYY-MM-DDTHH:MM:00Z |
| **Meeting scheduled UTC** | YYYY-MM-DD HH:00..HH:60 UTC (target T-24 h ± 4 h) |
| **Location** | Video conference (Zoom / Google Meet); recording mandatory |
| **Recording archive ID** | `s3://corelink-ga-evidence/2026-MM-DD-ga-go-nogo-attempt-N.mp4` |
| **Quorum** | 4 of 4 signers present (CEO + CTO + VPSec + VPProduct). **No proxy votes.** |

> **Solo-founder note (per ADR-0034 Option A).** Where the org operates with dual-hat sign-off (Owner = CEO + CTO + Product Lead per ADR-0034), the meeting still proceeds with **4 distinct signer slots filled by the canonical advisor pool**: CEO = Owner, CTO = Engineer Lead OR Architect, VPSec = Security Lead OR external Security Advisor, VPProduct = Product Lead OR external Product Advisor. **Dual-hat covering ≥ 2 slots invalidates the meeting** for the GA-go binary per WI-S20-001 §6.2 NP4 — the meeting must be re-scheduled with sufficient external slot coverage. The 13-canonical sign-off roster in PRR-S20-GA §3 has supersede precedence; this 4-signer meeting is the **final operational gate** that consumes those 13 sign-offs.

---

## 1. Pre-meeting checklist (T-48 h to T-24 h)

Owner (meeting facilitator) confirms each item before the meeting starts; missing any item ⇒ meeting cancelled + rescheduled.

| # | Pre-flight item | Owner | Evidence | Status |
|---|---|---|---|---|
| PRE-1 | **GA-GATE-CRITERIA.md §8 readiness summary populated** | Owner | screenshot of §8 matrix with per-track counts | ☐ |
| PRE-2 | **All 59 criteria status captured with evidence links** (zero blanks) | Owner + each track lead | per-criterion evidence URL clickable | ☐ |
| PRE-3 | **PRR-S20-GA work_status = APPROVED** OR **CONDITIONALLY_APPROVED with declared waiver list** | Owner | PRR-S20-GA frontmatter screenshot | ☐ |
| PRE-4 | **PRR-S20-CLOSING work_status = APPROVED** OR **CONDITIONALLY_APPROVED with declared deferrals** | Owner | PRR-S20-CLOSING §7 verdict screenshot | ☐ |
| PRE-5 | **R-6 30 d staging evidence digest series complete** (30 consecutive `daily-staging-evidence.md` GREEN) | SRE Lead | `specs/_audits/2026-MM-DD-30d-staging.md` summary | ☐ |
| PRE-6 | **External pentest retest letter received** (zero HIGH/CRITICAL pending; signed PDF on file) | Security Lead | `specs/_pentest/PENTEST-RETEST-LETTER.pdf` | ☐ |
| PRE-7 | **3 lighthouse customer attestations on file** (signed PDFs, attestation date < T-7 d) | Product Lead | `specs/_lighthouse/attestations/` | ☐ |
| PRE-8 | **13 PRR-S20-GA canonical sign-offs collected** (DocuSign envelope IDs) | Owner | PRR-S20-GA §3 table with all 13 status = APPROVED | ☐ |
| PRE-9 | **Screenshot evidence pack zipped** (per-criterion + per-PRE evidence ≥ 1 dashboard screenshot or doc PDF each) | Owner | `specs/_audits/2026-MM-DD-ga-evidence-pack.tar.gz` | ☐ |
| PRE-10 | **RB-GA-LAUNCH-ROLLBACK.md confirmed current** (`updated:` ≤ 30 d ago; trigger thresholds match SLO catalog) | SRE Lead + Owner | RB-GA-LAUNCH-ROLLBACK frontmatter | ☐ |
| PRE-11 | **All BLOCKED criteria documented** with root cause + ETA + waiver candidacy assessment | each track lead | inline in GA-GATE-CRITERIA.md §7 register | ☐ |
| PRE-12 | **Press / launch comms staged**; T-0 cancellation message drafted (in case of DEFER) | Owner | `marketing/launch/COMMS/T-0-DEFER-DRAFT.md` | ☐ |

If any PRE-X item is **NOT** checked at T-24 h: facilitator **cancels the meeting** + reschedules ≥ 7 d out. No exceptions.

---

## 2. Meeting agenda (60 minutes, hard timebox)

| Time | Item | Lead | Format |
|---|---|---|---|
| **0:00–0:05** | **Roll call + quorum confirm** | Facilitator (Owner) | 4 signers state name + role + connectivity; recording confirmed ON |
| **0:05–0:15** | **Engineering track readout (15 criteria)** | CTO | Dashboard share; READY/IN_PROGRESS/NOT_STARTED/BLOCKED count; for each non-READY, root cause + remediation ETA |
| **0:15–0:23** | **Security track readout (12 criteria)** | VPSec | Same format; pentest retest letter shown live; HIGH/CRITICAL count = 0 confirmed |
| **0:23–0:30** | **Operations track readout (10 criteria)** | SRE Lead (presenting; signer = CTO covers operations vote) | 30 d staging summary; on-call rotation snapshot; DR drill log |
| **0:30–0:36** | **Customer track readout (8 criteria)** | VPProduct | 3 lighthouse attestations shown; quickstart video clip; docs i18n status |
| **0:36–0:42** | **Legal/Compliance track readout (8 criteria)** | Legal Counsel (presenting; signer = VPSec OR Owner covers legal vote) | DPA signed + ToS/Privacy live + DSR pipelines green |
| **0:42–0:46** | **Marketing/Launch track readout (6 criteria)** | Owner (CEO + CMO dual-hat) | LAUNCH-RUNBOOK.md walkthrough; T-0 timeline confirmation |
| **0:46–0:54** | **BLOCKED items deep-dive + waiver discussion** | Facilitator | For each BLOCKED criterion: (1) root cause, (2) waiver candidacy, (3) compensating control, (4) waiver expiry date, (5) signer vote |
| **0:54–0:58** | **Decision matrix render** | Facilitator | See §3 below; populate verdict; collect 4 signatures |
| **0:58–1:00** | **If GO/GO-WITH-WAIVER:** confirm T-0 sequence + final go-to-press authorization; **If DEFER:** confirm re-attestation date + which criteria gate re-entry | Owner | Final words; recording stops |

If the meeting overruns: **automatic DEFER** (decision discipline; cannot rush a GA decision under time pressure).

---

## 3. Decision matrix

Apply mechanically based on §8 GA-GATE-CRITERIA.md readiness summary + §1 PRE-X confirmations + §2 deep-dive findings.

| Decision | Conditions | Required signatures |
|---|---|---|
| **GO** (full GA) | All 59 criteria `READY`; zero `IN_PROGRESS` / `NOT_STARTED` / `BLOCKED`; all 12 PRE-X checked; PRR-S20-GA APPROVED; PRR-S20-CLOSING APPROVED. | **4 of 4** (CEO + CTO + VPSec + VPProduct) **unanimous APPROVE**. Any single ABSTAIN or REJECT → **DEFER**. |
| **GO-WITH-WAIVER** | ≤ **5 criteria** WAIVED; each waiver has (1) documented risk, (2) compensating control (linked CTRL or runbook), (3) time-bound expiry ≤ 30 d post-launch, (4) signer who is granting the waiver; the 4 non-waivable items (GA-GATE-O01, C01, L01, S01-HIGH/CRITICAL) all `READY`. | **4 of 4 APPROVE** + **explicit `WAIVE` vote** on each waivered criterion by the signer whose track owns it (CEO can waive Marketing; CTO can waive Engineering; VPSec can waive Security non-HIGH/CRITICAL; VPProduct can waive Customer/Legal). |
| **DEFER** | Any non-waivable item not `READY`; OR > 5 total `WAIVED`; OR any single track < 80% `READY`; OR meeting cannot reach quorum; OR any PRE-X unchecked; OR any signer votes REJECT or ABSTAIN on GO/GO-WITH-WAIVER. | **DEFER is the default** — automatic if GO / GO-WITH-WAIVER conditions not met. No signatures required for DEFER (silence = DEFER). |

### 3.1 Waiver discipline

Waiver requires:

1. **Documented risk.** What can go wrong if this criterion is shipped not-ready. (Free-form 2-3 sentences.)
2. **Compensating control.** Concrete control or runbook that detects + mitigates the risk in production. Link to `CTRL-XX` in compliance matrix OR `RB-XX` in runbooks index.
3. **Time-bound expiry.** ≤ 30 d post-launch. After expiry the waiver auto-converts to a P0 issue; if not resolved by expiry+7 d → trigger D+30 rollback per RB-GA-LAUNCH-ROLLBACK §3.3.
4. **Sign-off by ≥ 1 VP+** present in the meeting. Recorded in §6 sign-off form below.

Waivers granted in this meeting are mirrored back to `GA-GATE-CRITERIA.md` §7 waiver register within 24 h of meeting close.

### 3.2 Non-waivable items (hard floor)

The following criteria are **never waivable** in this meeting:

- **GA-GATE-O01** — 30 d sustained staging zero P1+ (per PRR-S20-GA §6).
- **GA-GATE-C01** — 3 lighthouse customer attestations signed.
- **GA-GATE-L01** — DPA v1 signed by 3 lighthouse customers.
- **GA-GATE-S01** — pentest report with zero HIGH/CRITICAL open.
- All items in PRR-S20-GA §6 binary canonical: TLA+ green, SBOM signed, zero CRITICAL waivers, runbook 90 d cadence.

Any of these `NOT_READY` at meeting time forces **DEFER** with no exceptions.

---

## 4. Rollback trigger (forward reference)

If the decision is **GO** or **GO-WITH-WAIVER** and the launch proceeds, the following post-launch conditions trigger `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md`:

| Window | Trigger condition | Action |
|---|---|---|
| **D+1** (T+0 .. T+24 h) | Any P0 incident; OR error budget burn ≥ 10 × normal for ≥ 1 h; OR public reputational hit ("vibe check" — viral negative coverage > 100 retweets / HN front-page negative thread > 50 upvotes); OR customer mass-churn ≥ 1 lighthouse withdrawing within 24 h. | Convene emergency Go/No-Go (§5 below); decide hold/rollback. |
| **D+7** (T+1 d .. T+7 d) | P1 cluster (≥ 3 unrelated P1 incidents in 7 d); OR SLO breach sustained > 24 h on any GA-GATE-E04 perf budget; OR > 2 lighthouse customer escalations. | Convene rollback decision; consider revert-to-private-preview. |
| **D+30** (T+8 d .. T+30 d) | Any expired waiver still open as P0; OR cumulative SLA credits issued > $X (TBD per LH-1 contract); OR SOC 2 readiness regression > 5 points. | Convene quarterly review meeting; consider extended waiver vs feature-flag revert. |

See `RB-GA-LAUNCH-ROLLBACK.md` §3 for full decision tree.

---

## 5. Emergency re-convene (if rollback triggered)

If a rollback trigger fires in D+1 / D+7 / D+30 window, this template is **re-used** for the emergency Go/No-Go meeting:

- **Pre-flight**: skip §1 (no T-48 h ramp; emergency). Document the trigger + first detection time + first responder timeline (PagerDuty incident URL).
- **Quorum**: 4 of 4 still required; if any signer unreachable for > 1 h, the on-call **VP-of-Engineering proxy** can vote (documented in ADR-0034 emergency provisions).
- **Agenda**: collapse §2 into 30 min — focus on (1) what triggered, (2) ongoing impact, (3) decision: **HOLD** (continue current state with mitigations), **REVERT** (per RB-GA-LAUNCH-ROLLBACK §2 steps to private-preview), or **ESCALATE** (declare SEV-0 + war room).
- **Recording**: still mandatory.

---

## 6. Sign-off form

> Each signer states their decision verbally during §2 (0:54–0:58) AND types their name + date/time in the form below in real-time. The recording captures the verbal vote; the form captures the written signature.

### 6.1 GO / GO-WITH-WAIVER signatures

| # | Role | Name | Decision | Signed at UTC | Notes |
|---|---|---|---|---|---|
| 1 | **CEO** | _______________ | ☐ APPROVE / ☐ APPROVE-WITH-WAIVER / ☐ REJECT / ☐ ABSTAIN | YYYY-MM-DDTHH:MM:SSZ | optional 1-line |
| 2 | **CTO** | _______________ | ☐ APPROVE / ☐ APPROVE-WITH-WAIVER / ☐ REJECT / ☐ ABSTAIN | YYYY-MM-DDTHH:MM:SSZ | optional 1-line |
| 3 | **VPSec** | _______________ | ☐ APPROVE / ☐ APPROVE-WITH-WAIVER / ☐ REJECT / ☐ ABSTAIN | YYYY-MM-DDTHH:MM:SSZ | optional 1-line |
| 4 | **VPProduct** | _______________ | ☐ APPROVE / ☐ APPROVE-WITH-WAIVER / ☐ REJECT / ☐ ABSTAIN | YYYY-MM-DDTHH:MM:SSZ | optional 1-line |

**Verdict** (auto-derived): _______________ (GO / GO-WITH-WAIVER / DEFER)

### 6.2 Per-waiver sign-off (only if GO-WITH-WAIVER)

For each waivered criterion, the granting VP signs below.

| Waiver # | GA-GATE-XX | Granting VP | Documented risk | Compensating control | Expiry UTC | VP signature + timestamp |
|---|---|---|---|---|---|---|
| W-1 | _e.g. GA-GATE-M02_ | _e.g. CEO_ | _e.g. launch playbook draft pending Day-1 backfill_ | _e.g. on-call SRE rotation handles live escalation_ | YYYY-MM-DDTHH:MM:00Z | _e.g. Gustavo S., 2026-MM-DDTHH:MM:SSZ_ |
| W-2 | | | | | | |
| W-3 | | | | | | |
| W-4 | | | | | | |
| W-5 | | | | | | |

### 6.3 DEFER documentation (if applicable)

If verdict = DEFER:

- **Re-attestation date** (next attempt T-0): YYYY-MM-DD (≥ 7 d out; ≥ 30 d if non-waivable item gating)
- **Criteria gating re-entry** (which items must be `READY` before next attempt): GA-GATE-_____, GA-GATE-_____, ...
- **Owner of remediation**: _____ (CTO / VPSec / VPProduct / SRE Lead / Legal Counsel)
- **Communication plan**: ☐ Internal-only · ☐ Notify lighthouse customers · ☐ Public deferral statement
- **Press / launch comms reversal executed**: ☐ Yes (link to confirmation) · ☐ N/A (no comms pre-staged)

---

## 7. Post-meeting

Within 24 h of meeting close, the Owner / facilitator:

1. Copies this populated template to `specs/_audits/2026-MM-DD-ga-go-nogo-attempt-N.md` with `type: audit` + `doc_status: FROZEN` + signed sealing commit.
2. Updates `GA-GATE-CRITERIA.md` §7 waiver register with any new waivers (mirror table).
3. Updates `PRR-S20-GA.md` `work_status` to `APPROVED` (if GO) or keeps `CONDITIONALLY_APPROVED` (if GO-WITH-WAIVER) or leaves as-is (if DEFER) + tags release `ga-approved` (GO/GO-WITH-WAIVER only).
4. Sends meeting recording + signed audit doc + evidence pack to: lighthouse customer technical leads (advance notice 2 h before T-0), external auditor (within 24 h of T-0 for SOC 2 evidence), legal counsel (within 24 h).
5. If DEFER: triggers `marketing/launch/COMMS/T-0-DEFER-DRAFT.md` distribution + reschedules re-attestation meeting.

---

## 8. References

- `GA-GATE-CRITERIA.md` — 59 criteria checklist consumed by this template.
- `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md` — rollback decision tree for D+1 / D+7 / D+30 triggers.
- `specs/04_sprints/S20/PRR-S20-GA.md` — global PRR + 13 canonical sign-offs.
- `specs/04_sprints/S20/PRR-S20-CLOSING.md` — closing PRR + final per-canonical-source verdict.
- `ROADMAP-TO-GA.md` §7 — Wave R-7 Evidence Gate context.
- ADR-0034 — solo-tier dual-hat provisions + 4-signer minimum slot coverage.
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` — feeds the post-launch incident response if rollback triggered.

---

## 9. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Sonnet R-7 builder, worktree `wt/r7-2-ga-gate`) | Initial Go/No-Go decision template — 60 min agenda; 12 PRE-X pre-flight checks; 3-decision matrix (GO / GO-WITH-WAIVER / DEFER); waiver discipline (4 requirements + non-waivable floor); 4-signer veto unanimous protocol; D+1/D+7/D+30 rollback trigger forward refs; emergency re-convene §5; sign-off form §6 with per-waiver rows; post-meeting handoff §7. |

---

**Status:** ACTIVE. Instantiated per GA attempt; sealed audit instances live in `specs/_audits/`.
