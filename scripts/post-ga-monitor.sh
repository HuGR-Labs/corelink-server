#!/usr/bin/env bash
# post-ga-monitor.sh — Wave-27 daily post-GA continuity monitor.
#
# What this DOES:
#   - Reads the §§1..§4 criteria from `specs/_runbooks/RB-POST-GA-CONTINUITY.md`
#     (per-window exit gates) and the §5 freeze-thaw truth table from
#     `specs/_audits/2026-05-16-ga-1-feature-freeze.md §6`.
#   - Emits a single markdown digest under `reports/post-ga/YYYY-MM-DD-digest.md`
#     summarizing: current T+offset window, per-window gate status, greenlight
#     composite sample-point status (when available), §5 thaw-condition truth
#     table, next-action list, and the first-quarter framework v1.0.1 prep
#     status (§6 of the runbook).
#   - In --dry-run mode, does NOT touch any production endpoint, does NOT
#     query Prometheus or PagerDuty, does NOT mutate any file outside
#     reports/post-ga/. All "metrics" are synthetic deterministic placeholders.
#     The dry-run path is the one validated by the runbook's §9 cadence and is
#     the one the CI gate executes.
#   - In --window=24-72h / --window=72h-7d modes, emits the per-window baseline
#     / weekly-summary artefacts referenced by §2.1.1 / §3.1.3 of the runbook.
#
# What this does NOT do:
#   - It DOES NOT touch any real Cloudflare, PagerDuty, Statuspage, Prometheus,
#     Grafana, or Postmark endpoint. All external sources are stubbed in
#     dry-run; the live mode is reserved for a future hooking commit and is
#     gated behind the explicit `--live` flag (which is NOT exercised by CI).
#   - It DOES NOT page on-call. The runbook's §1.4 / §2.3 / §3.4 / §4.4
#     escalation triggers are evaluated by humans reading the digest, not by
#     this script.
#   - It DOES NOT mutate the GA-1 feature-freeze monitor JSON. That file is
#     append-only and is mutated only by `scripts/check-ga-freeze-allowed.py`
#     + Owner 2-key process per the freeze audit §4.
#
# Charter constraints honored:
#   - No unsafe code (this is bash + jq optional).
#   - No background processes; everything is synchronous.
#   - Exits 0 in --dry-run when all schema checks pass; non-zero only if the
#     companion runbook or freeze audit cannot be found.
#   - No file mutation outside reports/post-ga/ (defensive `mkdir -p`).
#
# Usage:
#   scripts/post-ga-monitor.sh --dry-run
#   scripts/post-ga-monitor.sh --window=0-24h
#   scripts/post-ga-monitor.sh --window=24-72h --emit-baseline
#   scripts/post-ga-monitor.sh --window=72h-7d --emit-summary
#   scripts/post-ga-monitor.sh --window=7d-30d
#   scripts/post-ga-monitor.sh --thaw-check     # evaluate §5 truth table only
#
# Exit codes:
#   0 — digest emitted; all schema checks pass
#   1 — companion runbook / freeze audit not found (configuration error)
#   2 — invalid CLI argument
#   3 — write failure under reports/post-ga/
#
# Reference: specs/_runbooks/RB-POST-GA-CONTINUITY.md §9 (dry-run cadence).

set -euo pipefail

# ---------------------------------------------------------------------------
# Argument parsing
# ---------------------------------------------------------------------------

DRY_RUN=0
WINDOW=""
EMIT_BASELINE=0
EMIT_SUMMARY=0
THAW_CHECK=0
LIVE=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)        DRY_RUN=1; shift ;;
    --live)           LIVE=1; shift ;;
    --window=*)       WINDOW="${1#--window=}"; shift ;;
    --emit-baseline)  EMIT_BASELINE=1; shift ;;
    --emit-summary)   EMIT_SUMMARY=1; shift ;;
    --thaw-check)     THAW_CHECK=1; shift ;;
    -h|--help)
      sed -n '1,40p' "$0"
      exit 0
      ;;
    *)
      echo "ERROR: unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

# Default: --dry-run if no flag passed (CI-safe default).
if [[ "$DRY_RUN" -eq 0 && "$LIVE" -eq 0 && "$THAW_CHECK" -eq 0 && -z "$WINDOW" ]]; then
  DRY_RUN=1
fi

# ---------------------------------------------------------------------------
# Locate repo root + companion docs
# ---------------------------------------------------------------------------

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

RUNBOOK="$REPO_ROOT/specs/_runbooks/RB-POST-GA-CONTINUITY.md"
FREEZE_AUDIT="$REPO_ROOT/specs/_audits/2026-05-16-ga-1-feature-freeze.md"
CUTOVER_RUNBOOK="$REPO_ROOT/specs/_runbooks/RB-GA-CUTOVER.md"
FRAMEWORK_AUDIT="$REPO_ROOT/specs/_audits/2026-05-15-framework-v1-0-0-ga-audit.md"
OUT_DIR="$REPO_ROOT/reports/post-ga"

if [[ ! -f "$RUNBOOK" ]]; then
  echo "ERROR: post-GA runbook not found at $RUNBOOK" >&2
  exit 1
fi
if [[ ! -f "$FREEZE_AUDIT" ]]; then
  echo "ERROR: GA-1 freeze audit not found at $FREEZE_AUDIT" >&2
  exit 1
fi
if [[ ! -f "$CUTOVER_RUNBOOK" ]]; then
  echo "ERROR: cutover runbook not found at $CUTOVER_RUNBOOK" >&2
  exit 1
fi
if [[ ! -f "$FRAMEWORK_AUDIT" ]]; then
  echo "ERROR: framework audit not found at $FRAMEWORK_AUDIT" >&2
  exit 1
fi

mkdir -p "$OUT_DIR" || { echo "ERROR: cannot write to $OUT_DIR" >&2; exit 3; }

TODAY="$(date -u +%Y-%m-%d)"
NOW="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# ---------------------------------------------------------------------------
# Schema checks: confirm companion docs still hold the anchor headings the
# digest depends on. If a heading drifts, the digest will fall back to a
# WARNING row but the script still exits 0 in --dry-run.
# ---------------------------------------------------------------------------

check_anchor () {
  local file="$1" needle="$2" label="$3"
  if grep -qF "$needle" "$file"; then
    echo "OK $label"
  else
    echo "WARN $label (anchor drift: \"$needle\" not found in $(basename "$file"))"
  fi
}

ANCHORS_BLOCK="$(
  check_anchor "$RUNBOOK"        "## §1. T+0 to T+24h"                "rb-§1"
  check_anchor "$RUNBOOK"        "## §2. T+24h to T+72h"               "rb-§2"
  check_anchor "$RUNBOOK"        "## §3. T+72h to T+7d"                "rb-§3"
  check_anchor "$RUNBOOK"        "## §4. T+7d to T+30d"                "rb-§4"
  check_anchor "$RUNBOOK"        "## §5. Freeze-thaw conditions"       "rb-§5"
  check_anchor "$RUNBOOK"        "## §6. First-quarter framework"      "rb-§6"
  check_anchor "$FREEZE_AUDIT"   "## §6. Thaw conditions"              "freeze-§6"
  check_anchor "$CUTOVER_RUNBOOK" "## 6. Post-cutover"                 "cutover-§6"
  check_anchor "$FRAMEWORK_AUDIT" "90-day rolling quarterly cadence"   "framework-quarterly"
)"

# ---------------------------------------------------------------------------
# Thaw truth table evaluation
# ---------------------------------------------------------------------------

# In dry-run, all values are synthetic placeholders.
# In live mode (future), each row would be sourced from the canonical
# system-of-record (PagerDuty, pilot tracker, etc).

if [[ "$DRY_RUN" -eq 1 ]]; then
  THAW_5_1="PENDING (cutover not yet executed in dry-run posture)"
  THAW_5_2="PENDING (no T+7d window observed in dry-run posture)"
  THAW_5_3="PENDING (PagerDuty not queried in dry-run posture)"
  THAW_5_4="PENDING (pilot tracker not queried in dry-run posture)"
  THAW_5_5="PENDING (Owner declaration absent in dry-run posture)"
  THAW_VERDICT="ACTIVE-FREEZE (dry-run synthetic)"
else
  THAW_5_1="UNKNOWN (live mode stub — see Live-mode TODO in script)"
  THAW_5_2="UNKNOWN"
  THAW_5_3="UNKNOWN"
  THAW_5_4="UNKNOWN"
  THAW_5_5="UNKNOWN"
  THAW_VERDICT="UNKNOWN (live mode stub)"
fi

# ---------------------------------------------------------------------------
# Per-window status placeholders (dry-run synthetic)
# ---------------------------------------------------------------------------

case "$WINDOW" in
  "0-24h"|"")
    WINDOW_LABEL="T+0..T+24h (§1)"
    WINDOW_GATES="greenlight-composite sample-points (T+0/+6h/+12h/+24h) | initial pilot ack ≥ 3/5 | status page OPERATIONAL sticky-7d | T+24h baseline saved"
    ;;
  "24-72h")
    WINDOW_LABEL="T+24h..T+72h (§2)"
    WINDOW_GATES="SLO baseline envelope v1 locked | zero RED SLO rows vs staging | ≥1 first-GA-CAS-write | D+1 trigger closure 2-key"
    ;;
  "72h-7d")
    WINDOW_LABEL="T+72h..T+7d (§3)"
    WINDOW_GATES="first weekly compliance digest | adversarial absorption | DEBT register Q1 prep | cutover retro sealed | D+7 closure"
    ;;
  "7d-30d")
    WINDOW_LABEL="T+7d..T+30d (§4)"
    WINDOW_GATES="≥1 pilot-to-GA conversion | 30d retro sealed | post-mortem cadence stood up | D+30 closure | Q1 framework prep"
    ;;
  *)
    echo "ERROR: invalid --window=$WINDOW (expected 0-24h | 24-72h | 72h-7d | 7d-30d)" >&2
    exit 2
    ;;
esac

# ---------------------------------------------------------------------------
# Optional emit: baseline envelope (§2.1.4) / weekly summary (§3.1.3)
# ---------------------------------------------------------------------------

EXTRA_ARTIFACTS=""

if [[ "$EMIT_BASELINE" -eq 1 ]]; then
  BASELINE_FILE="$OUT_DIR/baseline-envelope-v1.json"
  cat > "$BASELINE_FILE" <<'EOF'
{
  "schema_version": "1.0.0",
  "anchor": "T+72h post-GA",
  "source": "scripts/post-ga-monitor.sh --emit-baseline (dry-run synthetic)",
  "slo_rows": {
    "p99_ms_cas_write": {"observed": null, "staging_ref": null, "delta_pct": null, "band": "PENDING"},
    "p99_ms_audit_export": {"observed": null, "staging_ref": null, "delta_pct": null, "band": "PENDING"},
    "burn_rate_1h": {"observed": null, "budget": null, "band": "PENDING"}
  },
  "notes": "Synthetic baseline emitted in dry-run mode; live values written only when --live flag is wired."
}
EOF
  EXTRA_ARTIFACTS="${EXTRA_ARTIFACTS}\n- Baseline envelope written: \`reports/post-ga/baseline-envelope-v1.json\`"
fi

if [[ "$EMIT_SUMMARY" -eq 1 ]]; then
  SUMMARY_FILE="$OUT_DIR/T-plus-7d-weekly-summary.md"
  cat > "$SUMMARY_FILE" <<EOF
# Post-GA T+7d Weekly Summary (dry-run synthetic)

Generated: $NOW
Source: \`scripts/post-ga-monitor.sh --window=72h-7d --emit-summary\`

This is a stub emitted in dry-run mode. In live mode the summary will
include: per-SLO p50/p95/p99 + burn-rate envelope vs §2.1.4 baseline,
SEV count by severity, pilot tenant health roll-up, DEBT burn-down delta.

See \`specs/_runbooks/RB-POST-GA-CONTINUITY.md\` §3.1.3.
EOF
  EXTRA_ARTIFACTS="${EXTRA_ARTIFACTS}\n- Weekly summary written: \`reports/post-ga/T-plus-7d-weekly-summary.md\`"
fi

# ---------------------------------------------------------------------------
# Compose the digest
# ---------------------------------------------------------------------------

DIGEST_FILE="$OUT_DIR/${TODAY}-digest.md"

{
  echo "# Post-GA Continuity Monitor — Digest"
  echo
  echo "**Generated:** $NOW"
  echo "**Mode:** $( [[ "$DRY_RUN" -eq 1 ]] && echo "dry-run (synthetic)" || echo "live" )"
  echo "**Window evaluated:** $WINDOW_LABEL"
  echo "**Source runbook:** \`specs/_runbooks/RB-POST-GA-CONTINUITY.md\`"
  echo
  echo "---"
  echo
  echo "## 1. Anchor schema check"
  echo
  echo '```'
  echo "$ANCHORS_BLOCK"
  echo '```'
  echo
  echo "## 2. Window gates"
  echo
  echo "**Window:** $WINDOW_LABEL"
  echo
  echo "**Gates evaluated:**"
  echo
  echo "- $WINDOW_GATES"
  echo
  echo "(In dry-run all gate evaluations are PENDING; live mode wires the gate-evaluator stubs to real telemetry.)"
  echo
  echo "## 3. Freeze-thaw truth table (RB §5 / freeze audit §6)"
  echo
  echo "| # | Condition | Status |"
  echo "|---|---|---|"
  echo "| 5.1 | GA cutover executed + post-mortem SEALED | $THAW_5_1 |"
  echo "| 5.2 | T+7d clean SLO window | $THAW_5_2 |"
  echo "| 5.3 | Zero SEV-0/SEV-1 across T+0..T+7d | $THAW_5_3 |"
  echo "| 5.4 | At least 1 pilot-to-GA conversion confirmed | $THAW_5_4 |"
  echo "| 5.5 | Owner formal thaw declaration filed | $THAW_5_5 |"
  echo
  echo "**Verdict:** $THAW_VERDICT"
  echo
  echo "## 4. Quarterly framework v1.0.1 review prep (RB §6)"
  echo
  if [[ -f "$OUT_DIR/quarterly-anchor.txt" ]]; then
    echo '```'
    cat "$OUT_DIR/quarterly-anchor.txt"
    echo '```'
  else
    echo "_Anchor not yet captured. Will be written by §3.3.3 of \`RB-POST-GA-CONTINUITY.md\` at T+7d._"
  fi
  echo
  if [[ -n "$EXTRA_ARTIFACTS" ]]; then
    echo "## 5. Extra artefacts emitted this run"
    echo
    printf '%b\n' "$EXTRA_ARTIFACTS"
    echo
  fi
  echo "## 6. Next actions"
  echo
  echo "- Read \`specs/_runbooks/RB-POST-GA-CONTINUITY.md\` exit criteria for window: $WINDOW_LABEL"
  echo "- If any condition above is RED in live mode, page on-call SRE + route per RB §1.4 / §2.3 / §3.4 / §4.4."
  echo "- If §5 truth table flips to all-TRUE, file the thaw declaration per RB §5.1."
  echo
  echo "---"
  echo
  echo "_End of digest. Auto-generated by \`scripts/post-ga-monitor.sh\`._"
} > "$DIGEST_FILE"

echo "OK post-ga-monitor.sh: digest written to $DIGEST_FILE"
if [[ "$DRY_RUN" -eq 1 ]]; then
  echo "OK dry-run mode: no production endpoints touched; exit 0."
fi
exit 0
