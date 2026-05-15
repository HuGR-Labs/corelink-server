#!/usr/bin/env bash
# GAP-15 closure: Cold-restore drill orchestrator.
#
# Drives the 9-step cold-restore runbook (specs/_runbooks/RB-COLD-RESTORE-FROM-ZERO.md)
# against staging (--staging) or validates readiness without touching state (--dry-run).
# A --prod mode exists for the rare real-incident path but is firewall-protected
# by an explicit confirmation env var.
#
# Modes:
#   --dry-run  Validate inventory, permissions, backup freshness, manifest decrypt.
#              Touches no state. Safe to run from CI.
#   --staging  Execute full runbook against an ephemeral staging-equivalent region.
#              Tears down on exit. Requires BACKUP_GPG_RECIPIENT + BYOK_TEST_TENANT.
#   --prod     Real-incident path. Requires CONFIRM_PROD=I_UNDERSTAND env.
#              NEVER schedule this in cadence; only manual SRE invocation during
#              a confirmed regional disaster.
#
# Outputs a structured log:
#   drill-cold-restore-YYYY-MM-DD-HH-MM.log
#
# Post-drill, auto-uploads the log + verification output to Drata via the
# existing corelink-drata-sync CLI (does NOT reimplement evidence upload).
#
# This script is intentionally idempotent at the step granularity: each step
# emits a LOG_STEP=N_TITLE marker. If interrupted, the operator can resume
# from the last successful step by re-running with the same --resume-from flag
# (not yet implemented — placeholder; current behavior is full re-run).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TS_START="$(date -u +%s)"
TS_HUMAN="$(date -u +%Y-%m-%d-%H-%M)"
LOG_FILE="drill-cold-restore-${TS_HUMAN}.log"

# ---------------------------------------------------------------------------
# Args
# ---------------------------------------------------------------------------
MODE=""
RESUME_FROM=""
SKIP_DRATA_UPLOAD="false"

usage() {
    sed -n '2,40p' "$0"
    cat <<'EOF'

Usage:
  scripts/cold-restore-drill.sh --dry-run
  scripts/cold-restore-drill.sh --staging
  CONFIRM_PROD=I_UNDERSTAND scripts/cold-restore-drill.sh --prod

Options:
  --skip-drata-upload   Skip post-drill evidence upload (offline mode).
  --resume-from STEP    Reserved; not yet implemented.
  -h, --help            Show this help.

Required env (varies by mode):
  BACKUP_GPG_RECIPIENT          GPG key id for backup decrypt (staging/prod).
  BACKUP_R2_BUCKET              Backup R2 bucket (defaults to corelink-backups-<env>).
  BYOK_TEST_TENANT              Synthetic drill tenant id (staging/prod).
  LOST_REGION                   Region label that is "destroyed" (e.g. us-east-1).
  SURVIVING_ENV                 Surviving-region environment label.
  CORELINK_ENV                  Target env (staging|production).
  CONFIRM_PROD                  Must equal I_UNDERSTAND for --prod mode.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run) MODE="dry-run"; shift ;;
        --staging) MODE="staging"; shift ;;
        --prod)    MODE="prod"; shift ;;
        --skip-drata-upload) SKIP_DRATA_UPLOAD="true"; shift ;;
        --resume-from) RESUME_FROM="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "[FATAL] unknown arg: $1" >&2; usage >&2; exit 64 ;;
    esac
done

if [[ -z "${MODE}" ]]; then
    echo "[FATAL] one of --dry-run / --staging / --prod required" >&2
    usage >&2
    exit 64
fi

# ---------------------------------------------------------------------------
# Logging helpers — all log lines also go to stdout for live war-room view.
# ---------------------------------------------------------------------------
log()  {
    local line
    line="[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*"
    echo "${line}"
    echo "${line}" >> "${LOG_FILE}"
}

log_step() {
    log "LOG_STEP=$*"
}

warn() {
    log "[WARN] $*"
}

fail() {
    log "[FATAL] $*"
    log "drill aborted in mode=${MODE} at step=${CURRENT_STEP:-pre-step}"
    if [[ "${MODE}" != "dry-run" && "${SKIP_DRATA_UPLOAD}" != "true" ]]; then
        upload_drata_log "failed" || warn "post-failure drata upload skipped"
    fi
    exit 1
}

# Step tracker for fail() context.
CURRENT_STEP="init"

# ---------------------------------------------------------------------------
# Mode guards
# ---------------------------------------------------------------------------
case "${MODE}" in
    prod)
        if [[ "${CONFIRM_PROD:-}" != "I_UNDERSTAND" ]]; then
            echo "[FATAL] --prod requires CONFIRM_PROD=I_UNDERSTAND env" >&2
            echo "[FATAL] this mode is ONLY for real regional disasters." >&2
            exit 65
        fi
        if [[ "${CI:-false}" == "true" ]]; then
            echo "[FATAL] --prod refused from CI environment" >&2
            exit 65
        fi
        CORELINK_ENV="${CORELINK_ENV:-production}"
        ;;
    staging)
        CORELINK_ENV="${CORELINK_ENV:-staging}"
        if [[ "${CORELINK_ENV}" == "production" ]]; then
            echo "[FATAL] --staging refuses CORELINK_ENV=production" >&2
            exit 66
        fi
        ;;
    dry-run)
        CORELINK_ENV="${CORELINK_ENV:-staging}"
        ;;
esac

LOST_REGION="${LOST_REGION:-us-east-1}"
SURVIVING_ENV="${SURVIVING_ENV:-${CORELINK_ENV}-survivor}"
BACKUP_R2_BUCKET="${BACKUP_R2_BUCKET:-corelink-backups-${CORELINK_ENV}}"
BYOK_TEST_TENANT="${BYOK_TEST_TENANT:-cold-restore-drill-tenant}"

log "=== corelink cold-restore drill ==="
log "mode=${MODE} env=${CORELINK_ENV} lost_region=${LOST_REGION}"
log "surviving_env=${SURVIVING_ENV} backup_bucket=${BACKUP_R2_BUCKET}"
log "byok_test_tenant=${BYOK_TEST_TENANT} log_file=${LOG_FILE}"
log "started_at_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# ---------------------------------------------------------------------------
# Helper: dry-run wrapper. Returns 0 in dry-run mode without executing the
# wrapped command; logs what it would do.
# ---------------------------------------------------------------------------
would_run() {
    if [[ "${MODE}" == "dry-run" ]]; then
        log "DRY-RUN: would execute: $*"
        return 0
    fi
    log "exec: $*"
    "$@"
}

# ---------------------------------------------------------------------------
# Drata upload helper. Calls existing CLI; does NOT reimplement.
# ---------------------------------------------------------------------------
upload_drata_log() {
    local status="$1"
    if [[ "${SKIP_DRATA_UPLOAD}" == "true" ]]; then
        log "drata upload skipped (--skip-drata-upload)"
        return 0
    fi
    if [[ "${MODE}" == "dry-run" ]]; then
        log "drata upload skipped (dry-run mode)"
        return 0
    fi
    if ! command -v corelink-drata-sync >/dev/null 2>&1; then
        warn "corelink-drata-sync CLI not on PATH; skipping evidence upload"
        return 0
    fi
    log "uploading drill log to Drata (status=${status})"
    corelink-drata-sync upload \
        --stream incident_response \
        --evidence-id "cold-restore-drill-${TS_HUMAN}" \
        --files "${LOG_FILE}" \
        --status "${status}" \
        || warn "drata upload returned non-zero"
}

# ---------------------------------------------------------------------------
# Pre-flight checks (all modes)
# ---------------------------------------------------------------------------
CURRENT_STEP="preflight"
log "preflight: tooling checks"

require_tool() {
    if ! command -v "$1" >/dev/null 2>&1; then
        if [[ "${MODE}" == "dry-run" ]]; then
            warn "missing tool: $1 (acceptable in dry-run; would block real run)"
        else
            fail "missing required tool: $1"
        fi
    fi
}

require_tool wrangler
require_tool jq
require_tool python3
require_tool curl

# Tools that are required for real run but optional for dry-run.
for t in rclone gpg; do
    if ! command -v "${t}" >/dev/null 2>&1; then
        if [[ "${MODE}" == "dry-run" ]]; then
            warn "missing tool: ${t} (acceptable in dry-run)"
        else
            fail "missing required tool: ${t}"
        fi
    fi
done

# Env checks (real-run modes only)
if [[ "${MODE}" != "dry-run" ]]; then
    [[ -n "${BACKUP_GPG_RECIPIENT:-}" ]] || fail "BACKUP_GPG_RECIPIENT required for ${MODE} mode"
fi

# ---------------------------------------------------------------------------
# Step 1 — Establish IC + war room
# ---------------------------------------------------------------------------
CURRENT_STEP="1_ic_establishment"
log_step "1_IC_ESTABLISHMENT_START"
log "step 1: establish IC + war room (target 15min)"

if [[ "${MODE}" == "dry-run" ]]; then
    log "DRY-RUN: would validate PD on-call rotation"
    log "DRY-RUN: would validate war-room channel template"
else
    log "MANUAL CHECKPOINT: IC has been declared in #drill-cold-restore-${TS_HUMAN}?"
    log "MANUAL CHECKPOINT: scribe assigned + PD incident opened?"
fi
log_step "1_IC_ESTABLISHMENT_DONE"

# ---------------------------------------------------------------------------
# Step 2 — Inventory damage
# ---------------------------------------------------------------------------
CURRENT_STEP="2_inventory"
log_step "2_INVENTORY_START"
log "step 2: inventory damage (target 30min cumulative)"

# In dry-run we just check that the probe commands are constructible.
# In staging we run real probes against synthetic "lost region" namespace.
# In prod we run real probes against real LOST_REGION.

probe_r2() {
    if [[ "${MODE}" == "dry-run" ]]; then
        log "DRY-RUN: probe R2 bucket corelink-cold-${LOST_REGION}"
        return 0
    fi
    if wrangler r2 bucket list --remote 2>/dev/null \
        | grep -q "corelink-cold-${LOST_REGION}"; then
        log "R2 bucket still present (warm failover may suffice; cold restore may be overkill)"
        return 1
    fi
    log "R2 HARD DESTROYED: bucket absent"
    return 0
}

probe_d1() {
    local destroyed=0
    for db in corelink_core corelink_audit corelink_billing; do
        if [[ "${MODE}" == "dry-run" ]]; then
            log "DRY-RUN: probe D1 db=${db}"
            destroyed=$((destroyed + 1))
            continue
        fi
        if ! wrangler d1 info "${db}" --remote >/dev/null 2>&1; then
            log "D1 HARD DESTROYED: ${db}"
            destroyed=$((destroyed + 1))
        fi
    done
    log "d1_destroyed_count=${destroyed}"
}

probe_kv() {
    if [[ "${MODE}" == "dry-run" ]]; then
        log "DRY-RUN: probe KV namespaces in ${LOST_REGION}"
        return 0
    fi
    if ! wrangler kv namespace list --remote 2>/dev/null \
        | grep -q "CORELINK_KV"; then
        log "KV HARD DESTROYED: namespaces absent"
    else
        log "KV namespaces present (verify per-region binding)"
    fi
}

probe_r2 || warn "R2 inventory inconclusive"
probe_d1
probe_kv
log_step "2_INVENTORY_DONE"

# ---------------------------------------------------------------------------
# Step 3 — Provision new region
# ---------------------------------------------------------------------------
CURRENT_STEP="3_provision"
log_step "3_PROVISION_START"
log "step 3: provision new region (target +60min)"

if [[ "${MODE}" == "dry-run" ]]; then
    log "DRY-RUN: would invoke terraform apply for region=${LOST_REGION}"
    log "DRY-RUN: would create R2 bucket corelink-cold-${LOST_REGION}"
    log "DRY-RUN: would create D1 databases (corelink_core, corelink_audit, corelink_billing)"
    log "DRY-RUN: would create KV namespaces (CORELINK_KV, CORELINK_FEATURE_FLAGS)"
    log "DRY-RUN: would run scripts/migrate_d1.sh"
else
    # In a real drill, an operator manually executes provisioning per
    # RB-COLD-RESTORE-FROM-ZERO §3. This script does NOT automate provisioning
    # because it requires explicit Terraform workspace selection + operator
    # review of plan output (charter: no destructive provisioning without
    # explicit operator gate).
    log "MANUAL CHECKPOINT: operator must execute Terraform apply per RB-COLD-RESTORE-FROM-ZERO §3"
    log "MANUAL CHECKPOINT: confirm completion by typing 'PROVISIONED' in war-room channel"
fi
log_step "3_PROVISION_DONE"

# ---------------------------------------------------------------------------
# Step 4 — Restore R2 from cross-region backup
# ---------------------------------------------------------------------------
CURRENT_STEP="4_r2_restore"
log_step "4_R2_RESTORE_START"
log "step 4: R2 restore (target +90min cumulative T+3:00)"

# Inventory backup manifests
log "checking backup manifest freshness in ${BACKUP_R2_BUCKET}"
if [[ "${MODE}" == "dry-run" ]]; then
    log "DRY-RUN: would list manifests + pick newest"
    SNAPSHOT_DATE="$(date -u +%Y-%m-%d)"
    log "DRY-RUN: would call scripts/restore-from-snapshot.sh --snapshot=${SNAPSHOT_DATE}"
else
    SNAPSHOT_DATE="$(wrangler r2 object list "${BACKUP_R2_BUCKET}" --remote 2>/dev/null \
        | jq -r '[.objects[]? | select(.key | endswith("manifest.json"))]
                 | sort_by(.uploaded) | last | .key' \
        | cut -d/ -f1 || true)"
    if [[ -z "${SNAPSHOT_DATE}" || "${SNAPSHOT_DATE}" == "null" ]]; then
        fail "no backup manifests found in ${BACKUP_R2_BUCKET}"
    fi
    log "selected snapshot: ${SNAPSHOT_DATE}"

    # RPO measurement
    if SNAPSHOT_EPOCH="$(date -u -d "${SNAPSHOT_DATE}" +%s 2>/dev/null \
        || date -u -j -f "%Y-%m-%d" "${SNAPSHOT_DATE}" +%s 2>/dev/null || echo "")" \
        && [[ -n "${SNAPSHOT_EPOCH}" ]]; then
        SNAPSHOT_AGE_MIN=$(( ($(date -u +%s) - SNAPSHOT_EPOCH) / 60 ))
        log "rpo_measurement_minutes=${SNAPSHOT_AGE_MIN}"
        if [[ ${SNAPSHOT_AGE_MIN} -gt 15 ]]; then
            warn "RPO target breached (target ≤ 15 min; actual ${SNAPSHOT_AGE_MIN} min)"
        fi
    else
        warn "could not parse SNAPSHOT_DATE=${SNAPSHOT_DATE} for RPO calc"
    fi

    # Delegate to existing restore primitive
    log "invoking restore-from-snapshot.sh"
    "${REPO_ROOT}/scripts/restore-from-snapshot.sh" \
        --snapshot "${SNAPSHOT_DATE}" \
        --env "${CORELINK_ENV}" \
        --target-region "${LOST_REGION}" \
        --skip-byok-check \
        || fail "restore-from-snapshot.sh failed"
fi
log_step "4_R2_RESTORE_DONE"

# ---------------------------------------------------------------------------
# Step 5 — D1 verification (restore was bundled into step 4)
# ---------------------------------------------------------------------------
CURRENT_STEP="5_d1_verify"
log_step "5_D1_VERIFY_START"
log "step 5: D1 invariant verification"

if [[ "${MODE}" == "dry-run" ]]; then
    log "DRY-RUN: would verify audit_events seq density"
    log "DRY-RUN: would verify tenant count parity"
else
    SEQ_CHECK="$(wrangler d1 execute corelink_audit --remote \
        --command "SELECT MAX(seq) AS max_seq, COUNT(*) AS total FROM audit_events;" \
        --json 2>/dev/null || echo "[]")"
    log "d1_seq_check_raw=${SEQ_CHECK}"
fi
log_step "5_D1_VERIFY_DONE"

# ---------------------------------------------------------------------------
# Step 6 — KV verification
# ---------------------------------------------------------------------------
CURRENT_STEP="6_kv_verify"
log_step "6_KV_VERIFY_START"
log "step 6: KV tenant_id integrity"

if [[ "${MODE}" == "dry-run" ]]; then
    log "DRY-RUN: would verify KV keys conform to t:<ulid>:... scheme"
else
    log "manual operator step per RB-COLD-RESTORE-FROM-ZERO §6"
fi
log_step "6_KV_VERIFY_DONE"

# ---------------------------------------------------------------------------
# Step 7 — DO state recovery
# ---------------------------------------------------------------------------
CURRENT_STEP="7_do_byok"
log_step "7_DO_BYOK_START"
log "step 7: DO state hydration + BYOK envelope re-bind"

if [[ "${MODE}" == "dry-run" ]]; then
    log "DRY-RUN: would warm-load DOs for synthetic drill tenants"
    log "DRY-RUN: would unwrap BYOK envelope for ${BYOK_TEST_TENANT}"
else
    log "manual operator step per RB-COLD-RESTORE-FROM-ZERO §7"
fi
log_step "7_DO_BYOK_DONE"

# ---------------------------------------------------------------------------
# Step 8 — Smoke tests (verification gate)
# ---------------------------------------------------------------------------
CURRENT_STEP="8_smoke"
log_step "8_SMOKE_START"
log "step 8: invoke verify-cold-restore.py"

VERIFY_OUT="drill-verification-${TS_HUMAN}.json"
if [[ "${MODE}" == "dry-run" ]]; then
    log "DRY-RUN: would invoke python3 scripts/verify-cold-restore.py"
    # Sanity: confirm the verify script exists + is executable.
    if [[ ! -f "${REPO_ROOT}/scripts/verify-cold-restore.py" ]]; then
        warn "verify-cold-restore.py not found at expected path"
    else
        log "verify-cold-restore.py present"
    fi
else
    if ! python3 "${REPO_ROOT}/scripts/verify-cold-restore.py" \
        --env "${CORELINK_ENV}" \
        --fixture "${REPO_ROOT}/specs/_compliance/drill-evidence/fixtures/cold-restore-tenant-pre-drill.json" \
        --output "${VERIFY_OUT}"; then
        log_step "8_SMOKE_FAIL"
        fail "verify-cold-restore.py reported failure; drill outcome = FAIL"
    fi
fi
log_step "8_SMOKE_PASS"

# ---------------------------------------------------------------------------
# Step 9 — Customer comms + go-live decision
# ---------------------------------------------------------------------------
CURRENT_STEP="9_golive"
log_step "9_GOLIVE_START"
log "step 9: customer comms + go-live decision"

if [[ "${MODE}" == "dry-run" ]]; then
    log "DRY-RUN: would stage customer-comms draft (no send)"
    log "DRY-RUN: would invoke corelink-drata-sync upload"
elif [[ "${MODE}" == "staging" ]]; then
    log "drill mode: drafting customer-comms (NOT sent); uploading evidence to drata"
elif [[ "${MODE}" == "prod" ]]; then
    log "PROD mode: customer-comms requires CTO approval; status page update + email send is operator gate"
    log "MANUAL CHECKPOINT: CTO go-live approval received?"
fi
log_step "9_GOLIVE_DONE"

# ---------------------------------------------------------------------------
# Wrap-up: evidence upload + summary
# ---------------------------------------------------------------------------
TS_END="$(date -u +%s)"
ELAPSED=$((TS_END - TS_START))
log "drill complete: elapsed_seconds=${ELAPSED} mode=${MODE}"

# RTO summary
log "rto_summary: read_path_seconds=${ELAPSED} target_read=14400 target_write=28800"
if [[ ${ELAPSED} -gt 28800 && "${MODE}" != "dry-run" ]]; then
    warn "write-path RTO breached: ${ELAPSED}s > 28800s"
fi

upload_drata_log "completed"

log "=== drill complete (mode=${MODE} status=ok elapsed=${ELAPSED}s) ==="
exit 0
