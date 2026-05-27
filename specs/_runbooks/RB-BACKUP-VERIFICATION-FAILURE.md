---
id: "RB-BACKUP-VERIFICATION-FAILURE"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-15"
updated: "2026-05-27"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "r-prep", "backup-verification", "dr", "daily-cadence", "slo-backup-verification"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-backup-verify` was absorbed into `corelink-ops` via inline `mod dr::backup_verify;` per SEAL `specs/_audits/sealed/2026-05-26-w35-p2-ops-absorption.md`. Canonical consumer path is now `corelink_ops::dr::backup_verify::*` (verification trait + in-memory fake). Operational references using the absorbed crate path still work for HISTORICAL log/grep reference but new automation should use the umbrella.

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §3, §10.

# RB-BACKUP-VERIFICATION-FAILURE — Daily Backup Verification Alert Response

> **Status:** ACTIVE. Owned by SRE Lead. Triggers from
> `.github/workflows/backup-daily-verify.yml` whenever
> `scripts/backup-daily-verify.sh` reports a non-zero exit (≥ 1 tier
> returned `stale` / `corrupt` / `restore_failed`).

## 1. Purpose

The daily backup verification harness runs every day at 05:00 UTC and
catches silent backup failures between cold-restore drill cycles.
When it fires an alert, **time matters** — backups that aren't
verifiable today may not be restorable tomorrow. This runbook is the
canonical response procedure.

Related but distinct:
- `RB-BACKUP-VERIFICATION` — monthly **proactive** restore cycle.
- `RB-COLD-RESTORE-FROM-ZERO` — emergency full-restore path.
- `SLO-BACKUP-VERIFICATION` — pass-rate SLO this runbook restores.

## 2. Trigger

A GitHub issue tagged `backup-verify` + `sev-2` (or `sev-3` for
dry-run failures during pre-handler-ship phase) is opened by
`.github/workflows/backup-daily-verify.yml`. Alternatively, an oncall
page from PagerDuty fires when 2 consecutive daily cycles fail.

## 3. Acknowledge (≤ 5 min)

- [ ] Acknowledge the page / issue.
- [ ] Open the workflow run; download the
      `backup-verification-${run_id}` artifact.
- [ ] Read the latest `.backup-verify-logs/backup-verification-YYYY-MM-DD.json`.
- [ ] Read `.backup-verify-metrics/backup-verification-YYYY-MM-DD.prom`
      to confirm exactly which `tier` × `result` combination fired.

## 4. Triage by failure mode

The verification harness reports four canonical failure statuses; each
demands a different response.

### 4.1 `result="stale"` — freshness budget exceeded

Snapshot for the named tier is older than its RPO budget (R2 24h /
D1 6h / KV 12h).

| Step | Action |
|------|--------|
| 1 | Check `scripts/backup-daily.sh` workflow runs for the last 48h. |
| 2 | If `backup-daily.sh` failed: investigate that workflow first; **do not** mark backups untrustworthy yet. |
| 3 | If `backup-daily.sh` succeeded but artifact is missing from R2 / D1 / KV catalog: open SEV-1 (silent backup drop). |
| 4 | If GPG recipient drifted: rotate per `RB-SYSTEM-CMK-ROTATION` and force a fresh `backup-daily.sh` run. |
| 5 | Manually trigger `backup-daily-verify.yml` via `workflow_dispatch` to confirm green. |

### 4.2 `result="corrupt"` — integrity sample mismatch

≥ 1 sampled object's backup hash did not match its live hash, OR the
snapshot's manifest hash mismatched on re-read.

| Step | Action |
|------|--------|
| 1 | **Treat as SEV-1** until proven otherwise (silent storage corruption is the worst class of backup failure). |
| 2 | Open the JSON log; identify the `tenant_id` + `blob_id` of every mismatched / missing sample. |
| 3 | Manually re-fetch the live blob + backup blob; recompute BLAKE3-256 hashes locally. |
| 4 | If both sides recompute clean → false positive (sampling race during `backup-daily.sh`); file post-mortem, do not page on-call again. |
| 5 | If recompute confirms divergence → mark the entire affected snapshot bad; trigger `scripts/cold-restore-drill.sh --dry-run` against the prior snapshot to confirm older snapshot is clean. |
| 6 | Notify affected tenant(s) per data-corruption disclosure obligation if production data was at risk. |

### 4.3 `result="restore_failed"` — sample restore bytes diverged

Restore succeeded but the restored object bytes did not match the live
bytes after rehash. Indicates a corruption path **during restore**
(decryption / decompression / wrangler API drift), not at-rest
corruption.

| Step | Action |
|------|--------|
| 1 | Open SEV-2 (degraded restore reliability; not yet data-loss). |
| 2 | Check Cloudflare Wrangler version; recent SDK regressions are the most common root cause. |
| 3 | Verify BYOK CMK has not rotated mid-cycle; if so, re-run with stable key. |
| 4 | Re-run the daily verify in `--dry-run` mode to isolate live-handler vs. in-memory fake. |
| 5 | If repeated in 2 consecutive cycles → escalate to SEV-1 + freeze deploy. |

### 4.4 `result="ok"` on all tiers but workflow still failed

The wall-clock SLA (30 min) was breached or the GHA runner timed out.

| Step | Action |
|------|--------|
| 1 | Open the JSON log; sum tier durations. |
| 2 | If GHA runner contention → re-run; not actionable. |
| 3 | If verification step itself is slow → file capacity ticket; no SEV until 3 consecutive overruns. |

## 5. Escalation matrix

| Trigger                                            | Severity | Page target           | Window |
|----------------------------------------------------|----------|-----------------------|--------|
| 1 daily cycle failed (dry-run mode)                | SEV-3    | SRE oncall, async     | 24h    |
| 1 daily cycle failed (live mode)                   | SEV-2    | SRE oncall, primary   | 4h     |
| 2 consecutive daily cycles failed                  | SEV-1    | SRE Lead + Eng Mgr    | 1h     |
| `corrupt` status on any tier (live mode)           | SEV-1    | SRE Lead + Eng Mgr + Security Lead | 30m |
| 3 consecutive daily cycles failed in 7d window     | SEV-1 + Exec brief | + Founder       | 1h     |

## 6. Investigation checklist

- [ ] Pulled JSON log artifact.
- [ ] Identified failing tier(s) + status code(s).
- [ ] Cross-checked `backup-daily.sh` workflow status for the last 48h.
- [ ] Recomputed at least one mismatched blob's hash locally (for `corrupt`).
- [ ] Checked BYOK key rotation events in the last 7d (audit-chain query).
- [ ] Checked Cloudflare status page for R2 / D1 / KV provider incidents.
- [ ] Cross-checked SLO burn (`SLO-BACKUP-VERIFICATION`); if budget exhausted, freeze deploys.

## 7. Restoration

Once the root cause is identified and fixed:

1. Manually trigger `backup-daily-verify.yml` via `workflow_dispatch`.
2. Confirm the run returns `result="ok"` on every tier.
3. Close the SEV issue with a one-paragraph root-cause summary.
4. File a post-mortem if severity ≥ SEV-1 (`RB-POSTMORTEM-PROCESS`).
5. If `corrupt` was confirmed: rotate the affected snapshot out of the
   trusted catalog and update the next cold-restore drill to use a
   prior-known-good snapshot.

## 8. Ownership + cadence

| Attribute | Value |
|---|---|
| Owner | SRE Lead |
| Cadence | Daily (workflow); response on-page when triggered |
| Response SLA | Ack ≤ 5 min (SEV-1) / ≤ 30 min (SEV-2) |
| SLO touched | SLO-BACKUP-VERIFICATION (≥ 99.5% rolling 30d) |
| Escalation | §5 matrix above |
| Retention | 7 years (Quality Standard 14.s17.7); artifact 90d |

## 9. References

- `scripts/backup-daily-verify.sh`
- `.github/workflows/backup-daily-verify.yml`
- `crates/corelink-backup-verify` — verification trait + in-memory fake
- `specs/03_architecture/slo_catalog.md §4.22` — SLO-BACKUP-VERIFICATION
- `specs/_runbooks/RB-BACKUP-VERIFICATION.md` — monthly cycle (sibling)
- `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` — quarterly cycle 1
- `specs/_runbooks/RB-COLD-RESTORE-FROM-ZERO.md` — emergency path
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — post-mortem template
- `specs/_runbooks/RB-GA-CUTOVER.md` §0.2.7 + §0.6 — GA cutover requires last verification ≤ 7d AND D1 backup snapshot taken at T-7d before T-0h.

---

**Fim RB-BACKUP-VERIFICATION-FAILURE.**
