---
id: "RB-FM-206"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "operational", "iac", "terraform", "drift", "wi-s13-004"]
dry_run_cadence: "monthly"
sla_detect: "≤ 24h (daily cron)"
sla_remediate: "≤ 7d"
---

# RB-FM-206 — Terraform Drift (estado real ≠ definido em IaC)

> **FM:** FM-206 (RPN=36, P1) | **PAT:** PAT-DRIFT-DETECTION-001 | **SLA:** detect ≤ 24h · remediate ≤ 7d
> **Dry-run cadence:** monthly | **Auto-apply:** FORBIDDEN | **Channel:** #infra-drift

---

## 0. At-a-Glance

| Field | Value |
|---|---|
| Trigger | Daily cron 03:00 UTC OR manual `workflow_dispatch` |
| Detection MTTD | ≤ 24h (daily cron bound) |
| Alert channel | Slack #infra-drift SEV-3 |
| Remediation SLA | ≤ 7 days from detection |
| Auto-apply | **FORBIDDEN** — always manual + dual-approval (WI-S13-002) |
| Escalation > 7d | SEV-2 + post-mortem + Architect + Security Lead |
| Audit evidence | D1 `terraform_drift_findings` table (compliance-grade; 7y retention) |

---

## 1. Detection

### 1.1 Automated detection (primary)

The GitHub Actions workflow `.github/workflows/terraform-drift.yml` runs daily at 03:00 UTC and on `workflow_dispatch`.

For each of 5 regions (`us-east`, `us-west`, `eu-west`, `ap-southeast`, `sa-east`):
1. `terraform init -backend-config=backend-<region>.hcl`
2. `terraform plan -detailed-exitcode -out=plan-<region>.tfplan`

Exit codes:
- **0** — no diff; clean run row inserted in D1 (cron health check).
- **1** — terraform error; SEV-2 alert; drift detection is blind for this region.
- **2** — diff exists; SEV-3 alert; finding inserted in D1.

### 1.2 Manual detection

Run `workflow_dispatch` from GitHub Actions UI or:
```sh
gh workflow run terraform-drift.yml
```

Or run locally (requires Cloudflare credentials):
```sh
cd infra/terraform
terraform init -backend-config=backend-us-east.hcl
terraform plan -detailed-exitcode
```

### 1.3 Cron skip detection

If the daily cron has not run in > 24h:
- Monitoring (Prometheus `corelink_admin_terraform_cron_runs_total` + GitHub Actions workflow status) fires SEV-3 alert.
- Action: trigger `workflow_dispatch` immediately + investigate cron health.

---

## 2. Severity Classification

| Diff count | Severity | Action |
|---|---|---|
| 0 | `none` | Clean run — no action required; cron health row inserted. |
| 1–2 | `low` | Investigate within 7d; likely tag/metadata change. |
| 3–10 | `medium` | Investigate within 3d; structural resource change. |
| > 10 | `high` | Investigate within 24h; major configuration change detected. |
| Any (exit 1) | `none` | SEV-2 error — fix terraform error; drift detection is blind. |

---

## 3. Decision Tree (RB-FM-206 Core)

When a SEV-3 alert fires (`drift_detected=true`, exit code 2):

```
DRIFT DETECTED
│
├─ Step A: Who changed what?
│   ├─ Check Cloudflare audit log (CF dashboard → Audit Log)
│   ├─ Check git log for recent `.tf` commits
│   └─ Check D1 admin_ops log (WI-S13-002) for recent admin operations
│
├─ CASE 1: Drift accidental (manual CF console change; not intended)
│   ├─ Decision: REVERT
│   │   1. `terraform apply` to revert to IaC state (manual workflow + dual-approval)
│   │   2. Update D1 finding: status=remediated, decision=revert
│   │   3. Post-mortem light: why was console used instead of PR?
│   │   4. Add CODEOWNERS reminder / process note
│   └─ OR Decision: IMPORT (if console change should persist)
│       1. Add resource to `.tf` + `terraform import`
│       2. Open PR with change + CODEOWNERS review
│       3. Update D1 finding: status=remediated, decision=apply
│
├─ CASE 2: Drift intentional (emergency fix via console; documented)
│   ├─ Decision: APPLY (legitimate drift; reconcile IaC to actual)
│   │   1. Confirm with author via Slack/PR comment
│   │   2. Document change in `.tf` file
│   │   3. Open PR → CI runs `terraform plan` for review
│   │   4. CODEOWNERS review + required reviews
│   │   5. Merge → manual `terraform apply` via separate workflow + dual-approval
│   │   6. Update D1 finding: status=remediated, decision=apply
│   └─ Acceptable drift pattern (e.g. timestamp field)?
│       1. Add `lifecycle { ignore_changes = ["field_name"] }` to resource
│       2. Open PR + document in acceptable-drift-patterns ADR
│       3. Update D1 finding: status=wontfix, decision=investigate
│
├─ CASE 3: Drift unexpected (no known owner; root cause unclear)
│   ├─ Decision: INVESTIGATE
│   │   1. Update D1 finding: status=investigating, decision=investigate
│   │   2. Assign to SRE on-call; escalate to Architect if > 48h
│   │   3. Cross-reference: Cloudflare incident history; recent platform changes
│   │   4. If platform-driven: contact Cloudflare support
│   │   5. Document root cause in D1 finding notes field
│   └─ If adversarial suspected → escalate to CASE 4
│
└─ CASE 4: Drift malicious (adversarial; unauthorized change)
    ├─ Escalate to Security Lead IMMEDIATELY (SEV-1)
    ├─ Decision: INVESTIGATE + REVERT
    │   1. Pull full audit log from CF dashboard: who + when
    │   2. Revoke Cloudflare credentials for affected admin account
    │   3. `terraform apply` to revert (manual + dual-approval)
    │   4. Open security incident post-mortem
    │   5. Update D1 finding: status=remediated, decision=revert
    └─ SLA: immediate escalation; resolve within 4h
```

---

## 4. Communication Protocol

### SEV-3 (drift detected; not blocking)

1. Slack #infra-drift SEV-3 alert auto-posted by GH Actions.
2. Tag SRE on-call.
3. Tag admin who made recent changes (via `git blame` on `.tf` files).
4. Post update in #infra-drift when investigation starts.
5. Post resolution summary when remediated.

### SEV-2 (terraform error OR drift > 7d)

1. Slack #infra-drift + #oncall.
2. Page SRE on-call via PagerDuty.
3. Escalate to Architect + Security Lead.

### Post-mortem trigger (drift > 7d without remediation)

Per spec contract §18: open post-mortem doc + drift discipline review with Architect + Security Lead.

---

## 5. Remediation Procedures

### 5.1 Manual `terraform apply` (CASE 1 REVERT or CASE 2 APPLY)

> **CRITICAL: Never run `terraform apply` via the CI drift detection workflow. It is FORBIDDEN.**
> Use the separate manual apply workflow with dual-approval (WI-S13-002).

```sh
# 1. Review plan first (already available as artifact from detection run)
gh run download <run_id> --name terraform-plan-<region>-<run_id>

# 2. Inspect plan
terraform show plan-<region>.tfplan

# 3. Trigger manual apply workflow (dual-approval required)
gh workflow run terraform-apply-manual.yml \
  --field region=<region> \
  --field finding_id=<uuid> \
  --field decision=apply
```

### 5.2 Terraform import (CASE 2 — legitimate change)

```sh
# Import resource that was created manually
terraform import <resource_type>.<resource_name> <resource_id>

# Verify state matches
terraform plan -detailed-exitcode
# Should return exit code 0 after successful import
```

### 5.3 `lifecycle.ignore_changes` (acceptable drift pattern)

```hcl
# In the resource block where field drifts expectedly:
resource "cloudflare_worker_script" "corelink_worker" {
  # ... other fields ...

  lifecycle {
    ignore_changes = [
      # Documented acceptable drift: Cloudflare updates this field daily
      "last_modified_on",
    ]
  }
}
```

Document in acceptable-drift-patterns ADR when adding new ignore_changes entries.

### 5.4 State file restoration (corruption or tampering)

```sh
# Cloudflare R2 backend supports versioning — list previous versions
# (exact CLI depends on backend configuration)

# 1. List state file versions via R2 API
cf r2 object list <bucket> --prefix "corelink/<region>/terraform.tfstate"

# 2. Identify last known-good version (before corruption timestamp)
# 3. Restore via backend console or R2 API
# 4. Verify: terraform plan should exit 0 after valid state restore
terraform plan -detailed-exitcode
```

---

## 6. D1 Audit Update (mandatory after every remediation)

All remediation decisions MUST be recorded in D1 `terraform_drift_findings` via the admin API (WI-S13-002, dual-approval gated):

```
POST /v1/admin/ops
X-Dual-Approver: <approver_user_id>
X-Approver-Signature: <hmac>

{
  "op_type": "TerraformDriftRemediate",
  "finding_id": "<uuid>",
  "decision": "apply" | "investigate" | "revert",
  "notes": "Root cause: [brief description]"
}
```

The D1 row update (`status`, `remediation_decision`, `remediated_at_ms`, `remediated_by_user_id`) is atomic with the audit CloudEvent emission.

---

## 7. Escalation Matrix

| Condition | Severity | Action |
|---|---|---|
| Drift detected | SEV-3 | SRE on-call notified; 7d remediation SLA starts |
| Drift > 7d | SEV-2 | Escalate to Architect + Security Lead; post-mortem opens |
| Cron skip > 24h | SEV-3 | Investigate GH Actions cron health; trigger dispatch |
| Cron skip > 48h | SEV-2 | Post-mortem + ops review |
| Terraform error (exit 1) | SEV-2 | Fix terraform error; drift detection blind |
| Drift > 10 resources | `high` | Investigate within 24h |
| State file tampering | SEV-2 | Immediate escalation; restore state + security review |
| Adversarial drift | SEV-1 | Immediate escalation; revoke credentials; security incident |

---

## 8. Prevention Checklist

- CTRL-AUDIT-002: all Cloudflare console changes are audit-logged.
- CI runs `terraform plan` on every PR touching `.tf` files.
- CODEOWNERS for `infra/terraform/` — changes require senior review.
- Required PR reviews before merge to `main`.
- No long-lived Cloudflare API credentials (OIDC-bound tokens only; WI-S13-004 §6.1).
- `terraform apply` via CI is **FORBIDDEN** (CI lint gate: `scripts/lint_no_terraform_apply.py`).
- Acceptable drift patterns documented in ADR + enforced via `lifecycle.ignore_changes`.
- Backend state file versioning enabled (R2 object versioning).

---

## 9. Monthly Dry-Run Procedure

Per `failure_modes.md §RB cadence line 278` and WI-S13-004 §6.1 item 3, this runbook MUST be dry-run monthly.

### Dry-run steps

1. **Synthesize drift in staging**: make a manual change in Cloudflare staging console (e.g. modify a Worker environment variable).
2. **Trigger detection**: run `gh workflow run terraform-drift.yml` against staging.
3. **Verify detection**: confirm SEV-3 alert posted in #infra-drift + D1 row inserted.
4. **Execute decision tree**: follow CASE 1 (revert); apply `terraform apply` via manual workflow.
5. **Verify remediation**: run drift detection again; confirm exit code 0.
6. **Update D1**: record remediation decision via admin API.
7. **Capture timeline**: document in `specs/_audits/YYYY-MM-DD-rb-fm-206-dry-run.md`.

### Dry-run report template

See `specs/_audits/YYYY-MM-DD-rb-fm-206-dry-run.md` (committed by WI-S13-006).

---

## 10. Acceptable Drift Patterns (initial allowlist)

> **Empty at GA** — populated incrementally as patterns are identified and documented via ADR.

| Pattern | Resource | Field | `lifecycle.ignore_changes` | Documented Date |
|---|---|---|---|---|
| _(none yet)_ | — | — | — | — |

---

## 11. Related References

- **WI-S13-004** — implementation spec (GitHub Action + D1 schema + this runbook).
- **WI-S13-002** — admin API + dual-approval gate (for `TerraformDriftRemediate` op).
- **FM-206** — `failure_modes.md` entry (RPN=36, P1, PAT-DRIFT-DETECTION-001).
- **PAT-DRIFT-DETECTION-001** — `resilience_patterns.md §3.7`.
- **INV-AUDIT-APPEND-ONLY** — `invariant_registry.md §3.6`.
- **CTRL-AUDIT-002** — Cloudflare console audit logging.
- **`.github/workflows/terraform-drift.yml`** — detection workflow.
- **`infra/slack/terraform-drift-template.json`** — Slack alert template.
- **`migrations/d1/0024_terraform_drift_findings.sql`** — D1 schema.

---

## 12. Change Log

| Version | Date | Author | Change |
|---|---|---|---|
| 0.1.0 | 2026-04-24 | Gustavo (via Claude Opus 4.7) | Initial stub. |
| 1.0.0 | 2026-05-14 | Gustavo (via Claude Sonnet 4.6) | Full WI-S13-004 runbook: decision tree + 4 cases + dry-run procedure + D1 audit update + escalation matrix + prevention checklist. |
