---
id: "WI-S13-004"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-28"
updated: "2026-04-28"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-005"]
parent: "S-13"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "SECURITY-MODEL"
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
tags: ["wi", "s13", "admin-plane", "terraform-drift", "drift-detection", "rb-fm-206", "high-risk"]
---

# WI-S13-004 — Terraform Drift Detection Daily 03:00 UTC + `terraform plan` per Region + Slack SEV-3 Alert + RB-FM-206 Manual Remediation Runbook + Auto-Apply Forbidden (Manual Gate)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-13](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S13-004 |
| Título | GitHub Action daily cron 03:00 UTC executando `terraform plan` para todas regiões CoreLink; if diff > 0 → SEV-3 alert Slack #infra-drift com diff snippet + Slack message + drift findings em D1 audit; manual remediation runbook RB-FM-206 (terraform-drift) com decision tree (apply vs investigate vs revert); auto-apply forbidden — manual approval mandatory (PAT-DRIFT-DETECTION-001 + FM-206 mitigation) |
| Sprint | S-13 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-005 (drift causes unexpected behavior; auto-apply bypass = catastrophic; manual remediation gate = security baseline) |

## 1. Intent

Implementar terraform drift detection daily para mitigar **FM-206** (Terraform drift — estado real ≠ definido em IaC): (1) **GitHub Action daily cron** 03:00 UTC; (2) `terraform plan` per region (currently CoreLink targets Cloudflare Workers + R2 + D1 + Neon Postgres em ~5 regiões + global config); (3) if `terraform plan` diff count > 0 → **SEV-3 alert** posted em Slack #infra-drift channel com diff summary + full plan output uploaded as artifact; (4) **RB-FM-206 manual remediation runbook** (`specs/05_runbooks/RB-FM-206.md`) com decision tree: (a) apply changes (drift legitimate; reconcile to IaC), (b) investigate (drift unexpected; investigate root cause), (c) revert manual change (drift via manual edit; revert to IaC); (5) **auto-apply forbidden** — `terraform apply` requires manual human approval; CI Action only runs `terraform plan`; mitigates FM-206 catastrophic auto-apply incidents (e.g., AWS console manual change reverted by CI = production down).

```yaml
# .github/workflows/terraform-drift.yml (forward; documented spec only — NO code in this WI)
name: Terraform Drift Detection
on:
  schedule:
    - cron: '0 3 * * *'  # 03:00 UTC daily
  workflow_dispatch:    # manual trigger for ad-hoc check

jobs:
  drift-check:
    strategy:
      matrix:
        region: [us-east, us-west, eu-west, ap-southeast, sa-east]
    runs-on: ubuntu-22.04
    permissions:
      contents: read
      id-token: write   # OIDC para Cloudflare API auth
    steps:
      - uses: actions/checkout@v4
      - uses: hashicorp/setup-terraform@v3
        with:
          terraform_version: 1.7.X
      - run: terraform init -backend-config=backend-${{ matrix.region }}.hcl
        working-directory: infra/terraform/
      - run: terraform plan -detailed-exitcode -out=plan.tfplan
        id: plan
        working-directory: infra/terraform/
        continue-on-error: true   # capture exit code; we handle differently
      - if: steps.plan.outputs.exitcode == '2'  # 2 = diff exists
        uses: slackapi/slack-github-action@v1.27.0
        with:
          channel-id: 'infra-drift'
          payload: |
            {
              "text": "SEV-3 Terraform drift detected in region ${{ matrix.region }}",
              "blocks": [...]
            }
      # NOTE: NO `terraform apply` step; auto-apply forbidden per RB-FM-206
```

D1 schema (drift findings audit log):
```sql
CREATE TABLE terraform_drift_findings (
    finding_id BLOB(16) PRIMARY KEY,
    region TEXT NOT NULL,
    detected_at_ms BIGINT NOT NULL,
    plan_diff_count INTEGER NOT NULL,
    plan_summary TEXT NOT NULL,
    plan_full_artifact_url TEXT,                -- GitHub Actions artifact URL
    severity TEXT NOT NULL CHECK (severity IN ('none', 'low', 'medium', 'high')),
    status TEXT NOT NULL CHECK (status IN ('open', 'investigating', 'remediated', 'wontfix')),
    remediation_decision TEXT,                  -- 'apply' | 'investigate' | 'revert' (per RB-FM-206 decision tree)
    remediated_at_ms BIGINT,
    remediated_by_user_id BLOB(16),             -- admin who remediated (dual-approval gated)
    runbook_ref TEXT NOT NULL DEFAULT 'RB-FM-206'
);
CREATE INDEX idx_terraform_drift_open ON terraform_drift_findings(status, detected_at_ms) WHERE status = 'open';
```

## 2. Narrative (HIGH_RISK ≥ 300 palavras + risk justification)

Terraform drift é root cause comum de incidentes: estado real (Cloudflare console, R2 bucket policy, D1 schema, Neon Postgres role) diverge from IaC committed em git; next `terraform apply` reverts manual change inadvertently OR new IaC change applies sobre stale state. Mitigation requires both detection (daily plan) + manual gate (no auto-apply). FM-206 has 36 risk score em failure_modes.md (P=3, D=3, I=4) = P1 priority.

**Bugs catastróficos possíveis** (todos endereçados):

1. **Auto-apply incident**: CI Action runs `terraform apply` autonomously; manual change reverted; production down. Mitigação: WORKFLOW EXPLICITLY ONLY runs `terraform plan` (NEVER `terraform apply`); manual approval mandatory; no `auto-merge` for terraform PRs (CODEOWNERS + required reviews).

2. **Drift detection bypass**: cron disabled or fails silently; drift accumulates undetected. Mitigação: GitHub Actions workflow status check; D1 `terraform_drift_findings` row inserted per run (even if 0 drift); cron skipped > 24h alert SEV-3 separately.

3. **Slack alert noise**: false-positive drift (e.g., timestamps in Cloudflare resources) floods channel; on-call ignores. Mitigação: ADR documents acceptable drift patterns (e.g., `last_modified_at` field ignored via `lifecycle.ignore_changes`); curated drift summary excludes acceptable patterns.

4. **Drift remediation skipped (sustained > 7d)**: detected but not remediated; trust degradation. Mitigação: post-mortem trigger spec contract §18 — drift > 7d sem remediation = post-mortem + drift discipline review.

5. **Manual remediation creates new drift**: admin manually applies via Cloudflare console; subsequent `terraform plan` shows drift. Mitigação: RB-FM-206 decision tree mandates `terraform apply` after manual changes; admin trained on procedure; D1 audit log tracks decision.

6. **Terraform state file corruption**: backend state file corrupted; `terraform plan` returns false drift. Mitigação: backend state versioning (S3 versioning equivalent); backup state file daily; recovery procedure in RB-FM-206.

7. **Permission escalation via terraform**: attacker compromises CI credentials; modifies IaC + auto-apply; resources tampered. Mitigação: NO auto-apply; OIDC-bound credentials (no long-lived secrets); CODEOWNERS for `infra/terraform/`; required PR reviews.

8. **Cross-region drift correlation**: drift em region X correlates with drift em region Y (e.g., shared module change); separate alerts mask root cause. Mitigação: matrix workflow runs all regions; aggregator step posts summary if multi-region drift detected.

**Atacante adversarial scenarios**:

- **Manual Cloudflare console change**: attacker compromises admin Cloudflare account; modifies Worker config; daily drift detection catches diff em next run (≤ 24h MTTD).

- **Terraform state file tampering**: attacker modifies state file in backend; `terraform plan` reports false-clean. Mitigação: state file integrity check (SHA-256 vs last known-good); alert SEV-2 if hash mismatch.

- **CI credential exfiltration + auto-apply attempt**: attacker steals GitHub Actions OIDC token; tries `terraform apply`; workflow lacks apply step (only plan); apply requires separate manual workflow with additional approval.

**Risk justification HIGH_RISK**:

- **FF-HR-005**: drift = unexpected behavior; auto-apply bypass = catastrophic.
- **Reversibility**: detection ≤ 24h MTTD; remediation ≤ 7d (RB-FM-206); but undetected drift can persist sustained = trust degradation + compliance gap.

11 sign-offs canonical incl. SRE Lead (cron + drift remediation cadence) + Architect (terraform module composition + state file integrity) + AppSec (CI credential security + auto-apply forbidden enforcement).

## 3. Customer Impact & Journey

**Persona 1 — SRE on-call em prospect enterprise (RFP)**:
- Evidence: daily drift detection cron + RB-FM-206 runbook + D1 audit log; 24h MTTD bound.
- Diferenciador: BuildBuddy/NativeLink lack terraform drift detection; CoreLink S-13 = AWS CloudFormation drift detection-equivalent + manual gate.

**Persona 2 — Auditor SOC 2 Type II + ISO 27001**:
- Audit query: SOC 2 CC8.1 (system change management); ISO 27001 A.5.18 (access provisioning).
- Evidence: D1 `terraform_drift_findings` table + RB-FM-206 dry-run report + monthly drift trend chart.

**Persona 3 — Internal admin executing terraform change**:
- Workflow: open PR with terraform change → CI runs `terraform plan` for review → CODEOWNERS + required reviews → merge → manual `terraform apply` workflow with dual-approval (composed WI-S13-002).

**SLA addendum**:
- Drift detection MTTD: ≤ 24h (daily cron).
- Drift remediation: ≤ 7d (RB-FM-206 cadence).
- Cron run cadence: daily 03:00 UTC.
- Acceptable drift patterns documented em ADR.
- Manual approval required for all `terraform apply`.

## 4. Capability Mapping

- **CAP-ADMIN-004** (Terraform drift detection daily) — IMPLEMENTA primary.
- Trace: `_spec_contract.md §4 + §5.4` + `resilience_patterns.md §3.7 (PAT-DRIFT-DETECTION-001)` + `failure_modes.md FM-206` + `security_model.md §6.7 (Runtime Hardening — config management)`.

## 5. Tipo

CI workflow + runbook + D1 audit; HIGH_RISK; FF-HR-005.

## 6. Escopo

### 6.1 In-scope

1. **`.github/workflows/terraform-drift.yml`** (forward; documented spec only):
   - Daily cron 03:00 UTC.
   - Manual `workflow_dispatch` for ad-hoc check.
   - Matrix over 5 regions: us-east, us-west, eu-west, ap-southeast, sa-east.
   - Steps: `terraform init` (backend per-region) → `terraform plan -detailed-exitcode -out=plan.tfplan`.
   - Exit code handling: 0 = no diff; 1 = error; 2 = diff exists.
   - On exit code 2: post Slack SEV-3 alert + upload plan artifact.
   - On exit code 1: post Slack SEV-2 alert (terraform error) + upload logs.
   - **NO `terraform apply` step** (security property; auto-apply forbidden).
   - Permissions: `contents: read` + `id-token: write` (OIDC for Cloudflare API auth).
   - OIDC-bound credentials only (no long-lived AWS/CF secrets in workflow).

2. **D1 schema migration `terraform_drift_findings`** (vide §1):
   - Per-run row inserted (even if 0 drift) for cron health check.
   - Index `idx_terraform_drift_open` for fast query of open findings.
   - Trigger entry from GitHub Actions via webhook → Worker → D1.

3. **`specs/05_runbooks/RB-FM-206.md` runbook**:
   - Decision tree (apply / investigate / revert).
   - Investigation procedure (cross-reference Cloudflare audit log + admin changes log).
   - Revert procedure (`terraform apply` after manual change with dual-approval gate).
   - Acceptable drift patterns appendix (`lifecycle.ignore_changes` documented).
   - Escalation matrix (SEV-3 → SRE on-call; > 7d → SEV-2 → Architect + Security Lead).
   - Dry-run cadence: monthly (per `failure_modes.md §RB cadence` line 278).

4. **Slack alert template**:
   - Channel: #infra-drift.
   - SEV-3 message format: `[SEV-3] Terraform drift detected in region {region}: {diff_count} resources changed. See plan artifact: {url}. Runbook: RB-FM-206.`.
   - Includes: region, diff count, top 5 changed resources, artifact URL, runbook reference.
   - Pinned daily summary message updated per cron run (top of channel).

5. **Métricas underscored Prometheus** (per `observability_model.md §3.1 + §4.1`):
   - `corelink_admin_terraform_drift_findings_total{region,severity}` (counter; severity ∈ none|low|medium|high).
   - `corelink_admin_terraform_drift_age_hours_gauge{finding_id,region}` (gauge; alert > 168h = 7d sustained).
   - `corelink_admin_terraform_cron_runs_total{outcome}` (outcome ∈ ok|drift_detected|terraform_error|workflow_failed).
   - `corelink_admin_terraform_remediation_duration_hours_bucket{region}` (histogram; from detection to remediated).

6. **Observability** — trace span `admin.terraform_drift.{cron_run,alert_post,remediate}` com attributes:
   - `terraform.region` (string).
   - `terraform.diff_count` (u32).
   - `terraform.severity` (enum).
   - `terraform.remediation_decision` (enum: apply|investigate|revert).
   - `result` (enum).

7. **Audit emission** (CloudEvent per cron run + per remediation):
   - `corelink.admin.terraform_drift.detected` (when diff > 0).
   - `corelink.admin.terraform_drift.remediated` (when status → remediated; via admin API forward dual-approval gate).
   - Atomic batch with D1 INSERT.

8. **Adversarial regression tests**:
   - Inject synthetic drift (manual Cloudflare console change in staging); verify next cron detects diff + Slack alert + D1 row.
   - Cron skip > 24h: verify alert SEV-3 fires (workflow status check).
   - Auto-apply attempt: verify workflow definition has no apply step; CI gate blocks PRs adding apply.
   - State file integrity tampering: simulate corrupt backend state; verify alert SEV-2.
   - Manual remediation drift: admin applies Cloudflare console change; subsequent cron detects re-drift; runbook decision tree triggered.

9. **Integration test E2E**:
   - Trigger workflow_dispatch in staging; verify `terraform plan` runs for all 5 regions in parallel.
   - Inject synthetic drift; verify SEV-3 alert posted + D1 row inserted.
   - Manual remediation flow: admin invokes admin API `POST /v1/admin/ops` with op_type=`TerraformDriftRemediate` (composed WI-S13-002 dual-approval); D1 row updated to status=remediated.

10. **RB-FM-206 dry-run** (covered also em WI-S13-006 ship gate):
    - Synthesize drift; execute runbook decision tree; capture timeline + decision rationale; commit dry-run report.

### 6.2 Out-of-scope (deferred)

- **DO config-singleton**: WI-S13-001.
- **Admin API + dual-approval**: WI-S13-002 (composed for remediation flow).
- **Secret rotation worker**: WI-S13-003.
- **Progressive rollout controller**: WI-S13-005.
- **Property tests + RB-FM-205 dry-run + PRR doc**: WI-S13-006.
- **Auto-apply with multi-engineer approval**: pós-GA enterprise (current = manual only).
- **Terraform Cloud/Enterprise integration**: pós-GA (current = open-source CLI).
- **Multi-cloud drift detection**: Cloudflare-only at GA.
- **Drift trend visualization dashboard** (Grafana custom panel): forward DASH-ADMIN expansion S-16.
- **Acceptable drift patterns ADR**: forward (initial empty allowlist; populated as patterns identified).

## 7. Anti-Scope

- **Auto-apply via CI** (security property; never).
- Skip cron daily cadence (drift detection MTTD bound mandatory).
- Skip D1 audit log (compliance evidence mandatory).
- Skip RB-FM-206 manual remediation runbook.
- Long-lived AWS/CF secrets em CI (OIDC only).
- Skip CODEOWNERS for `infra/terraform/`.
- Skip required PR reviews for terraform changes.
- Bash automation > 30 lines (Rust binary preferred for harness; runbook is Markdown).
- Silent skip of cron failures (alert SEV-3 if cron skipped > 24h).
- Acceptable drift unbounded (ADR-documented allowlist only).
- Direct D1 INSERT bypass (Worker binding only).

## 8. Acceptance Criteria (Gherkin)

```gherkin
Feature: Terraform drift detection daily + RB-FM-206

  Background:
    Given GitHub Actions workflow terraform-drift.yml configured
    And matrix over 5 regions
    And D1 terraform_drift_findings table operational
    And Slack #infra-drift channel configured

  Scenario: Daily cron 03:00 UTC runs successfully (no drift)
    Given no drift in any region
    When cron triggers
    Then terraform plan runs per region (5 parallel jobs)
    And exit code = 0 (no diff)
    And D1 row inserted (severity="none", status="open" but auto-closed)
    And metric corelink_admin_terraform_cron_runs_total{outcome="ok"} incremented

  Scenario: Drift detected → SEV-3 Slack alert
    Given Cloudflare console manual change in region us-east (Worker config modified)
    When cron triggers
    Then terraform plan detects diff in us-east (exit code = 2)
    And Slack #infra-drift posts SEV-3 message with diff summary + plan artifact URL
    And D1 row inserted (region="us-east", severity="medium", status="open")
    And metric corelink_admin_terraform_drift_findings_total{region="us-east",severity="medium"} incremented
    And audit "admin.terraform_drift.detected" emitted

  Scenario: Auto-apply forbidden (workflow definition has no apply step)
    When PR adds `terraform apply` step to workflow
    Then CODEOWNERS + AppSec review required
    And CI gate blocks PR (custom lint forbids apply step)

  Scenario: Cron skip > 24h alerts SEV-3
    Given last successful cron run > 24h ago
    When monitoring detects skip
    Then SEV-3 alert posted Slack
    And metric corelink_admin_terraform_cron_runs_total{outcome="workflow_failed"} incremented

  Scenario: Manual remediation via admin API (dual-approval gated)
    Given drift finding open (region="us-east")
    When admin POST /v1/admin/ops body={op_type: "TerraformDriftRemediate", finding_id, decision: "apply"}
      And X-Dual-Approver: B + valid HMAC signature
    Then dual-approval verified (composed WI-S13-002)
    And admin manually triggers terraform apply via separate workflow (not auto)
    And D1 row updated (status="remediated", remediation_decision="apply", remediated_by_user_id=A, remediated_at_ms=now)
    And audit "admin.terraform_drift.remediated" emitted

  Scenario: Drift sustained > 7d post-mortem trigger
    Given drift finding open with detected_at_ms > 7d ago
    When monitoring detects > 7d
    Then post-mortem doc auto-opened (per spec contract §18)
    And drift discipline review escalated to Architect + Security Lead

  Scenario: State file tampering detected
    Given backend state file SHA-256 mismatch with last known-good
    When integrity check runs
    Then SEV-2 alert + state restore procedure RB-FM-206 invoked

  Scenario: Acceptable drift pattern ignored
    Given Cloudflare resource has timestamp field that updates daily
    Given lifecycle.ignore_changes = ["last_modified_at"]
    When cron triggers
    Then terraform plan ignores the field (exit code = 0)
    And no false-positive alert

  Scenario: Cross-region drift aggregation
    Given drift detected in us-east + us-west simultaneously
    When matrix workflow completes
    Then aggregator step posts summary alert (multi-region pattern)
    And D1 rows inserted per region

  Scenario: RB-FM-206 dry-run executed (covered em WI-S13-006)
    Given synthesized drift in staging
    When RB-FM-206 decision tree executed
    Then timeline + decision rationale captured
    And dry-run report committed
```

## 9. Design Decisions

### 9.1 Why daily cron 03:00 UTC

- 03:00 UTC = lowest customer activity globally; minimal impact if cron causes load spike.
- Daily cadence balances: detection MTTD (24h bound) vs CI cost (~$1/dia per region × 5 regions × 30 = $150/mês).
- Industry standard (AWS CloudFormation drift detection cron daily).

### 9.2 Why `terraform plan` only (NÃO apply)

- Auto-apply = catastrophic risk class (bypass manual review = production down).
- `plan -detailed-exitcode` returns 0/1/2 (no-diff/error/diff) sem state mutation.
- Manual `terraform apply` requires separate workflow + dual-approval gate.

### 9.3 Why matrix per-region (NÃO single global plan)

- Per-region failure isolation: drift em us-east não blocks detection em us-west.
- Parallel execution faster (5 jobs ~3 min each = 3 min total wall).
- Per-region backend state (separate state files).

### 9.4 Why RB-FM-206 manual remediation (NÃO auto-investigate)

- Drift root cause varies (manual change vs IaC drift vs platform drift); auto-investigate brittle.
- Manual decision tree tested via dry-run; runbook accuracy validated.
- Composed with dual-approval (WI-S13-002) for actual remediation execution.

### 9.5 Why Slack channel #infra-drift (NÃO #all-alerts)

- Drift alerts ≠ urgency of SLO violations; separate channel reduces noise.
- Pinned daily summary message provides at-a-glance status.
- SRE on-call rotation watches #infra-drift specifically.

### 9.6 Why D1 audit log (NÃO only Slack)

- Slack ephemeral (channel history limits); D1 = compliance-grade audit.
- Query-able for monthly trend reports + audit access.
- Index `idx_terraform_drift_open` enables fast query of open findings.

### 9.7 Why OIDC-bound credentials (NÃO long-lived secrets)

- Industry trend: GitHub Actions OIDC = short-lived cert per workflow.
- Eliminates threat class secret exfiltration via CI compromise.
- Cloudflare API supports OIDC federation.

### 9.8 Why post-mortem trigger > 7d sustained

- 7d = sufficient buffer for legitimate investigation cycle.
- > 7d = drift discipline gap requiring root cause analysis.
- Spec contract §18 mandates trigger.

### 9.9 Why no ADR for this WI

- Pattern reused (PAT-DRIFT-DETECTION-001 já documented em `resilience_patterns.md §3.7`); no novel architecture decision.
- Acceptable drift patterns ADR is forward-cumulative (populated incrementally as patterns identified).

## 10. Completeness Criteria SOTA

- [ ] **10.s13.004.1** GitHub Actions workflow `terraform-drift.yml` documented spec + matrix per region (EVT-027).
- [ ] **10.s13.004.2** D1 `terraform_drift_findings` schema migration applied (EVT-018).
- [ ] **10.s13.004.3** RB-FM-206 runbook committed em `specs/05_runbooks/RB-FM-206.md` com decision tree (EVT-017).
- [ ] **10.s13.004.4** Slack #infra-drift alert template tested (synthetic drift injection) (EVT-013).
- [ ] **10.s13.004.5** Métricas (4 listadas §6.1.5) emitting em staging (EVT-013).
- [ ] **10.s13.004.6** Adversarial test: synthetic drift injection + cron skip + state file tampering + acceptable drift filter (EVT-040).
- [ ] **10.s13.004.7** RB-FM-206 dry-run report committed (covered em WI-S13-006) (EVT-017).
- [ ] **10.s13.004.8** Cost regression gate: GitHub Actions ≤ $20/mês + D1 + Slack negligible.
- [ ] **10.s13.004.9** OWASP ASVS V14 (configuration) 100% checklist pass (EVT-002).
- [ ] **10.s13.004.10** Drift detected → remediated cycle ≤ 7d sustained 30d staging *(GA Evidence Gate D+45)*.

## 11. DoD

- [ ] Workflow definition `terraform-drift.yml` committed (spec only; deployment per WI-S13-006).
- [ ] D1 schema migration applied.
- [ ] RB-FM-206 runbook committed com decision tree + dry-run cadence monthly.
- [ ] Slack alert template configured + tested.
- [ ] Métricas emitidas (4 listadas).
- [ ] Trace spans em OTel.
- [ ] CODEOWNERS for `infra/terraform/` configured.
- [ ] CI gate forbids `terraform apply` step in workflow (custom lint).
- [ ] OIDC credentials configured (no long-lived secrets).
- [ ] Adversarial regression tests green.
- [ ] Integration test E2E green em staging.
- [ ] Code review (SRE Lead + Architect + AppSec).
- [ ] PRR mini-sign-off (ship gate é WI-S13-006).
- [ ] Cost regression gate green.

## 12. Invariants Validated

### Mantidas

- **INV-AUDIT-APPEND-ONLY** (CRITICAL — registry §3.6 herdada S-09): drift findings + remediations append-only.
- **INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER** (CRITICAL — herdada S-03): D1 atomic batch.

### Novas

Nenhuma (drift detection = operational pattern; sem novel runtime invariant).

TLA+ alignment: não-aplicável (CI cron + audit emission; sem state machine cripto).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| Terraform drift workflow | `.github/workflows/terraform-drift.yml` | YAML (GitHub Actions) |
| D1 migration `terraform_drift_findings` | `migrations/0XX_terraform_drift_findings.sql` | SQL |
| RB-FM-206 runbook | `specs/05_runbooks/RB-FM-206.md` | Markdown |
| Slack alert template | `infra/slack/terraform-drift-template.json` | JSON |
| CI gate custom lint (forbid apply) | `scripts/lint_no_terraform_apply.py` | Python |
| Drift cron consumer Worker | `crates/corelink-terraform-drift-consumer/` | Rust |
| Adversarial tests | `crates/corelink-terraform-drift-consumer/tests/adversarial.rs` | Rust |
| E2E integration | `tests/e2e_terraform_drift.rs` | Rust |
| RB-FM-206 dry-run report (template) | `specs/_audits/2026-XX-XX-rb-fm-206-dry-run.md` (committed em WI-S13-006) | Markdown |

## 14. Quality Standards SOTA

- **14.s13.004.1** Zero `unsafe`; zero `unwrap` em src/.
- **14.s13.004.2** Workflow YAML lint passing (actionlint).
- **14.s13.004.3** Test coverage drift consumer Worker ≥ 90%.
- **14.s13.004.4** Latência: cron run completion ≤ 5 min wall (matrix 5 regions parallel).
- **14.s13.004.5** SAST: cargo-audit + cargo-deny + clippy clean for consumer Worker; actionlint for YAML.
- **14.s13.004.6** Métricas RED + age tracking.
- **14.s13.004.7** Runbook RB-FM-206 dry-run cadence monthly.
- **14.s13.004.8** Breaking changes em D1 schema = bump major + migration plan.
- **14.s13.004.9** Memory bounded em consumer Worker.
- **14.s13.004.10** Cost regression gate em CI.
- **14.s13.004.11** OIDC-bound credentials only.
- **14.s13.004.12** CODEOWNERS for `infra/terraform/`.

## 15. Chaos Experiments

1. **Synthetic drift injection in staging**: manual Cloudflare console change; verify next cron detects + alerts + D1 row.

2. **Cron skip > 24h**: disable cron temporarily; verify monitoring alert SEV-3 fires.

3. **Auto-apply attempt**: open PR adding `terraform apply` step; verify CI lint rejects.

4. **State file tampering**: corrupt backend state file (test backend); verify integrity check alert.

5. **Acceptable drift filter**: add resource with timestamp field; verify `lifecycle.ignore_changes` filters false-positive.

6. **Multi-region drift**: inject drift in 2 regions simultaneously; verify aggregator summary alert.

7. **CI credential exfiltration simulation**: red team validates OIDC-bound credentials are short-lived (≤ 1h).

8. **Manual remediation drift**: admin manually applies Cloudflare console change; subsequent cron detects re-drift; RB-FM-206 decision tree triggered.

## 16. PRR (Production Readiness Review)

PRR HIGH_RISK 11 sign-offs canonical (S-13 ship gate é WI-S13-006; este WI passa por mini-PRR SRE Lead + AppSec review):

- [ ] All 10 Gherkin scenarios green.
- [ ] Adversarial tests green.
- [ ] E2E synthetic drift detection green.
- [ ] Cost regression gate green.
- [ ] Métricas + Slack template configured.
- [ ] RB-FM-206 runbook committed.
- [ ] SRE Lead review (cron + remediation cadence).
- [ ] Architect review (terraform module composition).
- [ ] AppSec review (CI credential security + auto-apply forbidden enforcement).
- [ ] CODEOWNERS for `infra/terraform/` configured.

## 17. Sub-tasks

| ID | Sub-task | Estimativa |
|---|---|---|
| ST-001 | Workflow `.github/workflows/terraform-drift.yml` skeleton | 1.5h |
| ST-002 | Matrix per-region + OIDC credentials | 1h |
| ST-003 | Slack alert posting + template | 1.5h |
| ST-004 | D1 migration `terraform_drift_findings` | 1h |
| ST-005 | Drift consumer Worker (GitHub webhook → D1) | 2h |
| ST-006 | RB-FM-206 runbook escrita + decision tree | 2h |
| ST-007 | CODEOWNERS for `infra/terraform/` | 0.5h |
| ST-008 | CI gate custom lint (forbid `terraform apply`) | 1h |
| ST-009 | Métricas emit (4 metrics) + trace spans | 1h |
| ST-010 | Adversarial regression tests (5 scenarios) | 2h |
| ST-011 | E2E integration test synthetic drift | 1.5h |
| ST-012 | Code review (SRE Lead + Architect + AppSec) iteration | 1.5h |

**Total Optimistic**: ~16h. **PERT** (O=8h, M=12h, P=20h, per spec contract §12): **12.7h**. Sub-tasks soma é detail-grain; PERT spec contract é consolidated.

## 18. Dependencies

### Hard blockers

- **WI-S13-002 SEALED** (admin API + dual-approval gate for manual remediation flow `TerraformDriftRemediate` op type composed).
- Cloudflare OIDC federation operational (platform-level; not blocker for spec).

### Soft blockers

- Slack workspace + #infra-drift channel configured (operations task; not blocker for spec).
- Terraform module `infra/terraform/` baseline committed (existing repo state).

### Outbound

- **WI-S13-006** PRR ship gate consumes RB-FM-206 dry-run report.

## 19. Effort PERT

O: 8h, M: 12h, P: 20h → PERT **12.7h** (per spec contract §12).

## 20. Time-boxing

**16h hard limit**. If exceeded → escalation: split em sub-WI (workflow + alerts vs runbook + D1 audit).

## 21. Observability

4 métricas listadas §6.1.5. Trace spans em §6.1.6. Logs structured JSON.

Dashboard widget DASH-ADMIN:
- Terraform drift findings per region (daily trend 30d).
- Drift age (oldest open finding hours).
- Cron run status (ok/drift/error/skipped).
- Remediation duration histogram.

## 22. Cost Analysis

- GitHub Actions: 5 regions × 1 run/dia × 5 min × $0.008/min = $1.20/dia = $36/mês.
- Slack notifications: free (within plan).
- D1 writes: ~5 rows/dia × 30 = 150 rows/mês × 1 KB = 150 KB; ~$0.10/mês.
- **Total custo direto WI-S13-004**: ~$36/mês = $432/yr. Bounded vs cloud-native drift detection (AWS Config $2/resource/mês × 100 resources = $200/mês).

## 23. API Contract

Não-aplicável (este WI é CI workflow + runbook + D1 audit). Admin API for remediation reuses WI-S13-002 `POST /v1/admin/ops` with op_type=`TerraformDriftRemediate`.

## 24. Post-mortem Hooks

- Drift sustained > 7d sem remediation → post-mortem + drift discipline review (per spec contract §18).
- Auto-apply incident (workflow inadvertently includes apply) → CRITICAL post-mortem + Security review.
- State file corruption → SEV-2 + ops post-mortem.
- Cron skip > 48h → SEV-2 + post-mortem.

## 25. Rollback / Recovery

- Workflow rollback: revert PR + GitHub Actions reverts to previous commit.
- D1 schema rollback: only additive (no DROP); INV-AUTH-MIGRATION-ADDITIVE herdada.
- Slack alert: ephemeral; no rollback needed.
- RTO: ≤ 30 min (workflow revert + redeploy).

## 26. Security & Privacy

**STRIDE delta**:
- **Spoofing**: OIDC-bound credentials short-lived; no long-lived secret exfiltration vector.
- **Tampering**: state file integrity check + CODEOWNERS + required reviews.
- **Repudiation**: D1 audit log + 7y retention.
- **Information disclosure**: terraform plan output excludes secrets (Cloudflare API tokens never in plan).
- **DoS**: cron daily bounded; no operational impact.
- **Elevation of privilege**: NO auto-apply (manual gate); CODEOWNERS + dual-approval for remediation.

**LINDDUN delta**:
- **Linkability**: drift findings + admin remediator em audit (compliance accountability).
- **Identifiability**: admin user_id em D1 + audit (intentional CTRL-AUDIT-002).
- **Non-repudiation**: cripto property intentional.
- **Detectability**: drift publicly tracked em Slack channel + D1.
- **Disclosure**: terraform plan output filtered for secrets.
- **Unawareness**: admin onboarding documents RB-FM-206.
- **Non-compliance**: SOC 2 CC8.1 + ISO 27001 A.5.18 satisfied.

## 27. Knowledge Transfer

- RB-FM-206 runbook self-documents.
- Doc `docs/internal/admin-plane.md` (terraform drift section).
- Workshop interno (1h) com SRE Lead + AppSec pós-merge.

## 28. Risk Register (6-col)

| ID | Risco | Prob | Det | Impacto | Exposure | Residual | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | Auto-apply incident | L | H | CRITICAL | M | LOW | NO auto-apply; CI lint forbids apply step; CODEOWNERS |
| R-002 | Drift detection bypass (cron skip) | M | M | HIGH | M | LOW | Workflow status check + alert SEV-3 if > 24h |
| R-003 | Slack alert noise (false-positives) | M | M | LOW | L | LOW | Acceptable drift patterns ADR + lifecycle.ignore_changes |
| R-004 | Drift > 7d sem remediation | L | M | HIGH | M | LOW | Post-mortem trigger spec contract §18 |
| R-005 | Manual remediation creates new drift | M | L | MEDIUM | L | LOW | RB-FM-206 decision tree mandates terraform apply |
| R-006 | State file corruption | L | M | HIGH | L | LOW | Backend versioning + integrity check + restore procedure |
| R-007 | CI credential exfiltration | L | H | CRITICAL | L | LOW | OIDC short-lived only; no long-lived secrets |
| R-008 | Cross-region drift correlation missed | M | L | MEDIUM | L | LOW | Aggregator step posts summary if multi-region |
| R-009 | Slack outage | L | L | LOW | L | LOW | D1 audit log persistent; PagerDuty fallback for SEV-2+ |
| R-010 | Cost regression em CI | L | L | LOW | L | LOW | Cost gate + matrix bounded |

## 29. Review Checkpoints

1. **Design (D+0)**: SRE Lead + Architect review workflow shape + matrix.
2. **Code (D+1)**: peer review.
3. **Security (D+1)**: AppSec review CI credential security + auto-apply forbidden lint.
4. **Adversarial (pre-merge D+2)**: red team session — synthetic drift + cron skip + auto-apply attempt.
5. **PRR mini (D+2)**: SRE Lead + Architect + AppSec sign-off.

## 30. Sign-off (HIGH_RISK 11 canonical)

| # | Role | Name | Signed Date | Status |
|---|---|---|---|---|
| 1 | Owner | Gustavo Schneiter | _pending_ | _pending_ |
| 2 | Final Approver | Gustavo Schneiter | _pending_ | _pending_ |
| 3 | Architect | _TBD; emphatic — terraform module composition + state file integrity_ | _pending_ | _pending_ |
| 4 | Security Lead | _TBD; emphatic — auto-apply forbidden enforcement + CI credential security_ | _pending_ | _pending_ |
| 5 | SRE Lead | _TBD; emphatic — cron + remediation cadence + RB-FM-206 dry-run_ | _pending_ | _pending_ |
| 6 | Engineer (S-13 lead) | _TBD_ | _pending_ | _pending_ |
| 7 | QA Lead | _TBD_ | _pending_ | _pending_ |
| 8 | Product | Gustavo Schneiter | _pending_ | _pending_ |
| 9 | Compliance Officer | _TBD; SOC 2 CC8.1 evidence pack_ | _pending_ | _pending_ |
| 10 | Privacy Officer | _TBD; LGPD Art. 38 review_ | _pending_ | _pending_ |
| 11 | AppSec advisor | _TBD; emphatic — CI credential security + workflow yaml threat model + auto-apply forbidden lint_ | _pending_ | _pending_ |

> Crypto SME folds into Architect role specialization (per framework §33.5.4.3 + ADR-0034). Peer reviewers contribuem em PR review sem sign-off canonical separado (folded into Engineer + Architect).

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-28 | Gustavo (via Claude Opus 4.7) | Criação WI-S13-004 (cycle 12.S13.0). |

## 32. Anti-patterns evitados

- Auto-apply via CI (catastrophic risk class).
- Skip cron daily cadence.
- Skip D1 audit log.
- Skip RB-FM-206 manual remediation runbook.
- Long-lived AWS/CF secrets em CI (OIDC only).
- Skip CODEOWNERS for `infra/terraform/`.
- Skip required PR reviews for terraform changes.
- Bash automation > 30 lines.
- Silent skip of cron failures.
- Acceptable drift unbounded.
- Direct D1 INSERT bypass.

---

**Fim WI-S13-004.** Próximo: WI-S13-005 (progressive rollout controller + auto-rollback + error-budget burn integration).
