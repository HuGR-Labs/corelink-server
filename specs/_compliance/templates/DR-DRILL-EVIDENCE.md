---
id: "TEMPLATE-DR-DRILL-EVIDENCE"
type: "template"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-14"
updated: "2026-05-27"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["template", "dr-drill", "evidence", "soc2-cc7-5", "soc2-cc9-1", "iso-27031", "bcp-dr", "wt-r6-3"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-dr-drill` was absorbed into `corelink-ops` via inline `mod <name>;` per SEAL specs/_audits/sealed/2026-05-26-w35-p2-ops-absorption.md. Canonical consumer path for the `require_staging`, `RTO_CEIL_SECONDS`, `RPO_CEIL_SECONDS` symbols referenced in this template is now `corelink_ops::dr::drill::*`. Already-FROZEN drill-evidence copies of this template remain valid as-is (audit-trail).

# DR Drill Evidence — Auditor-ready form (per-run)

> **Template.** Copy to `specs/_audits/2026-MM-DD-bcp-drill-<DRILL-ID>[-<cycle>].md` for each drill run. **MUST** be sealed (`doc_status: FROZEN`) within 24h of drill completion. Auditor-facing under SOC 2 CC7.5 (recovery testing) + CC9.1 (risk identification/mitigation) + ISO 27031 §8.4 (ICT readiness tests).
>
> **Parent:** `specs/_compliance/BCP-DR-DRILL-CADENCE.md` (R-6 90-day cadence).
>
> **Frontmatter for runs:** when this template is instantiated, the resulting evidence file MUST set `type: "audit"` (canonical type for `specs/_audits/`), keep all required fields, and add `parent_drill: "DR-XXX"` + `drill_cycle: "<a|b|c|d|n/a>"` + `sprint: "R-6"`.

---

## 0. Header (fill all)

| Field | Value |
|---|---|
| **Drill ID** | DR-XXX[-cycle] |
| **Drill name** | <descriptive title from BCP-DR-DRILL-CADENCE.md §2/§3/§4/§5> |
| **Drill phase** | P1 / P2 / P3 / X (cross-cutting) |
| **Scheduled UTC** | YYYY-MM-DDTHH:MM:00Z |
| **Actual start UTC** | YYYY-MM-DDTHH:MM:SSZ |
| **Actual end UTC** | YYYY-MM-DDTHH:MM:SSZ |
| **Duration (mm:ss)** | MM:SS |
| **Environment** | `Staging` (production drills FORBIDDEN by `corelink-dr-drill::require_staging`) |
| **Status** | `completed` / `partial` / `failed` / `cancelled` / `aborted` |
| **DrillRun row ID (D1)** | `<uuid from corelink_dr_drill.dr_drill_runs>` |
| **Incident-commander (IC)** | <engineer name + tier L1/L2/L3> |
| **Scribe** | <engineer name> |
| **Observers (if any)** | <names + roles, e.g. customer lighthouse observer for DR-009> |

---

## 1. Pre-conditions (verify BEFORE drill start)

> List each prerequisite from `BCP-DR-DRILL-CADENCE.md` for this drill ID. Mark each with ✅ / ❌. Any ❌ MUST trigger cancellation per cadence §9.

- [ ] Staging environment green for the preceding 72h (no SEV1/SEV2/SEV3 active)
- [ ] All drill prerequisites from cadence doc §<phase>.<drill> verified
- [ ] On-call primary acknowledged drill window in PagerDuty (synthetic route only)
- [ ] Status page pre-warmed with planned maintenance note (if customer-visible)
- [ ] Customer-comms templates pre-staged per `ONCALL-ESCALATION-MATRIX.md` §7 (for P2/P3)
- [ ] Legal-on-call pre-briefed (DR-008, DR-009, DR-010 only)
- [ ] Backup engineer reachable in case primary needs handoff
- [ ] Chaos-mesh / fault-injection sidecar deployed + verified healthy
- [ ] Baseline metrics snapshot captured: <link to Grafana dashboard URL + timestamp>
- [ ] BLAKE3 audit-chain checkpoint captured at T-15 min: `<chain head hash>`

**Pre-condition result:** ✅ all green / ❌ blocked — reason: <text>

---

## 2. Actions taken (chronological)

> Minute-resolution timeline of every action by every actor. Auditors look for: (a) human action vs automated action attribution, (b) decision points + who decided, (c) deviation from runbook.

| T+ | UTC time | Actor | Action | System response | Notes |
|---|---|---|---|---|---|
| T+00:00 | HH:MM:SS | <IC> | "Drill DR-XXX started, env=staging" announced in #incident-drill-DR-XXX | PD timer started | — |
| T+00:30 | HH:MM:SS | <fault injector> | Chaos action: <e.g. kill 30% worker isolates> | Error rate spike detected | — |
| T+01:15 | HH:MM:SS | <L1> | Page received + ack | MTTA = 45s ✅ | — |
| ... | | | | | |

> Append rows until drill complete. For SEV1 simulations (DR-009, DR-010), include tier-transition rows explicitly (L1 → L2 page-up time; L2 → L3 page-up time).

---

## 3. Timing (vs target)

### RTO / RPO (P1/P2 only; not applicable for P3 simulations)

| Metric | Target (ceiling) | Measured | Pass/Fail |
|---|---|---|---|
| RTO (Recovery Time Objective) | `RTO_CEIL_SECONDS` from `corelink-dr-drill` (1800s default) OR drill-specific override from cadence doc | <secs> | ✅/❌ |
| RPO (Recovery Point Objective) | `RPO_CEIL_SECONDS` from `corelink-dr-drill` (60s default) OR drill-specific override | <secs> | ✅/❌ |
| Detection-to-mitigation (DTM) | drill-specific | <secs> | ✅/❌ |
| Mitigation-to-resolution (MTR) | drill-specific | <secs> | ✅/❌ |

### Escalation timing (P3 simulations + escalating P2)

| Transition | Target SLA (from ONCALL-ESCALATION-MATRIX §3) | Measured | Pass/Fail |
|---|---|---|---|
| Detection → L1 ack | ≤ 5 min (SEV1) | <secs> | ✅/❌ |
| L1 ack → L2 page-up | 300s auto (SEV1) | <secs> | ✅/❌ |
| L2 ack → L3 page-up | 600s auto (SEV1) | <secs> | ✅/❌ |
| L3 ack | ≤ 20 min (SEV1) | <secs> | ✅/❌ |
| Status-page first update | ≤ 5 min (SEV1) | <secs> | ✅/❌ |
| Customer email (P3 sim only) | ≤ 90 min staged for review | <mins> | ✅/❌ |
| Legal join (DR-009 only) | ≤ 30 min | <mins> | ✅/❌ |

### SLO impact (production-equivalent staging slice)

| SLO | Baseline | During drill | Recovered | Burn budget consumed |
|---|---|---|---|---|
| p99 latency (gw_request) | <ms> | <ms> | <ms> | <%> |
| Availability (5xx rate) | <pct> | <pct> | <pct> | <%> |
| Drill-specific SLO (e.g. read-your-writes for DR-003) | <baseline> | <measured> | <recovered> | <%> |

---

## 4. Success criteria evaluation

> Restate the success criteria from `BCP-DR-DRILL-CADENCE.md` for this drill ID, then mark each.

| # | Criterion (from cadence doc) | Measured value | Pass/Fail | Evidence link |
|---|---|---|---|---|
| 1 | <criterion 1 verbatim> | <value> | ✅/❌ | <path/url> |
| 2 | <criterion 2 verbatim> | <value> | ✅/❌ | <path/url> |
| ... | | | | |

**Overall drill result:** ✅ **PASS** (all criteria green) / ⚠️ **PARTIAL** (1+ criteria amber, no red) / ❌ **FAIL** (1+ criteria red).

---

## 5. Failure-mode coverage (FM-IDs touched)

| FM-ID | FM description (from `failure_modes.md`) | Touched? | Mitigation pattern exercised | Holds? |
|---|---|---|---|---|
| FM-XXX | <verbatim from FM table> | ✅ | <e.g. PAT-REGION-FAILOVER-001> | ✅/❌ |
| ... | | | | |

> If any mitigation pattern fails to hold, file a follow-up WI within 7 days and link below in §7.

---

## 6. Evidence artifacts (auditor-ready bundle)

> Each item MUST be retained ≥ 7 years per `specs/_compliance/SOC2-ROADMAP.md`. Use R2 bucket `corelink-audits-7y-retention` with Object Lock per `specs/03_architecture/failure_modes.md` FM-061 mitigation (`RB-GDPR-ERASURE-HOLD` legal path for redaction exceptions).

- [ ] Grafana dashboard snapshot (PDF) — before / during / after: `<r2://path>`
- [ ] PagerDuty incident timeline (JSON export from PD API): `<r2://path>`
- [ ] D1 `dr_drill_runs` row dump (CSV): `<r2://path>`
- [ ] D1 audit-chain delta (rows added during drill window, BLAKE3-verified): `<r2://path>`
- [ ] Chaos-mesh / fault-injection report JSON: `<r2://path>`
- [ ] Status-page version diff (HTML): `<r2://path>` (P2/P3 only)
- [ ] Customer-comms draft email (P3 only — staged, not sent): `<r2://path>`
- [ ] Slack `#incident-drill-DR-XXX` channel export (JSON): `<r2://path>`
- [ ] TLA+ invariant checker output (DR-006 session-consistency; DR-013 audit-append-only): `<r2://path>` (where applicable)
- [ ] BLAKE3 audit-chain checkpoint hash (T-15m and T+15m post-recovery): `<hash_pre>` / `<hash_post>` — diff: `<n rows>`
- [ ] Postmortem-lite doc (per `RB-POSTMORTEM-PROCESS.md`): `<spec path>`
- [ ] Legal-comms draft (DR-008, DR-009 only): `<r2://path>`

---

## 7. Lessons learned + action items

> Free-form prose section, then a structured action-item table. Auditor expectation: every ❌ in §3/§4/§5 has a corresponding action item with owner + due date.

### Lessons learned (prose)

<2–3 paragraphs covering: what worked, what surprised us, where the runbook drifted from reality (FM-202 coverage), how the team performed under simulated stress, any decisions made under uncertainty.>

### Action items

| AI-ID | Description | Owner | Due | WI link | Status |
|---|---|---|---|---|---|
| AI-DR-XXX-01 | <action> | <name> | YYYY-MM-DD | WI-SXX-NNN | open / closed |
| ... | | | | | |

---

## 8. SOC 2 / ISO 27031 cross-reference

| Framework | Control | How this drill provides evidence |
|---|---|---|
| SOC 2 | CC7.5 (recovery testing) | This evidence doc + timing measurements + success-criteria result |
| SOC 2 | CC9.1 (risk identification + mitigation testing) | FM coverage matrix §5; failed mitigations → action items §7 |
| SOC 2 | A1.2 (capacity recovery, where applicable) | RTO/RPO measurements §3 |
| SOC 2 | A1.3 (environmental processing) | Cross-region routing (DR-005), residency-tag audit (no EU-data crosses to US-West) |
| ISO 27031 | §8.4 (ICT readiness tests) | Drill execution + evidence retention 7y |
| ISO 27031 | §9.1 (review + improvement) | Lessons learned §7 + action-item closure tracking |

---

## 9. Sign-off

| Role | Name | Signature (initials + date) |
|---|---|---|
| Incident-commander (drill lead) | | |
| Scribe | | |
| Eng manager (L2) — required for P2/P3 | | |
| CTO (L3) — required for P3 SEV1 simulations | | |
| Compliance reviewer — required before doc_status: FROZEN | | |

**Final seal:** Set `doc_status: FROZEN` once compliance reviewer has signed. Sealed evidence is immutable; corrections require a follow-up doc with `supersedes:` set.

---

## 10. Notes for instantiation

- Replace **all** `<...>` placeholders before sealing.
- For recurring drills (DR-011, DR-012, DR-013), append `-a` / `-b` / `-c` / `-d` to the drill ID in the filename + `drill_cycle` frontmatter field.
- Cancelled drills MUST still produce an evidence doc with `status: cancelled` + reason; do NOT skip evidence even for cancellations (auditor verifies cancellation chain).
- For PASS drills, all 10 sections are still required — auditors verify completeness, not just outcome.
