#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd -P)"
detector="$root/scripts/classify-runner-failure.py"
wrapper="$root/scripts/run-with-infra-classification.sh"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/i1670.XXXXXX")"
trap 'rm -rf -- "$tmp"' EXIT

python3 -m py_compile "$detector" "$root/scripts/exec-with-timeout.py"

assert_classification() {
  local expected="$1" status="$2" text="$3" actual
  set +e
  actual="$(printf '%s\n' "$text" | python3 "$detector" --status "$status" 2>/dev/null)"
  set -e
  grep -qx "classification: $expected" <<<"$actual"
}

# Contract: disk-full, test failure, and success are disjoint.
assert_classification ENOSPC 7 'cargo: could not create target: No space left on device (os error 28)'
assert_classification TEST_FAILURE 7 'test parser ... FAILED; assertion failed'
assert_classification SUCCESS 0 'test parser ... ok; ENOSPC is only a fixture word'
assert_classification LINKER_FAILURE 1 'collect2: fatal error: ld terminated with signal 7 [Bus error]'
assert_classification TEST_FAILURE 9 'error[E0308]; collect2: ld failed with Bus error'

artifact="${CORELINK_CLASSIFICATION_ARTIFACT:-${CORELINK_ARTIFACT_DIR:-$tmp/artifact}/classification.json}"
summary="$tmp/summary.md"
set +e
output="$(CORELINK_CLASSIFICATION_ARTIFACT="$artifact" GITHUB_STEP_SUMMARY="$summary" \
  CORELINK_CLEANUP_ROOT="$tmp/workspace" "$wrapper" sh -c \
  'echo "cargo: write failed: No space left on device (os error 28)" >&2; mkdir -p "$PWD/target"; exit 7' 2>&1)"
status=$?
set -e
[ "$status" -eq 7 ]
grep -q '^classification: ENOSPC$' <<<"$output"
grep -q 'original exit 7 is preserved' <<<"$output"
python3 - "$artifact" <<'PY'
import json, sys
payload = json.load(open(sys.argv[1], encoding="utf-8"))
assert payload["classification"] == "ENOSPC"
assert payload["exit_code"] == 7
assert payload["original_exit_code"] == 7
assert payload["original_status_preserved"] is True
PY
grep -q 'Classification:.*ENOSPC' "$summary"
grep -q '^::warning title=Infrastructure failure::ENOSPC; original gate status preserved$' <<<"$output"

# Cleanup is bounded and only removes its fixed allowlist.  The sentinel stays.
mkdir -p "$tmp/workspace/target" "$tmp/workspace/cache" "$tmp/workspace/temp"
printf keep >"$tmp/workspace/cache/sentinel"
printf keep >"$tmp/workspace/temp/sentinel"
CORELINK_CLEANUP_ROOT="$tmp/workspace" CORELINK_CLEANUP_TIMEOUT_SECONDS=2 \
  "$wrapper" sh -c 'exit 0' >/dev/null
[ ! -e "$tmp/workspace/target" ]
[ -e "$tmp/workspace/cache/sentinel" ]
[ -e "$tmp/workspace/temp/sentinel" ]

# Timeout remains non-zero and cannot be relabeled as a test or disk failure.
set +e
timeout_output="$(CORELINK_GATE_TIMEOUT_SECONDS=1 "$wrapper" sh -c 'sleep 30' 2>&1)"
status=$?
set -e
[ "$status" -eq 124 ]
grep -q '^classification: TIMEOUT$' <<<"$timeout_output"
grep -q '^GATE_TIMEOUT ' <<<"$timeout_output"

echo 'i1670 runner classification contract: PASS'
