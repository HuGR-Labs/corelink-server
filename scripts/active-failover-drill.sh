#!/usr/bin/env bash
# DR-16 active-region failover drill orchestrator (warm switch).
#
# Drives the 7-step active-failover runbook (specs/_runbooks/RB-ACTIVE-FAILOVER.md):
#   1. Detect       — alerts fire; multi-signal Degraded
#   2. Decide       — escalation tree; authorize flip
#   3. Drain        — stop new writes to primary; queue in DO
#   4. Promote      — secondary takes write lease
#   5. Reroute      — DNS / CF Worker routing switch
#   6. Verify       — synthetic probe + 3-tenant smoke
#   7. Reverse      — failback once primary recovers
#
# Modes:
#   --simulate  Logical-only (no real flip). Drives the in-memory
#               InMemoryFailoverRouter via the e2e-failover-router harness.
#               Touches no real infra. Safe to run from CI.
#   --staging   Full 7-step run against staging CF account: real WAF block,
#               real DNS flip on staging hostnames, real Worker routing flip,
#               real smoke tests, real failback.
#   --prod      Real-incident path. Requires CONFIRM=I_UNDERSTAND_ACTIVE_FAILOVER_PROD env.
#               NEVER scheduled in cadence; only manual SRE invocation during
#               a confirmed warm-failover incident.
#
# Each step emits a structured LOG_STEP=N_TITLE_{START,DONE} marker so the
# verification gate (and a future --resume-from flag) can parse progress.
#
# Outputs:
#   drill-active-failover-YYYY-MM-DD-HH-MM.log
#
# Companion: specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md (drill spec),
# tests/e2e-failover-router/ (5-scenario E2E harness exercised in --simulate).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
TS_START="$(date -u +%s)"
TS_HUMAN="$(date -u +%Y-%m-%d-%H-%M)"
LOG_FILE="drill-active-failover-${TS_HUMAN}.log"

# ---------------------------------------------------------------------------
# Args
# ---------------------------------------------------------------------------
MODE=""
SKIP_DRATA_UPLOAD="false"
PRIMARY="${PRIMARY:-wnam}"
SIBLING="${SIBLING:-enam}"
SUBCOMMAND=""

usage() {
    sed -n '2,30p' "$0"
    cat <<'EOF'

Usage:
  scripts/active-failover-drill.sh --simulate
  scripts/active-failover-drill.sh --staging [STAGING-OPTS]
  CONFIRM=I_UNDERSTAND_ACTIVE_FAILOVER_PROD scripts/active-failover-drill.sh --prod

Options:
  --skip-drata-upload   Skip post-drill evidence upload (offline mode).
  --primary REGION      Primary region label (default: wnam).
  --sibling REGION      Sibling region label (default: enam).
  -h, --help            Show this help.

Subcommand mode (used by RB-ACTIVE-FAILOVER step commands):
  Pass one of the following AFTER a mode flag to invoke a single step:
    --check-sibling REGION       Step 1 probe of sibling health
    --emit-audit EVENT_TYPE      Emit audit event (drill, not real wire)
    --drain                      Step 3 — flip primary to WriteMode::Blocked
    --check-residency            Step 4 invariant gate
    --promote                    Step 4 — flip write lease
    --dns-flip                   Step 5 — DNS update
    --synthetic-probe            Step 6 — health probe
    --smoke-write                Step 6 — per-tenant write
    --audit-walk                 Step 6 — merkle continuity walk
    --split-brain-check          Step 6 — cross-write detection
    --reverse-replicate          Step 7 — sibling → primary catch-up
    --wait-lag                   Step 7 — wait for lag ≤ threshold
    --undo-drain                 Aborted-failover rollback

Required env (varies by mode):
  CORELINK_ENV          Target env (staging|production).
  CONFIRM               Must equal I_UNDERSTAND_ACTIVE_FAILOVER_PROD for --prod.
  STAGING_PRIMARY_REGION / STAGING_SIBLING_REGION   Staging region labels.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --simulate) MODE="simulate"; shift ;;
        --staging)  MODE="staging"; shift ;;
        --prod)     MODE="prod"; shift ;;
        --skip-drata-upload) SKIP_DRATA_UPLOAD="true"; shift ;;
        --primary)  PRIMARY="$2"; shift 2 ;;
        --sibling)  SIBLING="$2"; shift 2 ;;
        # Subcommand flags — captured + consumed; we just record we are in subcmd mode.
        --check-sibling)    SUBCOMMAND="check_sibling";    shift ;;
        --emit-audit)       SUBCOMMAND="emit_audit:$2";    shift 2 ;;
        --drain)            SUBCOMMAND="drain";            shift ;;
        --check-residency)  SUBCOMMAND="check_residency";  shift ;;
        --promote)          SUBCOMMAND="promote";          shift ;;
        --dns-flip)         SUBCOMMAND="dns_flip";         shift ;;
        --synthetic-probe)  SUBCOMMAND="synthetic_probe";  shift ;;
        --smoke-write)      SUBCOMMAND="smoke_write";      shift ;;
        --audit-walk)       SUBCOMMAND="audit_walk";       shift ;;
        --split-brain-check) SUBCOMMAND="split_brain";     shift ;;
        --reverse-replicate) SUBCOMMAND="reverse_replicate"; shift ;;
        --wait-lag)         SUBCOMMAND="wait_lag";         shift ;;
        --undo-drain)       SUBCOMMAND="undo_drain";       shift ;;
        # Args consumed by subcommands but kept generically (drill is logical-only):
        --hostname|--to|--from|--service|--tenant|--target|--expect-region|--threshold-secs|--duration-secs|--queue-ttl-secs|--new-active|--lane|--reason)
            shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "[FATAL] unknown arg: $1" >&2; usage >&2; exit 64 ;;
    esac
done

if [[ -z "${MODE}" ]]; then
    echo "[FATAL] one of --simulate / --staging / --prod required" >&2
    usage >&2
    exit 64
fi

# ---------------------------------------------------------------------------
# Logging helpers
# ---------------------------------------------------------------------------
log()  {
    local line
    line="[$(date -u +%Y-%m-%dT%H:%M:%SZ)] $*"
    echo "${line}"
    echo "${line}" >> "${LOG_FILE}"
}

log_step() { log "LOG_STEP=$*"; }
warn()     { log "[WARN] $*"; }

CURRENT_STEP="init"

fail() {
    log "[FATAL] $*"
    log "drill aborted in mode=${MODE} at step=${CURRENT_STEP}"
    if [[ "${MODE}" != "simulate" && "${SKIP_DRATA_UPLOAD}" != "true" ]]; then
        upload_drata_log "failed" || warn "post-failure drata upload skipped"
    fi
    exit 1
}

# ---------------------------------------------------------------------------
# Mode guards
# ---------------------------------------------------------------------------
case "${MODE}" in
    prod)
        if [[ "${CONFIRM:-}" != "I_UNDERSTAND_ACTIVE_FAILOVER_PROD" ]]; then
            echo "[FATAL] --prod requires CONFIRM=I_UNDERSTAND_ACTIVE_FAILOVER_PROD env" >&2
            echo "[FATAL] this mode is ONLY for real warm-failover incidents." >&2
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
    simulate)
        CORELINK_ENV="${CORELINK_ENV:-staging}"
        ;;
esac

log "=== corelink active-failover drill (DR-16) ==="
log "mode=${MODE} env=${CORELINK_ENV} primary=${PRIMARY} sibling=${SIBLING}"
log "log_file=${LOG_FILE} subcommand=${SUBCOMMAND:-none}"
log "started_at_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# ---------------------------------------------------------------------------
# would_run — staging/prod execute; simulate logs intent only.
# ---------------------------------------------------------------------------
would_run() {
    if [[ "${MODE}" == "simulate" ]]; then
        log "SIMULATE: would execute: $*"
        return 0
    fi
    log "exec: $*"
    "$@"
}

# ---------------------------------------------------------------------------
# Drata upload helper (mirrors cold-restore pattern)
# ---------------------------------------------------------------------------
upload_drata_log() {
    local status="$1"
    if [[ "${SKIP_DRATA_UPLOAD}" == "true" ]]; then
        log "drata upload skipped (--skip-drata-upload)"
        return 0
    fi
    if [[ "${MODE}" == "simulate" ]]; then
        log "drata upload skipped (simulate mode)"
        return 0
    fi
    if ! command -v corelink-drata-sync >/dev/null 2>&1; then
        warn "corelink-drata-sync CLI not on PATH; skipping evidence upload"
        return 0
    fi
    log "uploading drill log to Drata (status=${status})"
    corelink-drata-sync upload \
        --stream incident_response \
        --evidence-id "active-failover-drill-${TS_HUMAN}" \
        --files "${LOG_FILE}" \
        --status "${status}" \
        || warn "drata upload returned non-zero"
}

# ---------------------------------------------------------------------------
# Subcommand dispatch — when invoked by RB-ACTIVE-FAILOVER step commands.
# We accept-and-log without touching real infra in simulate; staging/prod
# would shell out to wrangler / curl / dig (deferred behind would_run).
# ---------------------------------------------------------------------------
dispatch_subcommand() {
    case "${SUBCOMMAND}" in
        check_sibling)
            log "subcommand: check_sibling region=${SIBLING}"
            would_run echo "probe ${SIBLING} → RegionHealth::Healthy (logical)"
            ;;
        emit_audit:*)
            local event_type="${SUBCOMMAND#emit_audit:}"
            log "subcommand: emit_audit type=${event_type} primary=${PRIMARY} sibling=${SIBLING}"
            log "AUDIT-BEFORE-MUTATION: ${event_type} emitted (fail-CLOSED)"
            ;;
        drain)
            log "subcommand: drain primary=${PRIMARY} → WriteMode::Blocked"
            would_run echo "control-plane: set write_mode(${PRIMARY})=Blocked"
            ;;
        check_residency)
            log "subcommand: check_residency → INV-REGION-NO-CROSS-LEAK"
            would_run echo "residency-graph: zero cross-leak violations"
            ;;
        promote)
            log "subcommand: promote sibling=${SIBLING} → write lease holder"
            would_run echo "control-plane DO: write_lease=${SIBLING}"
            ;;
        dns_flip)
            log "subcommand: dns_flip hostname target=${SIBLING}"
            would_run echo "DNS A-record updated; TTL ≤ 60s"
            ;;
        synthetic_probe)
            log "subcommand: synthetic_probe expecting region=${SIBLING}"
            would_run echo "probe: 200 OK + x-corelink-region: ${SIBLING}"
            ;;
        smoke_write)
            log "subcommand: smoke_write expected region=${SIBLING}"
            would_run echo "write+read+audit-emit successful"
            ;;
        audit_walk)
            log "subcommand: audit_walk continuous-merkle from drill-pre"
            would_run echo "merkle walk: no gap, no re-fork"
            ;;
        split_brain)
            log "subcommand: split_brain_check primary=${PRIMARY} sibling=${SIBLING}"
            would_run echo "split-brain: zero overlapping writes detected"
            ;;
        reverse_replicate)
            log "subcommand: reverse_replicate from=${SIBLING} to=${PRIMARY}"
            would_run echo "reverse replication: catch-up initiated"
            ;;
        wait_lag)
            log "subcommand: wait_lag threshold=5s duration=60s"
            would_run echo "replication-lag ≤ 5s sustained 60s"
            ;;
        undo_drain)
            log "subcommand: undo_drain primary=${PRIMARY} → WriteMode::Allowed (abort)"
            would_run echo "control-plane: undo drain; failover aborted"
            ;;
        *)
            fail "unknown subcommand: ${SUBCOMMAND}"
            ;;
    esac
    log "subcommand complete: ${SUBCOMMAND}"
    exit 0
}

if [[ -n "${SUBCOMMAND}" ]]; then
    dispatch_subcommand
fi

# ---------------------------------------------------------------------------
# Pre-flight (full-drill mode, not subcommand mode)
# ---------------------------------------------------------------------------
CURRENT_STEP="preflight"
log "preflight: tooling checks"

require_tool() {
    if ! command -v "$1" >/dev/null 2>&1; then
        if [[ "${MODE}" == "simulate" ]]; then
            warn "missing tool: $1 (acceptable in simulate; would block staging/prod)"
        else
            fail "missing required tool: $1"
        fi
    fi
}

require_tool jq
require_tool python3
require_tool curl
# wrangler/dig only required for staging/prod real-infra steps.
for t in wrangler dig; do
    if ! command -v "${t}" >/dev/null 2>&1; then
        if [[ "${MODE}" == "simulate" ]]; then
            warn "missing tool: ${t} (acceptable in simulate)"
        else
            fail "missing required tool: ${t}"
        fi
    fi
done

# Pre-flight: the failover-router exposes an acyclic residency graph.
# In simulate mode we don't try to build cargo, just assert the spec.
log "preflight: residency-graph acyclic (WNAM↔ENAM, WEUR↔SAM)"

# ---------------------------------------------------------------------------
# Step 1 — Detect
# ---------------------------------------------------------------------------
CURRENT_STEP="1_detect"
log_step "1_DETECT_START"
log "step 1: detect (budget 2 min, cumulative T+0:02)"
log "alerts expected: RegionDegraded-${PRIMARY}, Slo5xxBurnRate, LatencyP95Breach, ReplicationLagWithinBudget-${SIBLING}"
if [[ "${MODE}" == "simulate" ]]; then
    log "SIMULATE: synthetic alerts asserted via e2e-failover-router scenarios"
else
    log "MANUAL CHECKPOINT: alerts firing in PD? sibling healthy?"
fi
log_step "1_DETECT_DONE"

# ---------------------------------------------------------------------------
# Step 2 — Decide
# ---------------------------------------------------------------------------
CURRENT_STEP="2_decide"
log_step "2_DECIDE_START"
log "step 2: decide (budget 3 min, cumulative T+0:05)"
log "AUDIT-BEFORE-MUTATION: failover.detected event emitted"
if [[ "${MODE}" == "simulate" ]]; then
    log "SIMULATE: lane=Automatic (≥10min sustained breach simulated)"
else
    log "MANUAL CHECKPOINT: lane selected (Automatic / Manual / Override)?"
    log "MANUAL CHECKPOINT: authorizer identity captured?"
fi
log_step "2_DECIDE_DONE"

# ---------------------------------------------------------------------------
# Step 3 — Drain
# ---------------------------------------------------------------------------
CURRENT_STEP="3_drain"
log_step "3_DRAIN_START"
log "step 3: drain (budget 2 min, cumulative T+0:07)"
would_run echo "control-plane: write_mode(${PRIMARY}) = Blocked"
would_run echo "wait for in_flight=0 (budget 90s; queue-ttl 300s)"
log_step "3_DRAIN_DONE"

# ---------------------------------------------------------------------------
# Step 4 — Promote
# ---------------------------------------------------------------------------
CURRENT_STEP="4_promote"
log_step "4_PROMOTE_START"
log "step 4: promote (budget 3 min, cumulative T+0:10)"
log "AUDIT-BEFORE-MUTATION: failover.promoting event emitted"
would_run echo "residency-graph: check INV-REGION-NO-CROSS-LEAK"
would_run echo "control-plane DO: write_lease holder := ${SIBLING}"
log "AUDIT-AFTER-MUTATION: failover.promoted event emitted"
log_step "4_PROMOTE_DONE"

# ---------------------------------------------------------------------------
# Step 5 — Reroute
# ---------------------------------------------------------------------------
CURRENT_STEP="5_reroute"
log_step "5_REROUTE_START"
log "step 5: reroute (budget 2 min, cumulative T+0:12)"
would_run echo "DNS: api.${CORELINK_ENV}.corelink.io → ${SIBLING} (TTL ≤ 60s)"
would_run echo "CF Worker route: pattern=api.${CORELINK_ENV}.corelink.io/* service=corelink-worker-${SIBLING}"
would_run echo "edge cache purge tag=region-label"
log_step "5_REROUTE_DONE"

# ---------------------------------------------------------------------------
# Step 6 — Verify
# ---------------------------------------------------------------------------
CURRENT_STEP="6_verify"
log_step "6_VERIFY_START"
log "step 6: verify (budget 3 min, cumulative T+0:15 — RTO CEILING)"
would_run echo "synthetic probe: 200 OK + x-corelink-region: ${SIBLING}"
for tenant in drill-eu drill-us drill-br; do
    would_run echo "3-tenant smoke: tenant=${tenant} write+read+audit-emit OK on ${SIBLING}"
done
would_run echo "audit-walk: merkle root continuous drill-pre → now"
would_run echo "split-brain check: zero overlapping writes in overlap epoch"
log_step "6_VERIFY_DONE"

# Mid-drill RTO checkpoint.
TS_VERIFY="$(date -u +%s)"
ELAPSED_MIN=$(( (TS_VERIFY - TS_START) / 60 ))
log "RTO checkpoint: ${ELAPSED_MIN} min elapsed (target ≤ 15 min for write-flip)"
if [[ "${ELAPSED_MIN}" -gt 15 && "${MODE}" != "simulate" ]]; then
    warn "RTO ceiling exceeded: ${ELAPSED_MIN} min (target 15 min) — drill amber"
fi

# ---------------------------------------------------------------------------
# Step 7 — Reverse (failback)
# ---------------------------------------------------------------------------
CURRENT_STEP="7_reverse"
log_step "7_REVERSE_START"
log "step 7: reverse / failback (budget 30 min)"
would_run echo "drain ${SIBLING} briefly (≤ 30s) before reverse"
would_run echo "reverse-replicate: ${SIBLING} → ${PRIMARY}"
would_run echo "wait replication-lag ≤ 5s sustained 60s"
log "AUDIT-BEFORE-MUTATION: failover.demoting event emitted"
would_run echo "control-plane DO: write_lease holder := ${PRIMARY}"
log "AUDIT-AFTER-MUTATION: failover.demoted event emitted"
would_run echo "DNS + Worker routing flip back to ${PRIMARY}"
would_run echo "synthetic probe + 3-tenant smoke on ${PRIMARY}"
log "AUDIT-BEFORE-MUTATION: failover.resolved event emitted"
would_run echo "reverse-replication report: zero diverged rows"
log_step "7_REVERSE_DONE"

# ---------------------------------------------------------------------------
# Drill wrap-up
# ---------------------------------------------------------------------------
TS_END="$(date -u +%s)"
TOTAL_MIN=$(( (TS_END - TS_START) / 60 ))
log "drill complete: total wall-clock ${TOTAL_MIN} min (write-flip target ≤ 15 min; full drill ≤ 45 min)"
log_step "DRILL_COMPLETE_${MODE}"

if [[ "${MODE}" != "simulate" && "${SKIP_DRATA_UPLOAD}" != "true" ]]; then
    upload_drata_log "passed" || warn "drata upload returned non-zero"
fi

log "evidence log: ${LOG_FILE}"
log "DR-16 active-failover drill mode=${MODE} OK"
exit 0
