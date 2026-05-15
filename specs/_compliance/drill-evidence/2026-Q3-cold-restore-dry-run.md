---
id: "DRILL-EVIDENCE-2026-Q3-COLD-RESTORE-DRY-RUN"
type: "audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-6"
parent_drill: "DR-15"
drill_cycle: "a"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "dr-drill", "cold-restore", "gap-15", "dr-15", "dry-run", "soc2-a1-2", "soc2-a1-3", "soc2-cc7-5", "iso-27031"]
---

# Cold Restore Drill — 2026-Q3 Dry Run (DR-15-a)

> **Status:** TEMPLATE / DRAFT — placeholder for the first dry-run scheduled before GA staging cut. To be sealed (`doc_status: FROZEN`) within 24h of drill completion. Closes the first cycle of **GAP-15** evidence.
>
> **Parent spec:** `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md`.
>
> **Runbook executed:** `specs/_runbooks/RB-COLD-RESTORE-FROM-ZERO.md`.
>
> **Orchestrator:** `scripts/cold-restore-drill.sh --dry-run`.
>
> **Verification gate:** `scripts/verify-cold-restore.py --skip-network` (dry-run does not hit network).

---

## 0. Header (fill before sealing)

| Field | Value |
|---|---|
| **Drill ID** | DR-15-a |
| **Drill name** | Cold Restore From Zero — 2026-Q3 dry-run |
| **Drill phase** | X (cross-cutting; cold-restore-specific) |
| **Mode** | `--dry-run` |
| **Scheduled UTC** | YYYY-MM-DDTHH:MM:00Z |
| **Actual start UTC** | YYYY-MM-DDTHH:MM:SSZ |
| **Actual end UTC** | YYYY-MM-DDTHH:MM:SSZ |
| **Duration (HH:MM)** | HH:MM |
| **Environment** | Staging-equivalent (no state touched in dry-run) |
| **Status** | `completed` / `partial` / `failed` / `cancelled` / `aborted` |
| **Incident Commander (IC)** | <name + tier L1/L2/L3> |
| **Scribe** | <name> |
| **Restore Operator** | <name> |
| **BYOK Operator** | <name> |
| **Compliance Reviewer** | <name> |
| **Observers (if any)** | — (dry-run does not require observers) |

---

## 1. Pre-conditions (from COLD-RESTORE-DRILL-SPEC §5)

> Each item must be ✅ before drill is allowed to proceed. Any ❌ requires drill rescheduling per BCP-DR-DRILL-CADENCE.md §9.

### 1.1 Standing prerequisites

- [ ] Cross-region replication active (last manifest < 24h)
- [ ] N-1 region backup retention: 7 most-recent manifests present + decryptable
- [ ] BYOK CMK accessibility validated < 24h
- [ ] R2 backup-bucket Object Lock Governance ≥ 7d
- [ ] Audit-chain Merkle root recorded < 24h
- [ ] Synthetic drill tenant `cold-restore-drill-tenant` provisioned
- [ ] `corelink-drata-sync` CLI binary present

### 1.2 Per-drill prerequisites (T-24h)

- [ ] Status-page banner posted (staging-only test notice)
- [ ] Admin-plane write freeze applied to staging
- [ ] Baseline-metrics snapshot captured (Grafana PDFs)
- [ ] IC + scribe + L2/L3 backup acknowledged
- [ ] War-room channel `#drill-cold-restore-YYYY-MM-DD` created
- [ ] Synthetic-tenant fixture refreshed at `specs/_compliance/drill-evidence/fixtures/cold-restore-tenant-pre-drill.json`

**Pre-condition result:** ✅ all green / ❌ blocked — reason: <text>

---

## 2. Actions taken (chronological)

| T+ | UTC | Step | Actor | Action | System response | Notes |
|---|---|---|---|---|---|---|
| T+00:00 | HH:MM:SS | 1 | IC | Drill DR-15-a started in `--dry-run` mode | log_step 1_IC_ESTABLISHMENT_START | — |
| T+00:15 | HH:MM:SS | 1 | IC | IC role declared in war-room | log_step 1_IC_ESTABLISHMENT_DONE | — |
| T+00:30 | HH:MM:SS | 2 | Restore Op | Damage-inventory probes against synthetic "lost" region | log_step 2_INVENTORY_DONE | dry-run; no real probes |
| T+01:30 | HH:MM:SS | 3 | Restore Op | Provisioning walkthrough (Terraform plan only, no apply) | log_step 3_PROVISION_DONE | manual checkpoint stub |
| T+03:00 | HH:MM:SS | 4 | Restore Op | R2 restore primitive dry-run | log_step 4_R2_RESTORE_DONE | restore-from-snapshot.sh --dry-run |
| T+04:00 | HH:MM:SS | 5 | Restore Op | D1 invariant checks (read-only) | log_step 5_D1_VERIFY_DONE | — |
| T+04:00 | HH:MM:SS | 6 | Restore Op | KV tenant_id integrity check | log_step 6_KV_VERIFY_DONE | parallel with step 5 |
| T+06:00 | HH:MM:SS | 7 | BYOK Op | DO state hydration + envelope-unwrap dry-run | log_step 7_DO_BYOK_DONE | no real KMS calls |
| T+07:00 | HH:MM:SS | 8 | All | verify-cold-restore.py --skip-network | log_step 8_SMOKE_PASS | format-validation only |
| T+08:00 | HH:MM:SS | 9 | IC | Customer-comms draft staged (no send); evidence prepared | log_step 9_GOLIVE_DONE | — |

---

## 3. Timing (vs target)

| Metric | Target | Measured | Pass/Fail |
|---|---|---|---|
| Detection → IC establishment | ≤ 15 min | <mm:ss> | ✅/❌ |
| RTO read-path | ≤ 4h (14400s) | <mm:ss> | ✅/❌ |
| RTO write-path (full) | ≤ 8h (28800s) | <mm:ss> | ✅/❌ |
| RPO measurement (snapshot age) | ≤ 15 min | <m> min | ✅/❌ |
| Audit-chain Merkle root continuity | bit-for-bit | match / mismatch | ✅/❌ |
| BYOK DEK SHA-256 continuity | bit-for-bit | match / mismatch | ✅/❌ |
| SLO histograms recovered ≤ 15min post-step-8 | < 15min | <m> min | ✅/❌ |

> **Note (dry-run only):** RTO/RPO measurements in dry-run are walkthrough-only; the production-equivalent timing is recorded but not enforced. The first `--staging` mode execution (DR-15-b, scheduled before GA staging cut) is the first run that enforces the full timing budget.

---

## 4. Success criteria evaluation (from COLD-RESTORE-DRILL-SPEC §7)

| # | Criterion | Measured | Pass/Fail | Evidence |
|---|---|---|---|---|
| 1 | RTO read-path met | <T> | ✅/❌ | log file |
| 2 | RTO write-path met | <T> | ✅/❌ | log file |
| 3 | RPO met (≤ 15 min) | <m> | ✅/❌ | log file `rpo_measurement_minutes` |
| 4 | Audit-chain Merkle root continuity | match | ✅/❌ | verify-cold-restore.py output |
| 5 | BYOK envelope integrity | match | ✅/❌ | verify-cold-restore.py output |
| 6 | SLO histograms recovered | yes | ✅/❌ | verify-cold-restore.py output |
| 7 | Zero production-tenant impact | yes | ✅/❌ | production status-page diff |

**Overall drill result:** ✅ PASS / ⚠️ PARTIAL / ❌ FAIL (mark one)

---

## 5. Per-step timing vs budget

| Step | Title | Budget | Actual | Δ |
|---|---|---|---|---|
| 1 | Establish IC + war room | 15 min | <mm:ss> | <+/-> |
| 2 | Inventory damage | 15 min | <mm:ss> | <+/-> |
| 3 | Provision new region | 60 min | <mm:ss> | <+/-> |
| 4 | Restore R2 from backup | 90 min | <mm:ss> | <+/-> |
| 5 | Restore D1 (parallel) | 60 min | <mm:ss> | <+/-> |
| 6 | Restore KV (parallel with 5) | 60 min | <mm:ss> | <+/-> |
| 7 | DO state hydration | 120 min | <mm:ss> | <+/-> |
| 8 | Smoke tests | 60 min | <mm:ss> | <+/-> |
| 9 | Customer comms + go-live | 60 min | <mm:ss> | <+/-> |

---

## 6. Verification gate output (verify-cold-restore.py)

```text
PASTE FULL OUTPUT HERE — including each C1..C4 line + duration + any 'extra' diagnostic.
Also paste the JSON report from --output drill-verification-<ts>.json (truncated to relevant fields).
```

---

## 7. Failure-mode coverage (FM-IDs touched)

| FM-ID | Description | Touched | Mitigation pattern | Holds? |
|---|---|---|---|---|
| FM-050 | R2 bucket outage | ✅ | PAT-REGION-FAILOVER-001 | ✅/❌ |
| FM-051 | Backup corruption | ✅ | DR-13 prereq + step 4 SHA check | ✅/❌ |
| FM-052 | R2 cross-region routing fail | ✅ | sibling-bucket allocation | ✅/❌ |
| FM-055 | D1 primary loss | ✅ | step 5 restore | ✅/❌ |
| FM-061 | Audit-log retention loss | ✅ | step 5 Merkle root check | ✅/❌ |
| FM-062 | Audit-chain gap | ✅ | verify-cold-restore.py C3 | ✅/❌ |
| FM-101 | Cross-region failover | ✅ | step 3 + 4 | ✅/❌ |
| FM-105 | Region-wide outage | ✅ | entire runbook | ✅/❌ |
| FM-204 | BYOK CMK access | ✅ | step 7 + verify-cold-restore.py C2 | ✅/❌ |

---

## 8. Evidence artifacts

- [ ] Drill log (`drill-cold-restore-YYYY-MM-DD-HH-MM.log`) — r2://`<path>`
- [ ] Verification output (`drill-verification-YYYY-MM-DD-HH-MM.json`) — r2://`<path>`
- [ ] Grafana baseline (T-24h) PDF — r2://`<path>`
- [ ] Grafana post-restore (T+8h) PDF — r2://`<path>`
- [ ] Customer-comms draft (staged, not sent) — r2://`<path>`
- [ ] War-room channel export (Slack JSON) — r2://`<path>`
- [ ] PagerDuty incident timeline (JSON) — r2://`<path>`
- [ ] Pre-drill fixture file frozen snapshot — `specs/_compliance/drill-evidence/fixtures/cold-restore-tenant-pre-drill.json` @ commit `<sha>`
- [ ] Drata upload receipt (HTTP 200) — `<evidence_id>`

---

## 9. Deltas vs RTO/RPO targets

> Document any deviation from the targets in §3. For PASS drills with margin, note margin size (informational for capacity planning). For PARTIAL/FAIL, root-cause analysis below.

<prose: 2–3 paragraphs>

---

## 10. Lessons learned + action items

### 10.1 Lessons learned (prose)

<2–3 paragraphs covering: what worked, what surprised us, where the runbook drifted from reality (FM-202 coverage), how the team performed under simulated stress, any decisions made under uncertainty during dry-run.>

### 10.2 Action items

| AI-ID | Description | Owner | Due | WI link | Status |
|---|---|---|---|---|---|
| AI-DR-15-a-01 | <action> | <name> | YYYY-MM-DD | WI-SXX-NNN | open / closed |

---

## 11. SOC 2 / ISO 27031 cross-reference

| Framework | Control | Evidence provided here |
|---|---|---|
| SOC 2 | CC7.5 (recovery testing) | full drill timeline + verification gate output |
| SOC 2 | CC9.1 (risk identification + mitigation testing) | FM coverage §7 + lessons learned §10 |
| SOC 2 | A1.2 (backups + DR) | RTO/RPO measurements + verification gate |
| SOC 2 | A1.3 (recovery testing operating-effectiveness) | this evidence (with sustained cycles) |
| ISO 27031 | §8.4 (ICT readiness tests — full DR) | drill execution + 7y retention |
| NIST SP 800-34 | §3.4.3 (full-interruption testing) | this drill is the canonical full-interruption test |

---

## 12. Sign-off

| Role | Name | Signature (initials + date) |
|---|---|---|
| Incident Commander | | |
| Scribe | | |
| Restore Operator | | |
| BYOK Operator | | |
| SRE Lead | | |
| Compliance Reviewer | | |
| CTO (L3) — required for `--staging` and `--prod`; optional for `--dry-run` | | |

**Final seal:** Set `doc_status: FROZEN` once SRE Lead + Compliance Reviewer have signed. Sealed evidence is immutable; corrections require a follow-up doc with `supersedes:` set.

---

**End drill-evidence-2026-Q3-cold-restore-dry-run.**
