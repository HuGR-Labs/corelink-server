---
id: "RB-COLD-RESTORE-FROM-ZERO"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "dr", "cold-restore", "gap-15", "soc2-a1-2", "soc2-a1-3", "rto-4h-read", "rto-8h-write", "rpo-15m", "dr-15"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §3.3, §10.

# RB-COLD-RESTORE-FROM-ZERO — Region Lost; Restore From N-1

> **Status:** ACTIVE. Companion to `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` (drill spec) and `scripts/cold-restore-drill.sh` (orchestrator).
>
> **Purpose:** procedural runbook for the worst-case Cloudflare DR scenario — a full region (R2 + D1 + KV + DO) is destroyed and we must rebuild from cross-region encrypted backups. Used both for the quarterly DR-15 drill and for real-incident response (with mode switches noted per step).
>
> **RTO targets:** read-path ≤ 4h; full write-path ≤ 8h.
> **RPO target:** ≤ 15 minutes (last cross-region replication checkpoint).
>
> **DO NOT** invoke any destructive command without `CONFIRM_PROD=I_UNDERSTAND` env if `CORELINK_ENV=production`. See `scripts/cold-restore-drill.sh` for guard implementation.

## 1. Trigger conditions

This runbook is invoked **only** when all three hold:

1. Cloudflare regional control plane returns hard-error (`region_destroyed: true` or CF support confirms ≥ 4h outage with no ETA), **AND**
2. Residency-graph router (`crates/corelink-failover-router`) has exhausted sibling-bucket routes in the same residency zone, **AND**
3. Warm cross-region failover (per `RB-region.md` §5) is not possible because the failed region was source-of-truth for affected residency partition.

For partial outages, see `RB-region.md` (warm failover) first; this runbook is the **last-resort** path.

## 2. Role assignments

| Role | Tier | Responsibility | Time commitment |
|---|---|---|---|
| Incident Commander (IC) | L2 (Eng Manager) | Sequences steps; owns timeline; calls go/no-go at step 9 | Full drill window (8h) |
| Scribe | L1 (SRE on-call) | Timestamps every action; updates `#drill-cold-restore-*` channel | Full drill window |
| Restore Operator | L1 (SRE on-call) | Executes wrangler / terraform / rclone commands | Steps 3–7 |
| BYOK Operator | L2 (Security on-call) | Validates KMS access; unwraps test envelopes | Steps 4, 7, 8 |
| Customer Comms | L1 (CSM) | Drafts customer comms; manages status page | Steps 1, 9 |
| Compliance Reviewer | L2 (Compliance Officer) | Validates audit-chain continuity; signs evidence doc | Steps 8, 9 + post-drill |
| CTO (L3) | L3 | Decision-maker on go-live; approves customer-facing comms | Step 9 only |

## 3. Pre-step bootstrap (T+0)

Before step 1, confirm:

- [ ] War-room channel `#drill-cold-restore-YYYY-MM-DD` (or `#incident-XXX` for real) is live.
- [ ] PagerDuty incident opened with severity = SEV-1 (drill or real).
- [ ] Conference bridge dialled by IC + Scribe + Restore Operator.
- [ ] Drill orchestrator started in correct mode: `bash scripts/cold-restore-drill.sh --staging` (or `--prod` for real).

The orchestrator writes a structured log to `drill-cold-restore-YYYY-MM-DD-HH-MM.log` — every step below references log markers (e.g. `LOG_STEP=3_PROVISION`).

---

## Step 1 — Establish IC + war room (T+0 to T+15min)

### 1.1 Prerequisites

- [ ] PD incident exists with `severity=SEV-1` and `drill={true|false}` field set.
- [ ] At least 2 SREs + 1 Eng Manager acknowledged page within 5min.

### 1.2 Commands

```bash
# 1. Drop a marker in the audit chain so post-drill diff is unambiguous.
wrangler d1 execute corelink_audit --remote \
    --command "INSERT INTO audit_chain_checkpoints (epoch, marker, ts) VALUES ('drill-pre', 'cold-restore-start', $(date -u +%s));"

# 2. Snapshot baseline metrics.
curl -sS -H "Authorization: Bearer ${GRAFANA_API_TOKEN}" \
    "https://grafana.corelink.io/render/d/rto-rpo/dashboard?from=now-24h&to=now" \
    > "drill-baseline-${TS}.png"
```

### 1.3 Success criteria

- IC role declared explicitly in war-room channel: "I am IC for cold-restore drill DR-15, scribe is <name>."
- PD timeline shows L1 ack within 5min of page.
- Baseline screenshot saved to `drill-baseline-${TS}.png`.

### 1.4 Rollback / abort

If IC role cannot be filled within 15min → escalate to L3 (CTO) page-up and **abort drill**; document in evidence doc with `status: aborted`. Real incidents do not abort; they escalate.

### 1.5 Log marker

`LOG_STEP=1_IC_ESTABLISHED`

---

## Step 2 — Inventory damage (T+15 to T+30min)

### 2.1 Prerequisites

- [ ] Step 1 complete.
- [ ] CF status page + internal residency-graph router metrics accessible.

### 2.2 Commands

For each binding (R2 / D1 / KV / DO), probe whether it is:

- **Read-only degraded** (200 reads OK but writes 503) — warm-failover candidate, NOT cold restore.
- **Hard-destroyed** (404 on bucket / database / namespace lookup) — cold restore confirmed.

```bash
# R2
wrangler r2 bucket list --remote 2>&1 | grep "corelink-cold-${LOST_REGION}" \
    || echo "R2 HARD DESTROYED: bucket absent"

# D1
for db in corelink_core corelink_audit corelink_billing; do
    wrangler d1 info "${db}" --remote 2>&1 \
        || echo "D1 HARD DESTROYED: ${db}"
done

# KV
wrangler kv namespace list --remote 2>&1 | grep "CORELINK_KV_${LOST_REGION}" \
    || echo "KV HARD DESTROYED: namespace absent"

# DO (probe via residency-graph router; if all colos in region return null, DO state is gone)
curl -sS "https://router.corelink.io/v1/residency/${LOST_REGION}/colos" \
    | jq -e '.colos | length > 0' >/dev/null \
    || echo "DO HARD DESTROYED: all colos null"
```

### 2.3 Success criteria

- IC has a binding-by-binding YES/NO destroyed-flag table; logged in war-room channel.
- If **any** binding is recoverable in-region (read-only or partial), STOP and switch to `RB-region.md` warm failover; cold restore is overkill.

### 2.4 Rollback / abort

If inventory is ambiguous (e.g. CF API itself flaky), wait 15min, re-probe. If still ambiguous after 30min, **proceed with cold restore** (false-positive cold-restore is recoverable; false-negative wastes hours).

### 2.5 Log marker

`LOG_STEP=2_INVENTORY_DONE`

---

## Step 3 — Provision new region (T+30 to T+90min)

### 3.1 Prerequisites

- [ ] Step 2 confirms hard destruction across all four bindings.
- [ ] Surviving region's Terraform state file accessible (S3 backend with versioning).
- [ ] CF API token with `Account:Workers:Edit`, `Account:R2:Edit`, `Account:D1:Edit`, `Account:KV:Edit` scopes.

### 3.2 Commands — Terraform path (preferred)

```bash
# 1. Cut a new Terraform workspace pinned to the same residency tag.
cd infra/terraform/cloudflare
terraform workspace new "${LOST_REGION}-restore-${TS}"
terraform workspace select "${LOST_REGION}-restore-${TS}"

# 2. Apply the region module with NEW colo allocation.
terraform apply \
    -var "region=${LOST_REGION}" \
    -var "residency_tag=${RESIDENCY_TAG}" \
    -var "colo_allocation=fallback" \
    -auto-approve
```

### 3.3 Commands — Manual fallback (if Terraform state is also lost)

```bash
# R2 bucket
wrangler r2 bucket create "corelink-cold-${LOST_REGION}" \
    --location="${RESIDENCY_TAG}" \
    --remote

# D1 databases
for db in corelink_core corelink_audit corelink_billing; do
    wrangler d1 create "${db}" --location="${RESIDENCY_TAG}"
done

# KV namespaces
for ns in CORELINK_KV CORELINK_FEATURE_FLAGS; do
    wrangler kv namespace create "${ns}_${LOST_REGION}" --remote
done

# Apply latest D1 migration set (creates schema; data restore is step 5)
bash scripts/migrate_d1.sh --env "${CORELINK_ENV}-restore"
```

### 3.4 Success criteria

- All four bindings exist in the new region (verify with `wrangler {r2,d1,kv} list` + colo probe).
- Schema migrations applied (D1 tables exist; empty).
- Residency tag preserved (verify: `wrangler r2 bucket get corelink-cold-${LOST_REGION} | jq .location` equals the prior region's `RESIDENCY_TAG`).

### 3.5 Rollback / abort

If Terraform fails AND manual fallback fails (both CF API outage OR account-level lockout): page L3 (CTO) + CF enterprise support + **abort drill** (real incident: escalate to CF support + customer-comms major-outage template).

### 3.6 Log marker

`LOG_STEP=3_PROVISION_DONE`

---

## Step 4 — Restore R2 from cross-region backup (T+90 to T+180min)

### 4.1 Prerequisites

- [ ] Step 3 complete.
- [ ] `BACKUP_GPG_RECIPIENT` private key present on operator workstation.
- [ ] `rclone` configured with `corelink-r2-backup:` remote pointing to surviving-region backup bucket.

### 4.2 Commands

Reuse the existing primitive — DO NOT reimplement.

```bash
# Pick the freshest manifest from the surviving-region backup bucket.
SNAPSHOT_DATE=$(wrangler r2 object list "corelink-backups-${SURVIVING_ENV}" --remote \
    | jq -r '[.objects[] | select(.key | endswith("manifest.json"))] | sort_by(.uploaded) | last | .key' \
    | cut -d/ -f1)

echo "Selected snapshot: ${SNAPSHOT_DATE}"

# Compute RPO: how stale is this snapshot?
SNAPSHOT_AGE=$((($(date -u +%s) - $(date -u -d "${SNAPSHOT_DATE}" +%s)) / 60))
echo "RPO measurement: ${SNAPSHOT_AGE} minutes stale"
[ "${SNAPSHOT_AGE}" -le 15 ] || echo "WARN: RPO target breached: ${SNAPSHOT_AGE} > 15 min"

# Execute restore — phases 1, 3 of the existing script.
./scripts/restore-from-snapshot.sh \
    --snapshot "${SNAPSHOT_DATE}" \
    --env "${CORELINK_ENV}-restore" \
    --target-region "${LOST_REGION}" \
    --skip-byok-check  # BYOK validation deferred to step 7
```

### 4.3 Verification

```bash
# Spot-check a deterministic CAS blob from the synthetic drill tenant fixture.
EXPECTED_SHA=$(jq -r '.cas_blobs[0].sha256' \
    specs/_compliance/drill-evidence/fixtures/cold-restore-tenant-pre-drill.json)
ACTUAL_SHA=$(wrangler r2 object get \
    "corelink-cold-${LOST_REGION}/cas/${EXPECTED_SHA}" \
    --remote --pipe 2>/dev/null \
    | shasum -a 256 | awk '{print $1}')
[ "${EXPECTED_SHA}" = "${ACTUAL_SHA}" ] || { echo "R2 restore SHA mismatch"; exit 1; }
```

### 4.4 Success criteria

- `restore-from-snapshot.sh` exits 0.
- Spot-check CAS blob SHA-256 matches pre-drill fixture.
- R2 object count in new region ≥ 99% of snapshot's manifest count.

### 4.5 Rollback / abort

If `rclone copy` fails on > 1% of objects: retry with `--retries 5`. If still failing: pick previous-day snapshot (RPO degrades to 24h+15m); log RPO breach in evidence doc.

### 4.6 Log marker

`LOG_STEP=4_R2_RESTORED`

---

## Step 5 — Restore D1 from snapshot (T+180 to T+240min, parallel with step 6)

### 5.1 Prerequisites

- [ ] Step 3 complete (D1 schema exists, empty).
- [ ] Manifest from step 4 references D1 artifacts for `corelink_core`, `corelink_audit`, `corelink_billing`.

### 5.2 Commands

D1 phase of `restore-from-snapshot.sh` already executed in step 4 (the script does R2 + D1 + KV in one shot). This step **verifies** D1-specific invariants.

```bash
# Last-write-wins check: audit-chain rows must be in monotonic order.
wrangler d1 execute corelink_audit --remote \
    --command "SELECT MAX(seq) AS max_seq, COUNT(*) AS total FROM audit_events;" --json \
    | jq -e '.[0].results[0].max_seq == (.[0].results[0].total - 1)' \
    || echo "WARN: audit_events seq has gaps (post-restore)"

# Tenant count parity vs manifest.
EXPECTED_TENANTS=$(jq -r '.tenant_count_at_snapshot' \
    "drill-workspace/${SNAPSHOT_DATE}/manifest.json")
ACTUAL_TENANTS=$(wrangler d1 execute corelink_core --remote \
    --command "SELECT COUNT(*) AS c FROM tenants;" --json | jq -r '.[0].results[0].c')
[ "${EXPECTED_TENANTS}" = "${ACTUAL_TENANTS}" ] || \
    echo "WARN: tenant count mismatch: expected=${EXPECTED_TENANTS} actual=${ACTUAL_TENANTS}"

# SQL replay of any deltas since snapshot (D1 export → SQL → replay).
# For cold-restore from N-1 region backup, NO replay needed because the
# region is destroyed and there are no in-flight transactions to replay.
# This branch is reserved for future synchronous-replication mode.
```

### 5.3 Success criteria

- Audit-event seq column is dense (no gaps) within the snapshot's epoch.
- Tenant count matches manifest.
- D1 query latency P99 < 100ms (region is healthy).

### 5.4 Rollback / abort

If seq has gaps: file SEV-2 ticket but proceed — gaps reflect snapshot edge; the audit-chain Merkle root in step 8 is the canonical integrity check.

### 5.5 Log marker

`LOG_STEP=5_D1_VERIFIED`

---

## Step 6 — Restore KV from export (T+180 to T+240min, parallel with step 5)

### 6.1 Prerequisites

- [ ] Step 3 complete (KV namespaces exist, empty).
- [ ] KV phase of `restore-from-snapshot.sh` completed in step 4.

### 6.2 Commands

```bash
# tenant_id integrity check: every KV key must parse against the tenant_id prefix scheme.
wrangler kv key list --binding CORELINK_KV --remote \
    | jq -r '.[].name' \
    | awk -F: '
        $1 != "t" || length($2) != 26 {
            print "BAD KEY: " $0;
            bad++
        }
        END { exit (bad > 0 ? 1 : 0) }
    ' || echo "WARN: KV key scheme violation detected"

# Feature flag namespace sanity.
wrangler kv key list --binding CORELINK_FEATURE_FLAGS --remote \
    | jq -e 'length > 0' \
    || echo "WARN: CORELINK_FEATURE_FLAGS empty post-restore"
```

### 6.3 Success criteria

- All KV keys conform to `t:<tenant-ulid>:...` prefix scheme.
- Feature-flag namespace non-empty.

### 6.4 Rollback / abort

If KV key scheme violation found: file SEV-2 + investigate manifest integrity; proceed to step 7 (KV violations do not block read-path).

### 6.5 Log marker

`LOG_STEP=6_KV_VERIFIED`

---

## Step 7 — DO state recovery (T+240 to T+360min)

### 7.1 Prerequisites

- [ ] Steps 4, 5, 6 complete.
- [ ] BYOK Operator on-call; KMS endpoint reachable.

### 7.2 Commands

Durable Objects do not have native backups — their state lives in the DO's own storage. Recovery model is **state hydration from D1+R2** since the canonical truth for tenant cache lives in D1 (`tenants.cache_state_blob`).

```bash
# 7.1 Recreate DO instances for each tenant in the synthetic-drill set.
# Tenant rows now exist in restored D1; DO actors are recreated lazily on first access.
# Force-warm by issuing a synthetic read per tenant:
jq -r '.tenants[].tenant_id' \
    specs/_compliance/drill-evidence/fixtures/cold-restore-tenant-pre-drill.json \
    | while read -r tid; do
        curl -sS "https://api.corelink.io/v1/tenants/${tid}/cache/warm" \
            -H "Authorization: Bearer ${DRILL_TENANT_PAT}" \
            --max-time 30
    done

# 7.2 BYOK envelope re-binding: unwrap synthetic tenant's pre-drill DEK.
# Reuses corelink-byok crate's unwrap path.
EXPECTED_DEK_SHA=$(jq -r '.byok_envelope.dek_sha256_for_verification' \
    specs/_compliance/drill-evidence/fixtures/cold-restore-tenant-pre-drill.json)
ACTUAL_DEK_SHA=$(curl -sS "https://api.corelink.io/internal/byok/unwrap-test" \
    -H "Authorization: Bearer ${BYOK_OPERATOR_PAT}" \
    -d "{\"tenant_id\":\"cold-restore-drill-tenant\"}" \
    | jq -r '.dek_sha256')
[ "${EXPECTED_DEK_SHA}" = "${ACTUAL_DEK_SHA}" ] || { echo "BYOK envelope re-bind FAILED"; exit 1; }
```

### 7.3 Success criteria

- All synthetic-drill tenants' DOs warm-loaded without 5xx.
- BYOK envelope unwrap returns identical DEK SHA-256.
- `byok_envelope_unwrap_latency_p99` < 5s.

### 7.4 Rollback / abort

If BYOK envelope unwrap fails: this is a **terminal cold-restore failure** — the data is technically restored but cryptographically inaccessible. Page L3 (CTO) + Security Lead immediately. Likely root cause: KMS CMK rotation between snapshot date and restore date (the overlap-period decryption pattern PAT-ROLL-FORWARD-001 should cover, but verify).

### 7.5 Log marker

`LOG_STEP=7_DO_BYOK_HYDRATED`

---

## Step 8 — Smoke tests (T+360 to T+420min)

### 8.1 Prerequisites

- [ ] Steps 4–7 complete.
- [ ] `verify-cold-restore.py` available + executable.

### 8.2 Commands

```bash
# Run the verification gate (full pass/fail).
python3 scripts/verify-cold-restore.py \
    --env "${CORELINK_ENV}-restore" \
    --fixture specs/_compliance/drill-evidence/fixtures/cold-restore-tenant-pre-drill.json \
    --merkle-root-expected "${PRE_DRILL_MERKLE_ROOT}" \
    --output drill-verification-${TS}.json

# Synthetic page drill — page on-call against new region to validate alerting wired up.
bash specs/_runbooks/RB-SYNTHETIC-PAGE-DRILL.md  # follow procedure

# 3-tenant E2E: pick 3 tenants from fixture; read + write + audit-chain append.
for tid in $(jq -r '.tenants[].tenant_id' \
        specs/_compliance/drill-evidence/fixtures/cold-restore-tenant-pre-drill.json \
        | head -3); do
    # Read
    curl -sS "https://api.corelink.io/v1/tenants/${tid}/cas/${KNOWN_CAS_HASH}" \
        -H "Authorization: Bearer ${DRILL_TENANT_PAT}" --fail
    # Write (4 KiB blob)
    dd if=/dev/urandom bs=4096 count=1 2>/dev/null \
        | curl -sS -X POST "https://api.corelink.io/v1/tenants/${tid}/cas" \
            -H "Authorization: Bearer ${DRILL_TENANT_PAT}" \
            --data-binary @- --fail
done
```

### 8.3 Success criteria

- `verify-cold-restore.py` exits 0.
- 3-tenant E2E all 200s; latency P99 < 500ms.
- Synthetic page acknowledged within 5min.

### 8.4 Rollback / abort

If verify-cold-restore.py reports any RED criterion: do NOT proceed to step 9 (go-live). Drill outcome = FAIL; file SEV-2; root-cause within 14d.

### 8.5 Log marker

`LOG_STEP=8_SMOKE_PASS` (or `LOG_STEP=8_SMOKE_FAIL`)

---

## Step 9 — Customer comms + go-live decision (T+420 to T+480min)

### 9.1 Prerequisites

- [ ] Step 8 PASS (all verification gates green).
- [ ] CTO (L3) available for go/no-go.
- [ ] Customer Comms operator has draft email ready.

### 9.2 Commands (drill mode)

```bash
# Drill mode: stage customer comms but DO NOT SEND.
cat > drill-customer-comms-draft-${TS}.md <<'EOF'
Subject: Staging DR drill complete — no customer impact

We completed a planned disaster-recovery drill at <DATE> UTC validating
end-to-end cold restore of one Cloudflare region. All targets met:
- Read-path restored in <ACTUAL_RTO_READ_MIN> minutes (target: 240).
- Write-path restored in <ACTUAL_RTO_WRITE_MIN> minutes (target: 480).
- RPO measured: <ACTUAL_RPO_MIN> minutes (target: ≤ 15).

No production tenant traffic was affected.
EOF

# Upload drill log + evidence doc + verification output to Drata.
corelink-drata-sync upload \
    --stream incident_response \
    --evidence-id "cold-restore-drill-${TS}" \
    --files "drill-cold-restore-${TS}.log,drill-verification-${TS}.json,drill-customer-comms-draft-${TS}.md"
```

### 9.3 Commands (real-incident mode)

```bash
# Real mode: CTO approves customer comms; CSM sends.
# Customer email is real, not drafted.
# Status page updated to "incident-recovered" with full timeline.
# Post-incident retro scheduled within 5 business days.
```

### 9.4 Success criteria

- IC declares drill complete in war-room channel: "DR-15 cold-restore drill PASS; evidence sealed; resuming standard rotation."
- Drata upload returns 200.
- Evidence doc PR opened: `specs/_compliance/drill-evidence/YYYY-QQ-cold-restore-staging.md`.
- Admin-plane write freeze lifted on staging.

### 9.5 Rollback / abort

If at step 9 something unexpected breaks (e.g. drill tenant's E2E fails 1h post-step-8): IC has discretion to declare drill PARTIAL; do NOT seal as PASS; file AI.

### 9.6 Log marker

`LOG_STEP=9_DRILL_COMPLETE`

---

## 4. Post-drill (T+8h to T+24h)

- [ ] Evidence doc sealed: `doc_status: FROZEN` set within 24h.
- [ ] Drata `incident_response` stream upload confirmed (response 200; evidence ID logged).
- [ ] BCP-DR-DRILL-CADENCE.md DR-15 row updated with cycle letter (a/b/c/d).
- [ ] Compliance Officer sign-off + GAP-15 status update in SOC2-EVIDENCE-ROLLUP.
- [ ] Postmortem-lite if PARTIAL or FAIL outcome.

## 5. Failure-mode coverage

| FM-ID | Description | This runbook covers |
|---|---|---|
| FM-050 | R2 bucket outage | ✅ via step 4 restore |
| FM-051 | Backup corruption | ✅ via DR-13 prereq + step 4 SHA check |
| FM-052 | R2 cross-region routing fail | ✅ via step 3 sibling-bucket allocation |
| FM-055 | D1 primary loss | ✅ via step 5 |
| FM-061 | Audit-log retention loss | ✅ via step 5 Merkle root verification |
| FM-062 | Audit-chain gap | ✅ via step 8 verify-cold-restore.py |
| FM-101 | Cross-region failover | ✅ via step 3 + 4 |
| FM-105 | Region-wide outage | ✅ entire runbook |
| FM-202 | Runbook drift | this runbook IS the drift-resistant artifact (quarterly drill catches drift) |
| FM-204 | BYOK CMK access | ✅ via step 7 |

## 6. References

- `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` — parent drill spec.
- `scripts/cold-restore-drill.sh` — orchestrator script.
- `scripts/verify-cold-restore.py` — verification gate (step 8).
- `scripts/restore-from-snapshot.sh` — low-level restore primitive (step 4–6).
- `scripts/backup-daily.sh` — backup primitive (produces what we consume).
- `specs/_runbooks/RB-BACKUP-VERIFICATION.md` — monthly warm verification (prerequisite cadence).
- `specs/_runbooks/RB-region.md` — warm cross-region failover (alternative path for partial outages).
- `specs/_runbooks/RB-SYNTHETIC-PAGE-DRILL.md` — synthetic page procedure (step 8).
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — postmortem template (post-drill if PARTIAL/FAIL).
- `specs/_compliance/BCP-DR-DRILL-CADENCE.md` — DR-15 cadence entry.
- `specs/03_architecture/resilience_patterns.md` — PAT-REGION-FAILOVER-001, PAT-ROLL-FORWARD-001.
- `specs/03_architecture/failure_modes.md` — FM table.
- `crates/corelink-region` — colo + residency-graph.
- `crates/corelink-failover-router` — sibling-bucket route selection.
- `crates/corelink-cf-bindings` — R2 / D1 / KV / DO wrappers.
- `crates/corelink-byok` — envelope encryption.
- `crates/corelink-drata-sync` — evidence upload CLI.
- `specs/_runbooks/RB-GA-CUTOVER.md` §0.2.6 — GA cutover requires last full cold-restore drill ≤ 90d before T-0h.

---

**End RB-COLD-RESTORE-FROM-ZERO.**
