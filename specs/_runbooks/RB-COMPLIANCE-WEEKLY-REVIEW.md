---
id: "RB-COMPLIANCE-WEEKLY-REVIEW"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "WT-R-PREP-COMPLIANCE-WEEKLY"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from: ["SOC2-EVIDENCE-ROLLUP-2026-05-15", "SOC2-GAP-ANALYSIS-2026-05-14", "VENDOR-RISK-REGISTER-2026-05-15"]
tags: ["runbook", "compliance", "soc2", "cc4.2", "gap-08", "weekly", "drata"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §10.

# RB-COMPLIANCE-WEEKLY-REVIEW — Monday compliance digest triage

> **Status:** ACTIVE. Owned by the Compliance Lead (Gustavo Schneiter until external advisor pool retained per GAP-04). Triggered every Monday 09:00 UTC by `.github/workflows/compliance-weekly.yml`. Operating-effectiveness evidence for **SOC 2 CC4.2** ("Evaluates and communicates deficiencies"). Closes **GAP-08** of `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`.

## 1. Purpose & scope

The Monday weekly cadence is the **deficiency-communication loop** required by SOC 2 CC4.2. The automated digest (`scripts/compliance-weekly-digest.py`) aggregates state across every compliance domain we track; this runbook tells the Compliance Lead how to triage the digest, who owns each anomaly class, and when to escalate.

**In scope:**

- SOC 2 readiness % deltas (implemented / partial / gap by criterion-count).
- GAP register: new / closed / severity-changed entries.
- Vendor risk SLA breaches.
- DR / BCP drill cadence misses.
- IR tabletop readiness flags.
- Compliance-gate CI workflow failures in the last 7d.

**Out of scope** (other cadences own these):

- LGPD DPO checklist — `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md` (monthly).
- BYOK quarterly review — `specs/_audits/templates/byok-quarterly-review.md`.
- Pentest finding triage — `specs/_runbooks/RB-PENTEST-FINDING-RESPONSE.md`.
- Drata sync failure — `specs/_runbooks/RB-DRATA-SYNC-FAILURE.md`.

## 2. Inputs

Each Monday by 09:15 UTC, the Compliance Lead has:

- A new digest file at `specs/_compliance/weekly-digests/YYYY-MM-DD.md`.
- An auto-opened PR (`auto/compliance-digest-YYYY-MM-DD`) with the digest body as PR description.
- If regression detected: PagerDuty event on service `corelink-compliance` (severity = warning) + PR comment summarising regression reasons.
- Step summary on the GitHub Actions run with the regression verdict.

## 3. Triage procedure — non-regression Monday (90% of weeks)

Even when the digest shows no regression, the Compliance Lead MUST acknowledge it in writing. This is the audit evidence trail.

### Steps (T+0 .. T+30min)

1. **Open the digest PR** linked in the workflow run summary.
2. **Verify §1 SOC 2 readiness** trends. Three rules:
   - Implemented % MUST be flat or rising week-over-week.
   - Drata dashboard % MUST be ≥ 95% (drop below = open SEV-3 ticket).
   - Auto-collection rate MUST be ≥ 90% (drop = check Drata sync runbook).
3. **Scan §2 GAP register delta** — any closed GAPs trigger archive of the closure evidence to `specs/_audits/YYYY-MM-DD-gap-XX-closure.md`.
4. **Scan §3 vendor risk** — any single SLA breach (≤ 1× cadence) → owner emailed by EOD.
5. **Scan §4 drill cadence flags** — informational warnings only; verify upcoming-week drills have facilitators assigned.
6. **Scan §5 IR tabletop** — if `T-7d brief readiness: IN WINDOW`, kick off the T-7 prep checklist in `specs/_compliance/IR-TABLETOP-PLAYBOOK.md` §6.
7. **Scan §6 CI failures** — any tagged compliance workflow failing in the last 7d → cross-reference with the owning workflow's runbook.
8. **Approve and merge the PR** with the comment template in §6.
9. **Drata ingestion** — the `incident_response` EvidenceStream picks up merged digests automatically at 03:00 UTC next day (per `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §4.1).

### Acceptance criteria for "Monday clean"

- PR approved and merged before EOD Monday UTC.
- All `informational` warnings have an owner tagged in PR comment.
- Compliance Lead signs off with the template comment in §6.

## 4. Triage procedure — regression Monday (escalation path)

When the digest exit code = 1, the workflow has already paged. The runbook covers the human steps.

### Severity matrix

| Regression class | Initial severity | Escalates to | SLA to acknowledge | SLA to close |
|---|---|---|---|---|
| Severity-upgrade of a `minor` → `major` GAP | SEV-3 | Compliance Lead | 4h | 7d |
| Severity-upgrade to `blocking-GA` | SEV-2 | Compliance Lead + Owner + Architect | 1h | 24h |
| Open GAP count rose | SEV-3 | Compliance Lead | 4h | 7d |
| Drill missed (Critical class) past grace 0d | SEV-2 | SRE Lead + Compliance Lead | 1h | 7d (re-drill) |
| Drill missed (Important class) past grace 7d | SEV-3 | SRE Lead | 4h | 14d (re-drill) |
| Vendor 2× SLA breach | SEV-3 | VP-Sec (vendor owner) | 4h | 30d (review packet) |
| Compliance-gate CI failure (cargo-audit / cargo-deny / cosign-sign / verify-fips / verify-lgpd / dr-drill / backup-daily) | SEV-3 | Workflow owner | 4h | 48h |
| Two or more concurrent regression classes | bump severity up one level | as above | as above | as above |

### Escalation triggers (the eight conditions that page PD)

The workflow triggers a PagerDuty event when **any** of the following is true (these are the canonical escalation triggers):

1. GAP severity upgrade detected (any direction `minor → major`, `major → blocking-GA`).
2. Open GAP count increased week-over-week (after first-week baseline).
3. A Critical-class drill (DR-005..DR-010, DR-15, DR-16) missed past 0d grace.
4. An Important-class drill (DR-001..DR-004, DR-011..DR-014) missed past 7d grace.
5. A Standard-class drill missed past 14d grace.
6. Any vendor in `VENDOR-RISK-REGISTER.md` past 2× its cadence window.
7. Any compliance-tagged GitHub Actions workflow with conclusion `failure` / `timed_out` / `cancelled` / `startup_failure` in the last 7d.
8. Two or more of (1)..(7) co-occur (severity escalates one tier).

(Total escalation triggers: 8 — also tracked in §3 of `specs/_compliance/weekly-digests/README.md`.)

### Steps when paged

1. **Acknowledge the PD page** within the SLA in the severity matrix.
2. **Open the digest PR** and read §7 (Regression verdict).
3. **Open a regression-handling thread** in `#compliance-regressions` Slack channel; copy the PR URL + the reasons block.
4. **For each regression reason:**
   - If GAP severity upgrade: open WI titled `WI-COMP-GAP-XX-RE-OPEN` with the new severity; assign to the GAP owner from the register summary.
   - If drill missed: file a re-drill date in `BCP-DR-DRILL-CADENCE.md`; notify SRE Lead.
   - If vendor 2× breach: kick off ad-hoc review per `specs/_runbooks/RB-VENDOR-RISK-QUARTERLY-REVIEW.md` §4 (ad-hoc lane).
   - If CI failure: open ticket against the workflow owner; link the failed run URL.
5. **Append the regression to the digest PR** as a top-of-file callout (under the H1) before merge — auditor-trail.
6. **Resolve the PD page** once each reason has a tracking issue + owner + ETA.
7. **Post-mortem-lite**: if regression class is SEV-2, file `specs/_audits/YYYY-MM-DD-compliance-regression-<gap-or-drill>.md` within 7d.

## 5. Owners & RACI

| Role | Responsibility | Backup |
|---|---|---|
| Compliance Lead | Triage every Monday; approve/merge PR; acknowledge PD pages | Owner |
| Owner (Gustavo) | Final approver of severity-upgrade reclassification; signs SEV-2 closure | VP-Sec |
| VP-Sec | Vendor-risk regression owner; drives Q/B/A review packets | Compliance Lead |
| SRE Lead | Drill-cadence regression owner; re-drill scheduling | SRE on-call |
| Privacy Officer | Privacy-tier regression (P-DSR / P-CONSENT / P-BREACH GAPs) | Owner |
| Architect | `blocking-GA` severity-upgrade closure (CC6.1 / C1.1 class) | Owner |

(See `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` for 3-tier escalation outside business hours.)

## 6. Monday clean comment template

When approving the non-regression weekly digest PR, the Compliance Lead leaves this comment to satisfy CC4.2 evidence-of-review:

```
Monday compliance review — YYYY-MM-DD

- Readiness deltas: <Implemented +/− X.X%, Partial +/− X.X%, Gap +/− X.X%>
- Drata dashboard: XX.X% (target ≥ 95%)
- GAP delta: <N closed, N new, N severity-changed>
- Vendor SLAs: all green | <N breaches followed up>
- Drill cadence: green | <flag list>
- IR tabletop: next session <TT-XX> in <Nd>
- CI gates: green | <failure list with owners tagged>

No regression. Acknowledged by Compliance Lead.
```

When regression is present, append:

```
REGRESSION reasons (from §7):
- <reason 1> → WI/issue: <link> · owner: <name> · ETA: <date>
- <reason 2> → WI/issue: <link> · owner: <name> · ETA: <date>

PD page <PD-incident-id> acknowledged at <UTC timestamp>.
```

## 7. Failure & rollback

### 7.1 Digest script fails (exit code 2 — config error)

- Workflow step "Run digest" fails; no PR opened; no PD event.
- The wrapper job leaves a step-summary line. Compliance Lead opens an issue against `scripts/compliance-weekly-digest.py` with the stderr extract.
- **Workaround:** run the digest manually on the Compliance Lead workstation via `python3 scripts/compliance-weekly-digest.py --dry-run` and paste the output into a hand-rolled `specs/_compliance/weekly-digests/YYYY-MM-DD.md` (note `audit_status: MANUAL-FALLBACK` in the top frontmatter).
- **Resolve** the script issue within 14d (CC4.2 allows one missed automated cycle before regression).

### 7.2 GitHub Actions outage on Monday 09:00 UTC

- Schedule re-fires automatically when the platform recovers.
- If outage extends past 24h: Compliance Lead manually runs the script (procedure as 7.1) to keep the cadence intact.

### 7.3 PagerDuty integration failure

- The PD step uses `secrets.PAGERDUTY_COMPLIANCE_KEY`. If the secret is missing, the workflow logs a warning and continues. Regression is still surfaced via the PR comment + label `compliance-regression`.
- Compliance Lead checks the GitHub Actions "watch all activity" filter for the `compliance-regression` label every Monday morning regardless of PD posture.

## 8. Cross-links

- Digest script: `scripts/compliance-weekly-digest.py`
- Workflow: `.github/workflows/compliance-weekly.yml`
- Digest index + methodology: `specs/_compliance/weekly-digests/README.md`
- SOC 2 rollup (CC4.2 row): `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §2.4
- GAP register (CC4.2 / GAP-08): `specs/_compliance/SOC2-GAP-ANALYSIS.md` §CC4.2
- Vendor risk register: `specs/_compliance/VENDOR-RISK-REGISTER.md`
- Vendor quarterly review: `specs/_runbooks/RB-VENDOR-RISK-QUARTERLY-REVIEW.md`
- DR drill cadence: `specs/_compliance/BCP-DR-DRILL-CADENCE.md`
- IR tabletop playbook: `specs/_compliance/IR-TABLETOP-PLAYBOOK.md`
- IR tabletop 2026 schedule: `specs/_compliance/IR-TABLETOP-SCHEDULE-2026.md`
- On-call escalation matrix: `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md`
- Drata sync failure runbook: `specs/_runbooks/RB-DRATA-SYNC-FAILURE.md`
- Roadmap §9 (Human Track / SOC 2 audit firm contract): `ROADMAP-TO-GA.md`

## 9. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7 r-prep-compliance-weekly) | Initial runbook — closes GAP-08 (CC4.2 weekly cadence); 8 escalation triggers + severity matrix + Monday clean template + 7.x failure modes. |

---

**Fim RB-COMPLIANCE-WEEKLY-REVIEW.**
