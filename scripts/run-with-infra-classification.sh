#!/usr/bin/env bash
# Execute one gate, retain its status, and leave a machine-readable diagnosis.
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "$0")" && pwd -P)"
log="$(mktemp "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/corelink-gate.XXXXXX")"
artifact="${CORELINK_CLASSIFICATION_ARTIFACT:-${CORELINK_ARTIFACT_DIR:-${GITHUB_WORKSPACE:-$PWD}/artifacts/runner-classification}/classification.json}"
timeout_seconds="${CORELINK_GATE_TIMEOUT_SECONDS:-1500}"

cleanup_allowlisted() {
  local root="${CORELINK_CLEANUP_ROOT:-${GITHUB_WORKSPACE:-$PWD}}" timeout_s="${CORELINK_CLEANUP_TIMEOUT_SECONDS:-20}" path
  [ -d "$root" ] || return 0
  root="$(cd -- "$root" && pwd -P)" || return 0
  [ "$root" != "/" ] || return 0
  case "$timeout_s" in (''|*[!0-9]*|0) timeout_s=20 ;; esac
  local paths=("$root/target" "$root/.cargo-cache" "$root/.cache/corelink" "$root/.tmp/corelink-ci")
  local runner_temp=""
  if [ -n "${RUNNER_TEMP:-}" ] && [ -d "$RUNNER_TEMP" ]; then
    runner_temp="$(cd -- "$RUNNER_TEMP" && pwd -P)" || runner_temp=""
    [ -n "$runner_temp" ] && paths+=("$runner_temp/corelink-ci")
  fi
  for path in "${paths[@]}"; do
    case "$path" in
      "$root/target"|"$root/.cargo-cache"|"$root/.cache/corelink"|"$root/.tmp/corelink-ci") ;;
      "$runner_temp/corelink-ci") ;;
      *) continue ;;
    esac
    [ -e "$path" ] || continue
    [ -L "$path" ] && continue
    timeout --signal=TERM "$timeout_s" rm -rf -- "$path" ||
      printf 'cleanup: bounded removal failed for allowlisted path %s\n' "$path" >&2
  done
}

finish() {
  local original_rc="$1" classify_rc=0 classification="FAILURE" summary_file="${GITHUB_STEP_SUMMARY:-}"
  trap - EXIT INT TERM
  set +e
  python3 "$script_dir/classify-runner-failure.py" --status "$original_rc" --json-output "$artifact" <"$log" >"$log.classification" 2>&1
  classify_rc=$?
  cat "$log"
  cat "$log.classification"
  classification="$(sed -n 's/^classification: //p' "$log.classification" | head -n 1)"
  [ -n "$classification" ] || classification="FAILURE"
  if [ -n "$summary_file" ]; then
    {
      printf '\n### Runner gate classification\n'
      printf '%s\n' "- Classification: \`$classification\`"
      printf '%s\n' "- Original exit: \`$original_rc\` (preserved)"
      printf '%s\n' "- Artifact: \`$artifact\`"
    } >>"$summary_file" || true
  fi
  if [ "$original_rc" -eq 124 ] && grep -q '^GATE_TIMEOUT ' "$log"; then
    echo "::error title=Gate timeout::the wrapped gate exceeded ${timeout_seconds}s" >&2
  elif [ "$classify_rc" -eq 42 ]; then
    echo "::warning title=Infrastructure failure::${classification}; original gate status preserved" >&2
  elif [ "$classify_rc" -eq 43 ]; then
    echo "::warning title=Runner cancellation::${classification}; original gate status preserved" >&2
  fi
  cleanup_allowlisted || true
  rm -f -- "$log" "$log.classification"
  exit "$original_rc"
}

trap 'finish "$?"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
set +e
python3 "$script_dir/exec-with-timeout.py" "$timeout_seconds" "$@" >"$log" 2>&1
gate_rc=$?
set -e
exit "$gate_rc"
