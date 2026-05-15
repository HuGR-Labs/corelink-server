---
id: "RB-TERRAFORM-DRIFT"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "SRE Lead"
final_approver: "Security Lead"
reviewers: []
supersedes: null
superseded_by: null
parent: "WI-R-PREP-TERRAFORM"
tags: ["runbook", "r-prep", "terraform", "drift", "iac", "soc2", "cc6.1"]
---

# RB-TERRAFORM-DRIFT — Terraform drift triage

> **Owned by:** SRE Lead (co-owned with Security Lead for production scope).
> **Triggered by:** failure of `.github/workflows/terraform-drift.yml`
> (daily 03:00 UTC cron or manual dispatch), or
> `.github/workflows/terraform-lint.yml` red on a PR touching
> `infra/terraform/**`.
>
> **SLA:**
>
> | Severity | Condition | MTTR target |
> |---|---|---|
> | SEV-3 | Drift detected (`plan -detailed-exitcode == 2`) | 24h to reconcile |
> | SEV-2 | Terraform error (`exit 1`) on apply pipeline | 4h |
> | SEV-1 | Drift on a security-control resource (KMS role, secret, IAM) | 1h, page oncall |

---

## 1. Trigger detection

A drift event is recognised when any of the following fires:

1. Scheduled run of `terraform-drift` reports exit code 2 in a region job.
2. Manual `terraform plan` on `main` shows resources to change/delete that
   weren't introduced by an open PR.
3. CloudTrail / Cloudflare audit log shows a console-level change to a
   Terraform-managed resource (out-of-band mutation).
4. Auditor (Schellman / Drata) flags missing evidence for an IaC-managed
   resource during a CC6.1 walkthrough.

If ANY of (1..4) occurs, this runbook applies.

---

## 2. Roles

| Role | Responsibility |
|---|---|
| **Incident commander (IC)** | Oncall SRE for staging drift; Security Lead for production drift |
| **Reviewer** | CODEOWNERS on the affected module path |
| **Approver** | SRE Lead for staging; SRE Lead + Security Lead for production |

---

## 3. Initial diagnosis (≤ 15 min)

1. Pull the latest `terraform plan` artifact from the workflow run.
2. Identify the resource(s) showing drift. Categorise into one of:
   - **A. Spurious drift** — provider added a default attribute / API
     shape changed; resource is intentionally unmanaged. Add to
     `lifecycle.ignore_changes`. Document in module comment.
   - **B. Console mutation** — a human edited the resource in the
     dashboard. Verify identity from audit log. Decide: revert via
     `terraform apply` (preferred) OR codify via PR if change is legit.
   - **C. State corruption** — state shows a resource that no longer
     exists (or vice versa). Run `terraform state rm` / `terraform import`
     under supervision.
   - **D. Provider bug** — version bump caused phantom diff. Pin previous
     version, file upstream issue, link in module.

3. Open a tracking issue: `INC-TF-DRIFT-<YYYYMMDD>-<env>-<region>`.

---

## 4. Reconciliation (case A)

```
# In the affected module main.tf:
lifecycle {
  ignore_changes = [
    <attribute_that_drifted>,
  ]
}
```

PR + CODEOWNERS review + terraform-lint green → merge.

---

## 5. Reconciliation (case B — console mutation)

**Preferred path:** revert.

1. Run `terraform plan` from a fresh checkout against the affected env.
2. Verify the plan shows ONLY the reverting changes (no incidental diffs).
3. Open a PR with the plan output attached as artifact.
4. Get CODEOWNERS approval.
5. Run the manual apply workflow (dual-approval for production).
6. Verify post-apply `plan -detailed-exitcode == 0`.

**Alternate path:** codify (only if the console change is intentional).

1. Open a PR that adds the corresponding Terraform resource matching the
   console state.
2. Run `terraform import` from the apply workflow.
3. Verify drift cleared.

In BOTH cases: file an INC retro within 48h examining how the
out-of-band change was made and whether IAM / dashboard access should be
revoked.

---

## 6. Reconciliation (case C — state corruption)

State surgery is high-risk; always:

1. Take a snapshot of the current state (`terraform state pull > snapshot-$(date -u +%FT%TZ).tfstate`).
2. Store snapshot in the incident issue as an attachment.
3. Run `terraform state rm` / `terraform import` in a dry-run session
   first (locally with read-only backend).
4. Pair on the actual apply with a second SRE.
5. Re-run `terraform plan` after every state mutation.

---

## 7. Reconciliation (case D — provider bug)

1. Pin the previous working provider version in the module's
   `required_providers` block.
2. Run `terraform init -upgrade=false`.
3. File an upstream issue linked from the module comment.
4. Track unpinning in a follow-up ticket once the upstream fix lands.

---

## 8. Verification gate

Before closing the incident:

- [ ] `terraform plan -detailed-exitcode == 0` from a fresh checkout.
- [ ] `terraform-lint` workflow green on a sentinel PR touching the module.
- [ ] `terraform-drift` next scheduled run (or manual dispatch) green.
- [ ] Incident retro filed within 48h if root cause is in case B or C.
- [ ] If the drifted resource was a secret / IAM role, run
      `scripts/secrets-checklist-verify.sh` to confirm matrix coherence.

---

## 9. Communication

- **SEV-3 (drift only):** post in `#infra-drift` Slack channel; no page.
- **SEV-2 (apply error):** post in `#infra-drift`; page if blocking a
  release.
- **SEV-1 (security-control drift):** page via PagerDuty
  `corelink-oncall`; notify Security Lead + SRE Lead immediately.

---

## 10. Audit evidence (SOC 2 CC6.1)

Each drift incident produces:

- A linked PR (closing the drift).
- The `terraform plan` artifact attached to the workflow run (90d
  retention).
- The incident issue with timeline + root cause.

Auditors sample one drift event per quarter; the IC files the evidence
bundle into the quarterly compliance evidence pack.

---

## 11. Cross-links

- `infra/terraform/README.md` — IaC layout + backend setup.
- `.github/workflows/terraform-drift.yml` — daily drift detection job.
- `.github/workflows/terraform-lint.yml` — PR-gate lint workflow.
- `specs/_runbooks/RB-FM-206.md` — auto-apply prohibition.
- `specs/_runbooks/RB-SECRETS-DRIFT.md` — secrets-matrix drift triage.
- `docs/internal/secrets-checklist.md` — single source of truth for secrets.
