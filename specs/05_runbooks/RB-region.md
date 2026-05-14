---
id: "RB-region"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "s14", "region", "terraform", "provisioning", "migration", "rollback", "chaos"]
---

# RB-region — Region Provisioning, Migration, Rollback, and Outage Response

> **WI-S14-001** — 4 regions (WNAM/ENAM/WEUR/SAM) Terraform module + migration + chaos.
> **Lane:** HIGH_RISK · **Dry-run status:** COMPLETED 2026-05-14

---

## 1. Overview

This runbook covers:

1. **§2 Provisioning** — Terraform apply 4 regions procedure.
2. **§3 DO jurisdiction verification** — WEUR EU jurisdiction validation (Schrems II).
3. **§4 Migration** — Single-region → multi-region tenant migration playbook.
4. **§5 Rollback** — Terraform state revert + D1 PITR + R2 backup restore.
5. **§6 Drift detection** — R2 location + D1 location + DO jurisdiction daily checks.
6. **§7 Region outage response** — On-call incident response procedure.
7. **§8 Quarterly config audit** — Compliance audit checklist.
8. **§9 Cost tracking** — Monthly budget review.

**CRITICAL:** Auto-apply is FORBIDDEN. All Terraform changes require PR + CODEOWNERS review + manual apply workflow + dual-approval. See `RB-FM-206`.

---

## 2. Provisioning Procedure

### 2.1 Pre-conditions

- [ ] Cloudflare account has R2 + D1 + DO + Workers + Custom Domains enabled.
- [ ] Terraform 1.7+ installed. Provider: `cloudflare ~> 4.52.0`.
- [ ] `CF_API_TOKEN` set (OIDC-bound; scoped to R2+D1+Workers+DNS+KV write).
- [ ] `CF_ACCOUNT_ID` set.
- [ ] `CF_ZONE_ID` set (zone for `api.corelink.dev`).
- [ ] Staging environment isolated (NOT production).
- [ ] PR approved by CODEOWNERS (Architect + Security Lead).
- [ ] Dual-approval received (WI-S13-002).

### 2.2 Plan (mandatory before apply)

```bash
cd infra/terraform/regions

# Initialize with per-region backend configs
for region in wnam enam weur sam; do
  terraform init \
    -backend-config="backend-${region}.hcl" \
    -reconfigure
  terraform plan \
    -var="cf_account_id=$CF_ACCOUNT_ID" \
    -var="cf_zone_id=$CF_ZONE_ID" \
    -var="environment=staging" \
    -out="plan-${region}.tfplan" \
    2>&1 | tee "plan-${region}.log"
done
```

Review all 4 plan outputs. Confirm:
- 4 R2 buckets: `corelink-cas-{region}`
- 4 D1 databases: `corelink-meta-{region}`
- 4 KV namespaces: `corelink-session-{region}`
- 4 DO Workers: `corelink-do-{region}`
- 4 DNS records: `{region}.api.corelink.dev`
- WEUR: `do_jurisdiction = "eu"` confirmed in plan

### 2.3 Apply (manual; dual-approval required)

```bash
# Apply one region at a time; verify before proceeding
for region in wnam enam weur sam; do
  echo "=== Applying ${region} (confirm: y/n) ==="
  read -r CONFIRM
  if [[ "$CONFIRM" == "y" ]]; then
    terraform apply "plan-${region}.tfplan"
    echo "=== Verifying ${region} ==="
    bash scripts/verify_r2_location.sh --region "$region"
    bash scripts/verify_d1_location.sh --region "$region"
    bash scripts/verify_do_jurisdiction.sh --region "$region"
  fi
done
```

### 2.4 Post-apply verification

```bash
# Verify all 4 regions
bash scripts/verify_r2_location.sh
bash scripts/verify_d1_location.sh
bash scripts/verify_do_jurisdiction.sh   # Exit 2 = CRITICAL for WEUR
```

Expected: all 3 scripts exit 0.

---

## 3. DO Jurisdiction Verification (WEUR Critical)

WEUR DO `jurisdictional_restriction = "eu"` MUST be set post-deploy.

Since Terraform `cloudflare_workers_script` 4.x does not expose `jurisdictional_restriction` directly, this must be set via Cloudflare Dashboard or CF API after Worker deployment:

```bash
# Set WEUR DO jurisdiction via CF API
curl -s -X PUT \
  "https://api.cloudflare.com/client/v4/accounts/${CF_ACCOUNT_ID}/workers/scripts/corelink-do-weur/settings" \
  -H "Authorization: Bearer ${CF_API_TOKEN}" \
  -H "Content-Type: application/json" \
  -d '{"jurisdictional_restriction": {"label": "eu"}}'

# Verify
bash scripts/verify_do_jurisdiction.sh --region weur
# Expected: exit 0, "WEUR EU jurisdiction confirmed — Schrems II compliant"
```

**HALT criteria:** If `verify_do_jurisdiction.sh` exits 2 for WEUR → DO NOT proceed with migration. Escalate to Compliance Officer immediately.

---

## 4. Migration Playbook (Single-Region → Multi-Region)

### 4.1 Pre-conditions

- [ ] All 4 regions provisioned and verified (§2).
- [ ] Dry-run completed and report reviewed.
- [ ] Admin role + dual-approval + WebAuthn UV=1 step-up obtained (S-13).
- [ ] D1 PITR backup confirmed (pre-migration snapshot).
- [ ] R2 versioning enabled on source buckets.

### 4.2 Dry-run (mandatory)

```bash
./target/debug/migrate-single-to-multi-region \
  --dry-run \
  --target-region weur \
  --tenant-id-filter "eu_"

# Review output:
# - Tenants to migrate count
# - D1 row count estimate
# - R2 blob count estimate
# - Estimated duration
# Commit dry-run report to specs/_audits/
```

### 4.3 Execute (post dry-run review approval)

```bash
./target/debug/migrate-single-to-multi-region \
  --execute \
  --target-region weur \
  --tenant-id-filter "eu_"

# Monitor:
# - corelink_region_migration_progress_ratio gauge
# - Per-tenant audit records in D1 region_migration_progress
```

### 4.4 Verify post-migration

```bash
# Verify migration completed
# 1. Check D1 region_migration_progress: all tenants state='completed'
# 2. Check R2 blobs present in corelink-cas-weur
# 3. Run WI-S14-002 insert checks (cross-region reject test)
# 4. Check audit log: corelink.region.migration.tenant_completed per tenant
```

---

## 5. Rollback Procedure

**RTO: ≤ 4h** (per WI-S14-001 §3 SLA addendum).

### 5.1 Trigger conditions

- Migration error_count > 5% of tenants.
- Hash verification failure on any tenant.
- Operator abort (manual decision).

### 5.2 Rollback steps

```bash
# Step 1: Signal rollback via migration script
./target/debug/migrate-single-to-multi-region --rollback --target-region weur

# Step 2: Terraform state revert
cd infra/terraform/regions
terraform init -backend-config="backend-weur.hcl"
terraform state pull > rollback-$(date +%Y%m%d-%H%M%S).tfstate.backup

# Identify and remove migrated resources (if newly added)
# terraform state rm module.weur.cloudflare_r2_bucket.corelink_cas  (if added erroneously)

# Step 3: D1 PITR restore
# CF Dashboard → D1 → corelink-meta-weur → Point-in-time Recovery
# Restore to snapshot timestamped before migration_run_id started

# Step 4: R2 backup restore
# CF Dashboard → R2 → corelink-cas-weur → Versioning
# List version IDs → restore objects to pre-migration version

# Step 5: Verify rollback complete
bash scripts/verify_r2_location.sh --region weur
bash scripts/verify_d1_location.sh --region weur
bash scripts/verify_do_jurisdiction.sh --region weur
```

---

## 6. Drift Detection Procedures

### 6.1 R2 location drift

```bash
# Daily via GH Actions terraform-drift.yml (auto-runs 03:00 UTC)
# Manual trigger:
bash scripts/verify_r2_location.sh

# On failure: SEV-2 alert + check WI-S14-002 insert check logs
```

### 6.2 D1 location drift

```bash
# Daily manual check (integrate into drift workflow WI-S14-002):
bash scripts/verify_d1_location.sh

# On failure: SEV-2 alert + capacity planning review
```

### 6.3 DO jurisdiction drift

```bash
# Daily check (integrate into drift workflow):
bash scripts/verify_do_jurisdiction.sh

# WEUR exit 2: CRITICAL — halt operations + Compliance Officer
# Other exit 1: SEV-2 + investigate
```

---

## 7. Region Outage Incident Response

### 7.1 Detection

- Metric: `corelink_region_health_status{region}` gauge drops to 0 or 1.
- Alert: `corelink_region_outage_events_total{region, event_type="detected"}` counter increment.
- PagerDuty: SEV-2 page to on-call.

### 7.2 Response steps

```bash
# 1. Acknowledge PagerDuty alert
# 2. Check region health dashboard (DASH-REGION)
# 3. Verify scope: which tenants affected (check audit log corelink.region.outage.detected)
# 4. Confirm failover routing engaged (WI-S14-003 PAT-REGION-FAILOVER-001)
#    - WEUR outage: confirm EU data does NOT cross to ENAM (Schrems II)
# 5. Communicate status to affected tenants
# 6. Monitor recovery: health_status back to 2 = Healthy
# 7. Post-recovery: run verify scripts
# 8. If outage > 30 min: open SEV-2 post-mortem
```

### 7.3 WEUR-specific outage (Schrems II)

WEUR outage requires additional steps:

- Confirm DO `jurisdictional_restriction = "eu"` prevents EU data routing to US.
- Do NOT enable cross-region failover for EU data to ENAM/WNAM (violates Schrems II).
- Contact Compliance Officer if data path unclear.
- Refer to WI-S14-009 TLA+ `region_residency.tla` for formal verification.

---

## 8. Quarterly Config Audit Checklist

Frequency: Quarterly (every 3 months).

```
[ ] R2 bucket locations: run verify_r2_location.sh for all 4 regions
[ ] D1 database locations: run verify_d1_location.sh for all 4 regions
[ ] DO jurisdiction: run verify_do_jurisdiction.sh — WEUR MUST exit 0
[ ] KV namespace titles: confirm corelink-session-{region} per region (4 namespaces)
[ ] Terraform state integrity: run terraform plan; confirm exit 0 for all regions
[ ] Terraform state backup: confirm R2 versioning + 90d retention active
[ ] Cost review: R2 + D1 + DO + KV total ≤ $800/month (WI-S14-001 §11 gate)
[ ] DO jurisdiction attestation: Compliance Officer sign-off (WEUR)
[ ] Schrems II TIA review: confirm no legal landscape changes (WI-S14-008)
[ ] Commit audit report to specs/_audits/YYYY-QN-region-config-audit.md
```

---

## 9. Cost Tracking

Monthly budget target: ≤ $800/month for 4 regions (WI-S14-001 §22 + §11).

| Resource | Unit Cost | 4 Regions | Monthly |
|---|---|---|---|
| R2 buckets | ~$5/month | × 4 | $20 |
| D1 instances | ~$10/month | × 4 | $40 |
| DO Workers | ~$15/month | × 4 | $60 |
| KV namespaces | ~$5/month | × 4 | $20 |
| **Baseline** | | | **$140** |
| Workload-dependent | | | ~$660 |
| **Total cap** | | | **$800** |

Review monthly via `check_cost_regression.py`. Trigger: alert if > $700/month.

---

## DRY-RUN RECORD

| Date | Executor | Steps verified | Result |
|---|---|---|---|
| 2026-05-14 | Gustavo Schneiter (WI-S14-001 builder) | §2 plan structure verified; §3 jurisdiction logic; §4 migration script dry-run output; §5 rollback commands; §6 verify scripts exit codes; §7 outage response logic; §8 checklist items | PASS (structural dry-run; staging apply deferred to staging environment with live CF credentials) |

---

**Fim RB-region v1.0.0.** Próximo: WI-S14-002 (tenant region pinning + insert checks).
