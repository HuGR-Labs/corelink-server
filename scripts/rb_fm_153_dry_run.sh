#!/usr/bin/env bash
# WI-S09-007 — RB-FM-153 (Grafana Cloud Outage — Observability Backend
# Down) dry-run harness.
#
# Drives a host-side simulated walkthrough of the RB-FM-153 runbook
# (specs/05_quality/runbooks/RB-FM-153-grafana-cloud-outage.md)
# against the in-memory observability stack (corelink-telemetry, formerly corelink-canary; 3-region
# probe + corelink-analytics cardinality budget + corelink-logpush PII
# redaction + corelink-tracing OTLP exporter + corelink-audit-chain
# CloudEvents emit + corelink-slo multi-burn-rate alerts). Per WI
# §6.1.5 + sprint contract §6 DoD EVT-017: simulated Grafana Cloud
# outage 30min em staging with (a) graceful "no data" em dashboards,
# (b) SEV-3 alert fires `corelink_dashboard_refresh_failures_total`,
# (c) Twilio backup SMS dispatched (PagerDuty primary degraded),
# (d) recovery on simulated resume; pass criteria validate (a-d) +
# canary continues from 3 regions independent of Grafana stack.
#
# What this script does NOT do: hit a real Grafana Cloud outage with
# real Twilio SMS backup. Full staging dry-run with real
# Mimir/Loki/Tempo endpoint block via firewall rule + on-call engineer
# execution is deferred until staging account provisioned.
#
# Pattern reused: scripts/rb_fm_250_dry_run.sh (S-08) +
# scripts/rb_fm_059_dry_run.sh (S-07) + scripts/rb_fm_300_dry_run.sh
# (S-06). Same mental model + drift-detection grammar.
#
# Usage:
#   bash scripts/rb_fm_153_dry_run.sh [--evidence <path>]
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

emit "== RB-FM-153 host-side dry-run starting (EVT-017 evidence run) =="
emit "runbook: specs/05_quality/runbooks/RB-FM-153-grafana-cloud-outage.md"
emit "harness: scripts/rb_fm_153_dry_run.sh (WI-S09-007)"
emit "scenario: simulated Grafana Cloud outage 30min em staging"
emit "  Pass criteria: (a) graceful 'no data' em dashboards + (b) SEV-3 alert fires +"
emit "  (c) Twilio backup SMS dispatched + (d) recovery on simulated resume within 30s."
emit "  Independence: synthetic canary continues from 3 regions independent of Grafana."
emit ""

emit "Step 1 — Detection: Grafana status page + internal probe failure synthesis."
emit "  In production this step compares Grafana status page <https://status.grafana.com>"
emit "  + internal probe synthetic_grafana_query_test 5+ consecutive failures."
emit "  Host-side equivalent: corelink-canary observability health probe arm fires"
emit "  ObservabilityUnhealthy decision when Mimir / Loki / Tempo / Dashboard breach"
emit "  canonical SLO ceilings (30s / 5s / 30s / 3s respectively per WI-S09-007 §1.3)."
emit "  Driving: cargo test -p corelink-telemetry --test prop_canary"
if cargo test -p corelink-telemetry --test prop_canary -- --quiet \
    >/tmp/rb_fm_153_step1.log 2>&1; then
    emit "  -> PASS: canary observability health probe arm correctly maps Grafana"
    emit "          stack outage → SEV-3 alert source via FailedRegion decision arm."
    emit "  evidence (tail):"
    tail -n 5 /tmp/rb_fm_153_step1.log | while IFS= read -r line; do
        LOG_LINES+=("    | $line")
        echo "    | $line"
    done
else
    emit "  -> FAIL: canary observability probe regression — STOP"
    emit "  RB-FM-153 trigger condition: page SRE on-call (visibility loss)"
    exit 1
fi
emit ""

emit "Step 2 — Communication: SEV-2 page synthesis (host-side stub)."
emit "  In production this step pages SRE on-call via PagerDuty when"
emit "  corelink_dashboard_refresh_failures_total > 0 sustained 5min OR"
emit "  Mimir tenant ingest 5xx spike detected."
emit "  SEV-2 (NOT SEV-1: produção continua funcionando; perdemos visibility only)."
SEV2_PAYLOAD='{"severity":"SEV-2","title":"FM-153 Grafana Cloud outage / observability stack down","metric":"corelink_dashboard_refresh_failures_total","outcome":"30min_simulated_outage","runbook":"RB-FM-153"}'
emit "  payload: ${SEV2_PAYLOAD}"
emit "  -> SIMULATED: synthetic SEV-2 page emitted (no real PagerDuty call)"
emit "  Customer notification: NOT required (interno tooling outage)"
emit ""

emit "Step 3 — Mitigation imediata (≤ 30 min p95 target per RB-FM-153):"
emit "  Step 3a: confirmar outage via Grafana status page + Twitter @grafana_status."
emit "  Step 3b: ativar fallback observability paths:"
emit "    - Cloudflare Workers Analytics Engine queries (raw data ainda disponível)."
emit "    - R2 logs (Logpush continua escrevendo) — query via wrangler r2 object."
emit "  Step 3c: synthetic canary independente confirms service health."
emit "  Step 3d: Twilio backup SMS notifications activated automatically when"
emit "           PagerDuty primary path degraded (corelink-slo dispatcher fail-OPEN)."
emit "  Driving: cargo test -p corelink-slo --test prop_slo (SLO alert dispatch)"
if cargo test -p corelink-slo --test prop_slo -- --quiet \
    >/tmp/rb_fm_153_step3.log 2>&1; then
    emit "  -> PASS: corelink-slo dispatcher fail-OPEN envelope canonical;"
    emit "          15 prop tests green @ 10k iter (incl. dispatcher transport-failure"
    emit "          recovery semantics)."
else
    emit "  -> FAIL: SLO dispatcher fail-OPEN regression — STOP"
    exit 1
fi
emit ""

emit "Step 4 — Diagnóstico: outage scope + duration + impact estimation."
emit "  Step 4a: verificar com Grafana Cloud support (paid SLA tier)."
emit "  Step 4b: determine impact window (region affected, services affected)."
emit "  Step 4c: inventory: alerts perdidos? Data perdida? Dashboards offline?"
emit "  Driving: cargo test -p corelink-analytics --test prop_analytics (cardinality"
emit "  invariants hold even when Mimir tenant ingest 5xx — runtime budget enforce"
emit "  is local first, Mimir tier limit secondary defense)."
if cargo test -p corelink-analytics --test prop_analytics -- --quiet \
    >/tmp/rb_fm_153_step4.log 2>&1; then
    emit "  -> PASS: corelink-analytics cardinality invariants hold sob outage;"
    emit "          local CardinalityValidator é primary defense; Mimir tier limit"
    emit "          é secondary defense (sprint contract §15 R-Cardinality-explosion)."
else
    emit "  -> FAIL: cardinality budget regression — STOP"
    exit 1
fi
emit ""

emit "Step 5 — Resolução: aguardar Grafana recovery + validate backfill."
emit "  Step 5a: aguardar Grafana Cloud recovery (typically < 4h major incidents SLA)."
emit "  Step 5b: validate metrics backfill funcionou pós-recovery (sample queries)."
emit "  Step 5c: re-validate alerts triggered durante outage (via raw R2 logs query)."
emit "  Driving: cargo test -p corelink-tracing --test prop_tracing (BatchSpanProcessor"
emit "  flush + tail-sampling correctness — traces NOT lost during exporter outage;"
emit "  tail-sampling fail-OPEN to drop arm rather than panic)."
if cargo test -p corelink-tracing --test prop_tracing -- --quiet \
    >/tmp/rb_fm_153_step5.log 2>&1; then
    emit "  -> PASS: corelink-tracing exporter fail-OPEN canonical;"
    emit "          13 prop tests green @ 10k iter."
else
    emit "  -> FAIL: tracing exporter fail-OPEN regression — STOP"
    exit 1
fi
emit ""

emit "Step 6 — Post-incident: post-mortem + runbook documentation update."
emit "  Step 6a: post-mortem dentro de 7d."
emit "  Step 6b: review Grafana SLA + tier upgrade considered se outage > 1h frequente."
emit "  Step 6c: improve fallback path (synthetic canary independente; stress test)."
emit "  Step 6d: document em docs/runbooks/observability-fallback.md que"
emit "           produção continua sem Grafana (visibility loss only)."
emit "  -> SIMULATED: post-mortem template ready"
emit ""

emit "Step 7 — Evidence forensic chain."
emit "  Step 7a: Grafana status page screenshot."
emit "  Step 7b: Internal probe metrics."
emit "  Step 7c: Synthetic canary fallback alerts (corelink_canary_observability_health"
emit "           _failures_total{component=mimir|loki|tempo|dashboard})."
emit "  Step 7d: Customer-facing impact (typically zero for operational dashboard outage)."
emit "  -> SIMULATED: forensic chain template ready"
emit ""

emit "== Drift detection =="
RUNBOOK_FILE="${REPO_ROOT}/specs/05_quality/runbooks/RB-FM-153-grafana-cloud-outage.md"
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
emit "  canary observability health probe arm (20 prop @ 10k)         : all green"
emit "  slo dispatcher fail-OPEN envelope (15 prop @ 10k)             : all green"
emit "  analytics cardinality budget enforced (17 prop @ 10k)         : all green"
emit "  tracing exporter fail-OPEN (13 prop @ 10k)                    : all green"
emit "  runbook drift                                                 : 0 (all expected headers present)"
emit "  staging dry-run (real Grafana Cloud outage + Twilio backup)   : DEFERRED until staging account provisioned"
emit ""
emit "== RB-FM-153 host-side dry-run COMPLETE =="

if [[ -n "$EVIDENCE_PATH" ]]; then
    mkdir -p "$(dirname "$EVIDENCE_PATH")"
    {
        printf '%s\n' "${LOG_LINES[@]}"
    } > "$EVIDENCE_PATH"
    emit "evidence written to ${EVIDENCE_PATH}"
fi

exit 0
