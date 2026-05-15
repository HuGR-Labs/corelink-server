#!/usr/bin/env bash
# i18n-stale-check.sh — R-prep i18n-de deliverable.
#
# Flags translated pages whose EN source has been updated more recently
# than the locale counterpart. Designed for weekly CI scheduling — see
# .github/workflows/i18n-stale.yml.
#
# Logic:
#   For every EN file under apps/docs/docs/**/*.{md,mdx}:
#     For every locale in {pt-BR, es-419, de}:
#       Compare git log -1 last-modified date of EN vs locale shadow.
#       If EN newer by more than $MAX_STALE_DAYS, flag as stale.
#
# Output:
#   - Human-readable summary on stdout.
#   - Optional Markdown report at $OUT (default: dist/i18n-stale.md).
#   - Exit codes:
#       0 — no stale entries (or below threshold)
#       1 — stale entries found ≥ $FAIL_THRESHOLD (default: 1)
#       2 — fatal error (git not available, malformed args)
#
# Usage:
#   bash scripts/i18n-stale-check.sh                        # defaults (14 d)
#   MAX_STALE_DAYS=7 bash scripts/i18n-stale-check.sh       # tighter SLA
#   bash scripts/i18n-stale-check.sh --json out.json        # JSON output
#   bash scripts/i18n-stale-check.sh --no-fail              # report-only
#
# SLA reference: TRANSLATION-WORKFLOW.md §3 — Tier-1 locales ≤ 14 d of EN change.

set -euo pipefail

# ──────────────────────────────────────────────────────────────────────────────
# Config — overridable by env
# ──────────────────────────────────────────────────────────────────────────────

MAX_STALE_DAYS="${MAX_STALE_DAYS:-14}"
FAIL_THRESHOLD="${FAIL_THRESHOLD:-1}"
# R-prep i18n-de — `de` joined as the fourth canonical locale.
LOCALES=(pt-BR es-419 de)

# ──────────────────────────────────────────────────────────────────────────────
# Arg parsing
# ──────────────────────────────────────────────────────────────────────────────

OUT_MD=""
OUT_JSON=""
NO_FAIL=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --md)
      OUT_MD="${2:-}"
      shift 2
      ;;
    --json)
      OUT_JSON="${2:-}"
      shift 2
      ;;
    --no-fail)
      NO_FAIL=1
      shift
      ;;
    --threshold-days)
      MAX_STALE_DAYS="${2:-}"
      shift 2
      ;;
    -h|--help)
      sed -n '2,30p' "$0"
      exit 0
      ;;
    *)
      echo "[i18n-stale] unknown arg: $1" >&2
      exit 2
      ;;
  esac
done

# ──────────────────────────────────────────────────────────────────────────────
# Locate repo + docs root
# ──────────────────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCS_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$DOCS_ROOT/../.." && pwd)"
cd "$REPO_ROOT"

if ! command -v git >/dev/null 2>&1; then
  echo "[i18n-stale] FATAL: git not found in PATH" >&2
  exit 2
fi

SRC_REL="apps/docs/docs"
I18N_REL="apps/docs/i18n"

# Epoch threshold = now - MAX_STALE_DAYS days
NOW_EPOCH="$(date -u +%s)"
THRESHOLD_SECS=$(( MAX_STALE_DAYS * 24 * 3600 ))

# ──────────────────────────────────────────────────────────────────────────────
# Last-commit epoch helper (uses git log -1; falls back to mtime when the file
# is untracked).
# ──────────────────────────────────────────────────────────────────────────────

last_commit_epoch() {
  local path="$1"
  if [[ ! -e "$path" ]]; then
    echo ""
    return 0
  fi
  local ts
  ts="$(git log -1 --format=%ct -- "$path" 2>/dev/null || true)"
  if [[ -z "$ts" ]]; then
    # Untracked or no history — fall back to filesystem mtime.
    if [[ "$(uname)" == "Darwin" ]]; then
      ts="$(stat -f %m "$path")"
    else
      ts="$(stat -c %Y "$path")"
    fi
  fi
  echo "$ts"
}

# ──────────────────────────────────────────────────────────────────────────────
# Walk EN sources, compare against each locale
# ──────────────────────────────────────────────────────────────────────────────

declare -a STALE_ROWS=()
declare -a JSON_ROWS=()
total_checked=0
total_stale=0

while IFS= read -r -d '' en_file; do
  rel="${en_file#"$SRC_REL/"}"
  en_epoch="$(last_commit_epoch "$en_file")"
  if [[ -z "$en_epoch" ]]; then
    continue
  fi
  for loc in "${LOCALES[@]}"; do
    target="$I18N_REL/$loc/docusaurus-plugin-content-docs/current/$rel"
    total_checked=$(( total_checked + 1 ))
    if [[ ! -e "$target" ]]; then
      STALE_ROWS+=("$loc|$rel|MISSING|n/a|n/a")
      JSON_ROWS+=("{\"locale\":\"$loc\",\"file\":\"$rel\",\"status\":\"missing\",\"en_age_days\":null,\"locale_age_days\":null}")
      total_stale=$(( total_stale + 1 ))
      continue
    fi
    loc_epoch="$(last_commit_epoch "$target")"
    if [[ -z "$loc_epoch" ]]; then
      continue
    fi
    delta=$(( en_epoch - loc_epoch ))
    if (( delta > THRESHOLD_SECS )); then
      en_age=$(( (NOW_EPOCH - en_epoch) / 86400 ))
      loc_age=$(( (NOW_EPOCH - loc_epoch) / 86400 ))
      gap_days=$(( delta / 86400 ))
      STALE_ROWS+=("$loc|$rel|STALE|${en_age}d|${loc_age}d (gap ${gap_days}d)")
      JSON_ROWS+=("{\"locale\":\"$loc\",\"file\":\"$rel\",\"status\":\"stale\",\"en_age_days\":$en_age,\"locale_age_days\":$loc_age,\"gap_days\":$gap_days}")
      total_stale=$(( total_stale + 1 ))
    fi
  done
done < <(find "$SRC_REL" -type f \( -name "*.md" -o -name "*.mdx" \) -print0)

# ──────────────────────────────────────────────────────────────────────────────
# Render summary
# ──────────────────────────────────────────────────────────────────────────────

echo "[i18n-stale] checked=$total_checked stale=$total_stale threshold=${MAX_STALE_DAYS}d locales=${LOCALES[*]}"

if (( total_stale > 0 )); then
  printf '\nStale / missing translations:\n'
  printf '  %-7s  %-60s  %-8s  %s\n' "LOCALE" "FILE" "STATUS" "AGES (EN / locale)"
  for row in "${STALE_ROWS[@]}"; do
    IFS='|' read -r loc rel status en_age loc_age <<< "$row"
    printf '  %-7s  %-60s  %-8s  %s / %s\n' "$loc" "$rel" "$status" "$en_age" "$loc_age"
  done
fi

# ──────────────────────────────────────────────────────────────────────────────
# Markdown report
# ──────────────────────────────────────────────────────────────────────────────

if [[ -n "$OUT_MD" ]]; then
  mkdir -p "$(dirname "$OUT_MD")"
  {
    echo "# i18n stale-content report"
    echo
    echo "- Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "- Threshold: ${MAX_STALE_DAYS} days (SLA per TRANSLATION-WORKFLOW.md §3)"
    echo "- Locales checked: ${LOCALES[*]}"
    echo "- Source pages: $(find "$SRC_REL" -type f \( -name "*.md" -o -name "*.mdx" \) | wc -l | tr -d ' ')"
    echo "- Stale or missing: $total_stale"
    echo
    if (( total_stale == 0 )); then
      echo "All locales within SLA. No action required."
    else
      echo "| Locale | File | Status | EN last-modified | Locale last-modified |"
      echo "| --- | --- | --- | --- | --- |"
      for row in "${STALE_ROWS[@]}"; do
        IFS='|' read -r loc rel status en_age loc_age <<< "$row"
        echo "| $loc | \`$rel\` | $status | $en_age ago | $loc_age ago |"
      done
    fi
  } > "$OUT_MD"
  echo "[i18n-stale] markdown report written to $OUT_MD"
fi

# ──────────────────────────────────────────────────────────────────────────────
# JSON output (for GitHub Actions issue body)
# ──────────────────────────────────────────────────────────────────────────────

if [[ -n "$OUT_JSON" ]]; then
  mkdir -p "$(dirname "$OUT_JSON")"
  {
    printf '{"generated":"%s","threshold_days":%s,"checked":%s,"stale":%s,"rows":[' \
      "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$MAX_STALE_DAYS" "$total_checked" "$total_stale"
    sep=""
    for row in "${JSON_ROWS[@]}"; do
      printf '%s%s' "$sep" "$row"
      sep=","
    done
    printf ']}\n'
  } > "$OUT_JSON"
  echo "[i18n-stale] JSON report written to $OUT_JSON"
fi

# ──────────────────────────────────────────────────────────────────────────────
# Exit code
# ──────────────────────────────────────────────────────────────────────────────

if (( NO_FAIL == 1 )); then
  exit 0
fi

if (( total_stale >= FAIL_THRESHOLD )); then
  exit 1
fi

exit 0
