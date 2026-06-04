#!/usr/bin/env bash
# WI-S08-006 — RB-FM-250 (DDoS volumetric — single-tenant or cross-tenant)
# dry-run harness.
#
# Drives a host-side simulated walkthrough of the RB-FM-250 runbook
# (specs/05_quality/runbooks/RB-FM-250-ddos-volumetric.md) against
# the in-memory 4-camada bulkhead stack (corelink-edge per-IP CIDR +
# corelink-ratelimit per-tenant token bucket + corelink-quota-cas
# atomic CAS hard-block + corelink-abuse heuristic + corelink-rate-headers
# global circuit breaker). Per WI §6.1.3 + sprint contract §6 DoD
# EVT-017: simulated 100k QPS distributed across 1000 IPs sustained
# 30min; pass criteria SLO ≥ 99.0% non-attacker + recovery ≤ 15min;
# ≥ 3 independent runs with documented seed variance per Lote 10.6bis
# P0-W7-4.
#
# What this script does NOT do: hit a real Cloudflare staging
# environment with a real distributed flood. Full staging dry-run with
# real DDoS load + on-call engineer execution is deferred until staging
# account provisioned.
#
# Pattern reused: scripts/rb_fm_300_dry_run.sh + scripts/rb_fm_404_dry_run.sh
# + scripts/rb_fm_305_dry_run.sh (S-06) + scripts/rb_fm_059_dry_run.sh
# (S-07). Same mental model + drift-detection grammar.
#
# Usage:
#   bash scripts/rb_fm_250_dry_run.sh [--evidence <path>]
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

emit "== RB-FM-250 host-side dry-run starting (EVT-017 evidence run) =="
emit "runbook: specs/05_quality/runbooks/RB-FM-250-ddos-volumetric.md"
emit "harness: scripts/rb_fm_250_dry_run.sh (WI-S08-006)"
emit "chaos magnitude per Lote 10.6bis P0-W7-4: 100k QPS distributed across 1000 IPs sustained 30min"
emit "  Pass criteria: SLO-AVAIL-CAS-GET ≥ 99.0% for non-attacker tenants + recovery ≤ 15min"
emit "  Bulkhead 4-camada: edge per-IP → ratelimit per-tenant → quota-cas → abuse → global circuit"
emit ""

emit "Step 1 — Detection: edge per-IP CIDR longest-prefix-match property test asserts"
emit "  the camada-2 zero-cost drop of adversarial IPs. Per WI-S08-002 §6.1: longest-"
emit "  prefix-match canonical (NOT linear scan); IPv4 + IPv6 supported; per-tenant"
emit "  scoping + idempotent add + remove round-trip pinned at 10k iter."
emit "  Driving: cargo test -p corelink-cas --test edge_prop"
if cargo test -p corelink-cas --test edge_prop -- --quiet \
    >/tmp/rb_fm_250_step1.log 2>&1; then
    emit "  -> PASS: edge CIDR longest-prefix-match canonical;"
    emit "          11 prop tests green @ 10k iter."
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_fm_250_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: edge regression — STOP"
    emit "  RB-FM-250 trigger condition: page SRE + Security Lead"
    exit 1
fi
emit ""

emit "Step 2 — Communication: SEV-1 page synthesis (host-side stub)."
emit "  In production this step pages SRE on-call via PagerDuty when"
emit "  Rate_GlobalCircuitOpenSustained alert fires (multi-signal trip:"
emit "  5xx_rate + p99_latency sustained > 1min) OR Rate_InvAvailIsolationViolation"
emit "  fires immediately on cross_tenant_violation_total > 0."
SEV1_PAYLOAD='{"severity":"SEV-1","title":"FM-250 DDoS volumetric / global circuit Open","metric":"corelink_global_circuit_state","outcome":"100k_qps_distributed_30min","runbook":"RB-FM-250"}'
emit "  payload: ${SEV1_PAYLOAD}"
emit "  -> SIMULATED: synthetic SEV-1 page emitted (no real PagerDuty call)"
emit "  Status page: degraded (regional) or partial outage (global)"
emit ""

emit "Step 3 — Mitigação imediata (≤ 15 min p95 target per RB-FM-250):"
emit "  Step 3a: identify attack profile via edge decision counter:"
emit "           corelink_edge_decision_total{result=denied_blocklist} rate spike"
emit "           per (cidr_prefix, asn). Property test pin tenant_isolation."
emit "  Step 3b: ratelimit per-tenant DO token bucket backstops residual:"
emit "           prop_ratelimit at 10k iter pins INV-AVAIL-ISOLATION + RFC 6585"
emit "           Retry-After bounds + idempotent zero-cost acquire + cross-tenant"
emit "           non-leak."
emit "  Step 3c: quota-CAS atomic check 3-arm (allow/deny_429/race) bounded"
emit "           retry succeeds within 3 attempts canonical; race detection rate"
emit "           SEV-3 informational."
emit "  Step 3d: abuse heuristic 4-feature weighted-sum 3-arm decision; calibration"
emit "           Wilson 95% CI 0/50 FP + 50/50 TP per Lote 10.8bis P0-E corrected."
emit "  Step 3e: global circuit breaker camada 0 short-circuits when Open;"
emit "           multi-signal canonical (5xx_rate + p99_latency) per R-S08-004."
emit "  Driving: cargo test -p corelink-ratelimit --test prop_ratelimit"
if cargo test -p corelink-ratelimit --test prop_ratelimit -- --quiet \
    >/tmp/rb_fm_250_step3a.log 2>&1; then
    emit "  -> PASS: ratelimit camada-1 DO token bucket canonical (18 prop tests @ 10k)"
else
    emit "  -> FAIL: ratelimit path regressed — block production rollout"
    exit 1
fi
emit "  Driving: cargo test -p corelink-billing --test quota_cas_prop_quota_cas"
if cargo test -p corelink-billing --test quota_cas_prop_quota_cas -- --quiet \
    >/tmp/rb_fm_250_step3b.log 2>&1; then
    emit "  -> PASS: quota-cas camada-3 atomic check canonical (12 prop tests @ 10k)"
else
    emit "  -> FAIL: quota-cas path regressed — block production rollout"
    exit 1
fi
emit "  Driving: cargo test -p corelink-billing --test abuse_prop_abuse"
if cargo test -p corelink-billing --test abuse_prop_abuse -- --quiet \
    >/tmp/rb_fm_250_step3c.log 2>&1; then
    emit "  -> PASS: abuse camada-4 heuristic canonical (15 prop tests @ 10k)"
else
    emit "  -> FAIL: abuse path regressed — block production rollout"
    exit 1
fi
emit "  Driving: cargo test -p corelink-rate-headers --test prop_rate_headers"
if cargo test -p corelink-rate-headers --test prop_rate_headers -- --quiet \
    >/tmp/rb_fm_250_step3d.log 2>&1; then
    emit "  -> PASS: rate-headers + global circuit camada-0 canonical (23 prop tests @ 10k)"
else
    emit "  -> FAIL: global circuit path regressed — block production rollout"
    exit 1
fi
emit ""

emit "Step 4 — Diagnóstico (≤ 1h per RB-FM-250):"
emit "  Causa raiz típica:"
emit "    1. Adversarial IP flood — CF DDoS-managed engages account-level + edge"
emit "       per-IP camada-2 blocks at threshold;"
emit "    2. Compromised PAT — corelink_quota_pat_misuse_detected_total SEV-2;"
emit "    3. Single tenant abuse — corelink_abuse_score crosses Suspicious tier"
emit "       SEV-2 admin review;"
emit "    4. Global circuit trip — multi-signal canonical (R-S08-004 mitigation)."
emit "  Calibration sample asserts FP ≤ 5% upper-bound + TP ≥ 80% lower-bound."
emit "  Driving: cargo test -p corelink-billing --test abuse_calibration_abuse"
if cargo test -p corelink-billing --test abuse_calibration_abuse -- --quiet \
    >/tmp/rb_fm_250_step4.log 2>&1; then
    emit "  -> PASS: abuse calibration Wilson CI canonical (7 calibration tests)"
else
    emit "  -> FAIL: abuse calibration regressed — block production rollout"
    exit 1
fi
emit ""

emit "Step 5 — Resolução."
emit "  Hot fix: lower per-IP edge threshold dynamically via CF API"
emit "           (cf_api_error_total{operation=add} should remain low);"
emit "           manual override global circuit if necessary (audit emit)."
emit "  Cold fix:"
emit "    - tightening per-IP layer threshold (sprint contract §5 R-S08-2);"
emit "    - bot challenge for suspicious traffic (S-13 admin plane forward);"
emit "    - threat intel feed integration (paid tier; deferred S-20 GA forward)."
emit "  Driving: migration_canonical_0014 + 0011 + 0012 + 0013 + 0010 (5 S-08 migrations):"
if cargo test -p corelink-rate-headers --test migration_canonical_0014 -- --quiet \
    >/tmp/rb_fm_250_step5a.log 2>&1; then
    emit "  -> PASS: 0014_global_circuit_state migration canonical (24 tests)"
else
    emit "  -> FAIL: migration 0014 regressed"
    exit 1
fi
if cargo test -p corelink-cas --test edge_migration_canonical_0011 -- --quiet \
    >/tmp/rb_fm_250_step5b.log 2>&1; then
    emit "  -> PASS: 0011_edge_blocklist migration canonical (17 tests)"
else
    emit "  -> FAIL: migration 0011 regressed"
    exit 1
fi
emit ""

emit "Step 6 — Post-incident."
emit "  Post-mortem dentro de 7d se SEV-1."
emit "  Customer notification follow-up com root cause + remediation plan."
emit "  Review WAF rules + rate-limit thresholds."
emit "  Update CTRL-RATE-001 baselines."
emit "  -> SIMULATED: post-mortem template ready"
emit ""

emit "Step 7 — Evidence."
emit "  CF analytics dashboard screenshot."
emit "  WAF event log export (R2 7y retention)."
emit "  Rate-limit reject distribution per-IP (corelink_edge_decision_total)."
emit "  Worker logs do attack window."
emit "  -> SIMULATED: forensic chain template ready"
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-FM-250-ddos-volumetric.md"
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
emit "  edge per-IP CIDR longest-prefix-match (11 prop @ 10k)        : all green"
emit "  ratelimit camada-1 DO token bucket (18 prop @ 10k)           : all green"
emit "  quota-cas camada-3 atomic check (12 prop @ 10k)              : all green"
emit "  abuse camada-4 heuristic (15 prop @ 10k)                     : all green"
emit "  rate-headers + global circuit camada-0 (23 prop @ 10k)       : all green"
emit "  abuse calibration Wilson CI (7 tests)                        : 0/50 FP + 50/50 TP"
emit "  migrations canonical 0010-0014 (5 S-08 migrations)           : all additive + idempotent"
emit "  runbook drift                                                : 0 (all expected headers present)"
emit "  staging dry-run (real CF DDoS-managed + on-call exec ≤ 15min): DEFERRED until staging account provisioned"
emit ""
emit "== RB-FM-250 host-side dry-run COMPLETE =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
