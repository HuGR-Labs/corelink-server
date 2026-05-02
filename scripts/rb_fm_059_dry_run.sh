#!/usr/bin/env bash
# WI-S07-005 — RB-FM-059 (Cloudflare Durable Object quota exceeded) dry-run
# harness.
#
# Drives a host-side simulated walkthrough of the RB-FM-059 runbook
# (specs/05_quality/runbooks/RB-FM-059-do-quota-exceeded.md) against
# the in-memory quota stack (corelink-quota InMemoryQuotaCheck + DO
# actor model `Mutex<()>` decision_lock + reservation tracker +
# size-proportional TTL pipeline). Per WI §6.1 + sprint contract §6 DoD:
# detection ≤ 5 min p95 via SLO probe; remediation ≤ 30 min p95;
# INV-QUOTA-ENFORCEMENT 0 violations sustained ≥ 3 independent runs
# with seed variance per Lote 10.6bis P0-W7-4.
#
# What this script does NOT do: hit a real Cloudflare staging
# environment. Full staging dry-run with chaos PR injecting FM-059 race
# (1000 concurrent writes at 99.9% quota) + on-call engineer execution
# is deferred until staging account provisioned + real CF DO singleton
# binding wired (charter `trait-abstraction-defer` pattern).
#
# Pattern reused: scripts/rb_fm_300_dry_run.sh + rb_fm_404_dry_run.sh +
# rb_fm_305_dry_run.sh (WI-S06-007). Same mental model + drift-detection
# grammar.
#
# Usage:
#   bash scripts/rb_fm_059_dry_run.sh [--evidence <path>]
#
# Exit codes:
#   0 — dry-run succeeded; every step emitted the expected outcome.
#   1 — drift detected: one or more steps did not match the runbook.
#   2 — environment / build failure prevented the run.

set -euo pipefail

EVIDENCE_PATH="${EVIDENCE_PATH:-}"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --evidence)
            EVIDENCE_PATH="${2:-}"
            shift 2
            ;;
        *)
            echo "unknown flag: $1" >&2
            echo "usage: $0 [--evidence <path>]" >&2
            exit 2
            ;;
    esac
done

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

LOG_LINES=()

emit() {
    local stamp
    stamp="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
    local line="${stamp} | ${1}"
    echo "$line"
    LOG_LINES+=("$line")
}

emit "== RB-FM-059 host-side dry-run starting (EVT-017 evidence run) =="
emit "runbook: specs/05_quality/runbooks/RB-FM-059-do-quota-exceeded.md"
emit "harness: scripts/rb_fm_059_dry_run.sh (WI-S07-005)"
emit "chaos magnitude per Lote 10.6bis P0-W7-4: 1000 concurrent writes at 99.9% quota"
emit "  Detection signal: corelink_quota_denials_total{result=race_detected}"
emit "                    (must remain 0 across the prop suite)"
emit ""

emit "Step 1 — Detection: quota check property test asserts the DO actor"
emit "  model Mutex<()> decision_lock serialises concurrent decisions"
emit "  byte-for-byte and INV-QUOTA-ENFORCEMENT holds across 10k iter."
emit "  Driving: cargo test -p corelink-quota --test prop_quota \\"
emit "            -- prop_quota_atomic_no_race"
if cargo test -p corelink-quota --test prop_quota -- \
    prop_quota_atomic_no_race --quiet \
    >/tmp/rb_fm_059_step1.log 2>&1; then
    emit "  -> PASS: DO actor model serialisation canonical;"
    emit "          INV-QUOTA-ENFORCEMENT 0 violations across 10k iter."
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_fm_059_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: quota race property test regression — STOP"
    emit "  RB-FM-059 trigger condition: page SRE + freeze quota writes"
    exit 1
fi
emit ""

emit "Step 2 — Communication: SEV-1 page synthesis (host-side stub)."
emit "  In production this step pages SRE on-call via PagerDuty when"
emit "  Dedup_InvQuotaEnforcementViolation alert fires (rate >0; 0m for"
emit "  immediate fire). Internal incident channel #incidents-corelink."
emit "  Customer notification only if isolated to single DO + customer"
emit "  impact is latency spike."
SEV1_PAYLOAD='{"severity":"SEV-1","title":"FM-059 DO quota exceeded / decision_lock leak","metric":"corelink_quota_denials_total{result=race_detected}","outcome":"1k_concurrent_writes_at_99.9pct","runbook":"RB-FM-059"}'
emit "  payload: ${SEV1_PAYLOAD}"
emit "  -> SIMULATED: synthetic SEV-1 page emitted (no real PagerDuty call)"
emit ""

emit "Step 3 — Mitigação imediata (≤ 5 min p95 target per RB-FM-059):"
emit "  Step 3a: identificar DO afetado via Worker logs do_id"
emit "           correlation. Per WI-S07-003 design: per-tenant per-"
emit "           region DO routing (tenant.primary_region) — single"
emit "           DO leak isolated to that scope."
emit "  Step 3b: re-route novo tráfego via PAT-FAILOVER-001 OR"
emit "           degrade-mode rate-limit-bypass-warning (PAT-DEGRADE-001)."
emit "  Step 3c: compactar storage trigger DO cleanup() purges expired"
emit "           reservations (reservation_ttl_ms expired entries)."
emit "  Step 3d: verify reservation expiry path: size-proportional TTL"
emit "           min(7d, max(60s, req_bytes/1MB/s × 2)) (Lote 10.7bis"
emit "           R5 P0-2). Multipart 160 GiB no longer over-quota mid-"
emit "           upload."
emit "  Driving: reservation expiry + tenant isolation:"
emit "  Driving: cargo test -p corelink-quota --test prop_quota \\"
emit "            -- prop_reservation_expiry_releases_bytes prop_tenant_isolation"
if cargo test -p corelink-quota --test prop_quota -- \
    prop_reservation_expiry_releases_bytes prop_tenant_isolation --quiet \
    >/tmp/rb_fm_059_step3.log 2>&1; then
    emit "  -> PASS: reservation expiry releases bytes + tenant isolation canonical"
else
    emit "  -> FAIL: reservation TTL or isolation regression — block production rollout"
    exit 1
fi
emit ""

emit "Step 4 — Diagnóstico (≤ 30 min):"
emit "  Step 4a: query do_storage_inspection admin endpoint -> top keys"
emit "           by size."
emit "  Step 4b: identify leak source:"
emit "           - Counters não-expirando (TTL bug)?"
emit "           - Sliding window com excessive entries?"
emit "           - Replay storage de chave revogada?"
emit "  Step 4c: cross-reference com tenant tier (free vs enterprise;"
emit "           enterprise tem budget maior; per-tier defaults pinned"
emit "           by WI-S07-002 EvictionConfig per ADR-0019)."
emit "  Driving: per-tier TTL canonical pinning + reservation TTL monotone:"
emit "  Driving: cargo test -p corelink-quota --test prop_quota \\"
emit "            -- prop_reservation_ttl_size_proportional"
if cargo test -p corelink-quota --test prop_quota -- \
    prop_reservation_ttl_size_proportional --quiet \
    >/tmp/rb_fm_059_step4.log 2>&1; then
    emit "  -> PASS: size-proportional reservation TTL canonical"
else
    emit "  -> FAIL: reservation TTL formula regressed — escalate to Architect"
    exit 1
fi
emit ""

emit "Step 5 — Resolução."
emit "  Hot fix: TTL aggressive cleanup + DO storage budget alert"
emit "           lowered para 70% (currently SEV-3 telemetry @ 80%, SEV-2"
emit "           @ 95%, SEV-1 @ 100% per ADR-0020 FROZEN boundary)."
emit "  Cold fix: DO state model refactor (use array-based ring buffer"
emit "           em vez de KV-style unbounded keys)."
emit "  Prevenção: property test + chaos test gerando 100k requests/seg"
emit "           para DO única e medindo state growth — currently 10k iter"
emit "           PR-gate; 100k iter nightly tier per WI-S07-005 §F."
emit "  Driving: audit emit per decision arm (fail-closed envelope):"
emit "  Driving: cargo test -p corelink-quota --test prop_quota \\"
emit "            -- prop_audit_emit_per_decision_arm prop_idempotent_reservation_lookup"
if cargo test -p corelink-quota --test prop_quota -- \
    prop_audit_emit_per_decision_arm prop_idempotent_reservation_lookup --quiet \
    >/tmp/rb_fm_059_step5.log 2>&1; then
    emit "  -> PASS: audit fail-closed envelope + idempotent reservation lookup green"
else
    emit "  -> FAIL: post-fix validation regressed — block production rollout"
    exit 1
fi
emit ""

emit "Step 6 — Post-incident."
emit "  Post-mortem dentro de 7d se SEV-2 confirmed."
emit "  Update CTRL-RATE-001 + CTRL-QUOTA-001 alert thresholds."
emit "  Review com SRE e Architect."
emit "  -> SIMULATED: post-mortem template ready"
emit ""

emit "Step 7 — Evidence."
emit "  Logs DO + traces (corelink.quota.{check_passed, denied_429,"
emit "  reserved, reservation_expired, reservation_rolled_in})."
emit "  Storage size histogram (DO state growth)."
emit "  Tenant impact metrics (latency p99 delta vs SLO < 3 ms target)."
emit "  -> SIMULATED: forensic chain template ready"
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-FM-059-do-quota-exceeded.md"
if [[ ! -f "$RUNBOOK_FILE" ]]; then
    emit "  -> FAIL: runbook file missing at $RUNBOOK_FILE"
    exit 1
fi
EXPECTED_HEADERS=("Detecção" "Comunicação" "Mitigação imediata" "Diagnóstico" "Resolução" "Post-incident" "Evidence" "References")
DRIFT=0
for header in "${EXPECTED_HEADERS[@]}"; do
    if ! grep -q "^## ${header}" "$RUNBOOK_FILE"; then
        emit "  -> DRIFT: runbook is missing header '${header}'"
        DRIFT=1
    fi
done
if [[ $DRIFT -eq 0 ]]; then
    emit "  -> PASS: every expected runbook header present (no drift)"
else
    emit "  -> FAIL: runbook drift detected; harness MUST be updated together"
    exit 1
fi

emit ""
emit "== EVT-017 evidence summary =="
emit "  prop_quota_atomic_no_race (10k iter)                    : 0 INV-QUOTA-ENFORCEMENT violations"
emit "  prop_reservation_expiry_releases_bytes + tenant isolation: green"
emit "  prop_reservation_ttl_size_proportional                  : monotone formula canonical"
emit "  audit fail-closed envelope + idempotent reservation     : green"
emit "  runbook drift                                           : 0 (all expected headers present)"
emit "  staging dry-run (chaos PR + on-call exec ≤ 30 min p95)  : DEFERRED until staging account provisioned"
emit ""
emit "== RB-FM-059 host-side dry-run COMPLETE =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
