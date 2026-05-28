#!/usr/bin/env bash
# h-day-cutover-gate-pack.sh — Wave 32 Phase H cutover gate orchestrator.
#
# PURPOSE:
#   Single entry-point that runs all three smoke harnesses in sequence
#   and produces a single GREEN / YELLOW / RED verdict:
#
#     1. scripts/pre-cutover-weekly-verify.sh   (8 DEFER items from Wave 28)
#     2. scripts/pre-cutover-wave32-extension.sh  (8 Wave-32 new surfaces)
#     3. scripts/ga-cutover-prod-dressrun.sh --dry-run (prep-ring dressrun)
#
#   Verdict rules (§3 WP-H.1 DoD):
#     GREEN  — all 8 DEFER items at expected state + all W32 surfaces PASS +
#               dressrun exits 0.
#     YELLOW — 1-2 PASS-WITH-NOTES (Sentry deferred, DEPS-MISSING for a
#               parallel-agent script not yet landed, etc.); requires Owner
#               acknowledgement via interactive prompt (or --no-prompt to
#               accept automatically in CI).
#     RED    — any FAIL in either smoke harness OR dressrun exits non-zero;
#               halts cutover with diagnostic dump.
#
# Usage:
#   bash scripts/h-day-cutover-gate-pack.sh [OPTIONS]
#
#   Options:
#     --dry-run          Pass --dry-run to child scripts; no network calls.
#     --no-prompt        Accept YELLOW verdict automatically (CI mode).
#     --evidence <path>  Path for dressrun evidence JSON
#                        (passed through to ga-cutover-prod-dressrun.sh).
#     --date YYYY-MM-DD  Override date for dressrun + digest naming.
#     -h|--help          Print this help.
#
# Exit codes:
#   0 — GREEN or YELLOW (after Owner ack).
#   1 — RED verdict (FAIL detected); cutover must not proceed.
#   2 — Environment / setup failure.
#
# Dependencies:
#   Required (exit 2 if missing):
#     - scripts/pre-cutover-weekly-verify.sh  (602 LOC baseline)
#     - scripts/ga-cutover-prod-dressrun.sh   (743 LOC dressrun)
#   Required (exit 2 if missing):
#     - scripts/pre-cutover-wave32-extension.sh  (this wave's extension)
#
# Charter: SYNCHRONOUS BASH ONLY. No PII emitted. Read-only.
# W3 constraint: this script does not put/mutate any secrets or infra state.
# Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>

set -euo pipefail

# -----------------------------------------------------------------------
# Argument parsing.
# -----------------------------------------------------------------------

DRY_RUN=0
NO_PROMPT=0
EVIDENCE_PATH=""
RUN_DATE="$(date -u +"%Y-%m-%d")"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        --no-prompt)
            NO_PROMPT=1
            shift
            ;;
        --evidence)
            EVIDENCE_PATH="${2:-}"
            shift 2
            ;;
        --date)
            RUN_DATE="${2:-}"
            shift 2
            ;;
        -h|--help)
            grep '^# ' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            printf 'unknown flag: %s\n' "$1" >&2
            printf 'usage: %s [--dry-run] [--no-prompt] [--evidence <path>] [--date YYYY-MM-DD]\n' "$0" >&2
            exit 2
            ;;
    esac
done

# -----------------------------------------------------------------------
# Locate repo root.
# -----------------------------------------------------------------------

if ! REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)"; then
    printf '::error::not inside a git repo\n' >&2
    exit 2
fi
cd "${REPO_ROOT}"

# -----------------------------------------------------------------------
# Script paths.
# -----------------------------------------------------------------------

WEEKLY_SCRIPT="scripts/pre-cutover-weekly-verify.sh"
EXTENSION_SCRIPT="scripts/pre-cutover-wave32-extension.sh"
DRESSRUN_SCRIPT="scripts/ga-cutover-prod-dressrun.sh"

# -----------------------------------------------------------------------
# Verify required scripts exist before running anything.
# -----------------------------------------------------------------------

for required in "${WEEKLY_SCRIPT}" "${EXTENSION_SCRIPT}" "${DRESSRUN_SCRIPT}"; do
    if [[ ! -f "${required}" ]]; then
        printf '::error:: required script not found: %s\n' "${required}" >&2
        exit 2
    fi
done

# -----------------------------------------------------------------------
# Helpers.
# -----------------------------------------------------------------------

BANNER_WIDTH=70

banner() {
    local title="$1"
    local pad=$(( (BANNER_WIDTH - ${#title} - 2) / 2 ))
    local line
    line="$(printf '%*s' "${BANNER_WIDTH}" '' | tr ' ' '=')"
    printf '\n%s\n' "${line}"
    printf '%*s %s %*s\n' "${pad}" '' "${title}" "${pad}" ''
    printf '%s\n' "${line}"
}

now_iso() { date -u +"%Y-%m-%dT%H:%M:%SZ"; }

# -----------------------------------------------------------------------
# Summary state.
# -----------------------------------------------------------------------

WEEKLY_EXIT=0
EXTENSION_EXIT=0
DRESSRUN_EXIT=0
NOTES=()           # YELLOW-level observations (deferred items, DEPS-MISSING)

# -----------------------------------------------------------------------
# Stage 1: Pre-cutover weekly verify (8 DEFER items).
# -----------------------------------------------------------------------

banner "Stage 1/3 — pre-cutover-weekly-verify.sh"
printf 'Script:  %s\n' "${WEEKLY_SCRIPT}"
printf 'Mode:    %s\n' "$([[ "${DRY_RUN}" -eq 1 ]] && printf 'dry-run' || printf 'live')"
printf 'Started: %s\n\n' "$(now_iso)"

WEEKLY_FLAGS=()
[[ "${DRY_RUN}" -eq 1 ]] && WEEKLY_FLAGS+=(--dry-run)
[[ -n "${RUN_DATE}" ]] && WEEKLY_FLAGS+=(--date "${RUN_DATE}")

if ! bash "${WEEKLY_SCRIPT}" "${WEEKLY_FLAGS[@]}" 2>&1; then
    WEEKLY_EXIT=$?
fi

printf '\nStage 1 exit code: %d\n' "${WEEKLY_EXIT}"

# -----------------------------------------------------------------------
# Stage 2: Wave-32 extension (8 new surfaces).
# -----------------------------------------------------------------------

banner "Stage 2/3 — pre-cutover-wave32-extension.sh"
printf 'Script:  %s\n' "${EXTENSION_SCRIPT}"
printf 'Mode:    %s\n' "$([[ "${DRY_RUN}" -eq 1 ]] && printf 'dry-run' || printf 'live')"
printf 'Started: %s\n\n' "$(now_iso)"

EXTENSION_FLAGS=()
[[ "${DRY_RUN}" -eq 1 ]] && EXTENSION_FLAGS+=(--dry-run)

EXTENSION_OUTPUT=""
if ! EXTENSION_OUTPUT="$(bash "${EXTENSION_SCRIPT}" "${EXTENSION_FLAGS[@]}" 2>&1)"; then
    EXTENSION_EXIT=$?
fi
printf '%s\n' "${EXTENSION_OUTPUT}"
printf '\nStage 2 exit code: %d\n' "${EXTENSION_EXIT}"

# Collect DEFERRED / DEPS-MISSING lines as YELLOW notes.
while IFS= read -r line; do
    if printf '%s' "${line}" | grep -qE '(DEFERRED|DEPS-MISSING)'; then
        NOTES+=("${line}")
    fi
done <<< "${EXTENSION_OUTPUT}"

# -----------------------------------------------------------------------
# Stage 3: ga-cutover-prod-dressrun.sh --dry-run (or sim mode).
# -----------------------------------------------------------------------

banner "Stage 3/3 — ga-cutover-prod-dressrun.sh --dry-run"
printf 'Script:  %s\n' "${DRESSRUN_SCRIPT}"
printf 'Mode:    dry-run (always --mode sim in gate-pack; no mutations)\n'
printf 'Started: %s\n\n' "$(now_iso)"

DRESSRUN_FLAGS=(--mode sim)
[[ -n "${EVIDENCE_PATH}" ]] && DRESSRUN_FLAGS+=(--evidence "${EVIDENCE_PATH}")
[[ -n "${RUN_DATE}" ]] && DRESSRUN_FLAGS+=(--date "${RUN_DATE}")

if ! bash "${DRESSRUN_SCRIPT}" "${DRESSRUN_FLAGS[@]}" 2>&1; then
    DRESSRUN_EXIT=$?
fi

printf '\nStage 3 exit code: %d\n' "${DRESSRUN_EXIT}"

# -----------------------------------------------------------------------
# Verdict computation.
# -----------------------------------------------------------------------

banner "CUTOVER GATE VERDICT"
printf 'Run date:       %s\n' "${RUN_DATE}"
printf 'Completed at:   %s\n' "$(now_iso)"
printf 'Weekly exit:    %d\n' "${WEEKLY_EXIT}"
printf 'Extension exit: %d\n' "${EXTENSION_EXIT}"
printf 'Dressrun exit:  %d\n' "${DRESSRUN_EXIT}"
printf '\n'

VERDICT="GREEN"

# Any FAIL → RED.
if [[ "${WEEKLY_EXIT}" -ne 0 || "${EXTENSION_EXIT}" -ne 0 || "${DRESSRUN_EXIT}" -ne 0 ]]; then
    VERDICT="RED"
fi

# YELLOW: no FAIL but DEFERRED / DEPS-MISSING notes present.
if [[ "${VERDICT}" == "GREEN" && "${#NOTES[@]}" -gt 0 ]]; then
    VERDICT="YELLOW"
fi

# -----------------------------------------------------------------------
# Verdict output + Owner ack gate (YELLOW).
# -----------------------------------------------------------------------

case "${VERDICT}" in
    GREEN)
        printf '██████  VERDICT: GREEN  ██████\n'
        printf 'All 8 DEFER items at expected state, all Wave-32 surfaces PASS,\n'
        printf 'dressrun exited 0. Cutover may proceed.\n'
        exit 0
        ;;

    YELLOW)
        printf '██████  VERDICT: YELLOW  ██████\n'
        printf 'No hard FAILs detected but the following notes require Owner ack:\n\n'
        for note in "${NOTES[@]}"; do
            printf '  YELLOW-NOTE: %s\n' "${note}"
        done
        printf '\n'
        printf 'YELLOW policy: 1-2 deferred or DEPS-MISSING items are acceptable\n'
        printf 'when they correspond to (a) Sentry DSN not yet configured, or\n'
        printf '(b) a parallel-agent script not yet landed in this worktree.\n'
        printf 'Owner must acknowledge before proceeding.\n\n'
        if [[ "${NO_PROMPT}" -eq 1 ]]; then
            printf '[--no-prompt] Owner ack accepted automatically (CI mode).\n'
            exit 0
        fi
        # Interactive Owner ack.
        if [[ -t 0 ]]; then
            read -r -p "Owner ACK: Type 'proceed' to accept YELLOW verdict and continue: " ack
            if [[ "${ack}" == "proceed" ]]; then
                printf 'Owner ack recorded. Cutover may proceed (YELLOW).\n'
                exit 0
            else
                printf 'Owner did not type "proceed". Halting cutover.\n'
                exit 1
            fi
        else
            printf 'stdin is not a tty and --no-prompt was not set.\n'
            printf 'Re-run with --no-prompt to accept YELLOW automatically in CI.\n'
            exit 1
        fi
        ;;

    RED)
        printf '██████  VERDICT: RED  ██████\n'
        printf 'One or more checks FAILED. Cutover must NOT proceed until resolved.\n\n'
        printf 'Diagnostic summary:\n'
        [[ "${WEEKLY_EXIT}" -ne 0 ]] && printf '  - pre-cutover-weekly-verify.sh    exit %d  ← REGRESSION or env failure\n' "${WEEKLY_EXIT}"
        [[ "${EXTENSION_EXIT}" -ne 0 ]] && printf '  - pre-cutover-wave32-extension.sh  exit %d  ← Wave-32 surface FAIL\n' "${EXTENSION_EXIT}"
        [[ "${DRESSRUN_EXIT}" -ne 0 ]] && printf '  - ga-cutover-prod-dressrun.sh       exit %d  ← Dressrun FAIL\n' "${DRESSRUN_EXIT}"
        printf '\nResolve all FAILs and re-run this gate pack before cutover.\n'
        exit 1
        ;;
esac
