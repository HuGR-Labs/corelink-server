#!/usr/bin/env bash
# Focused B-128 tests: exercise ci.sh's orchestration with stubbed validators.
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf -- "$WORK"' EXIT

mkdir -p "$WORK/bin"
cat >"$WORK/bin/python3" <<'PYTHON'
#!/usr/bin/env bash
if [ "${1:-}" = "-c" ]; then
    printf '1\n'
    exit 0
fi
if [ "${2:-}" = "scripts/classify-runner-failure.py" ]; then
    exit 1
fi
exit 0
PYTHON
chmod +x "$WORK/bin/python3"

run_ci() {
    PATH="$WORK/bin:$PATH" CORELINK_NO_SCCACHE=1 "$@"
}

normal_output="$WORK/normal.out"
if ! run_ci bash "$ROOT/scripts/ci.sh" --validators-only --fast >"$normal_output" 2>&1; then
    cat "$normal_output" >&2
    exit 1
fi
grep -q 'PASS: 15' "$normal_output"
grep -q 'FAIL: 0' "$normal_output"
grep -q 'INFRA: 0' "$normal_output"

readonly_results="$WORK/results"
: >"$readonly_results"
chmod 400 "$readonly_results"
test_script="$WORK/ci-unwritable-results.sh"
sed "s#RESULTS_FILE=\"\$(mktemp)\" || exit 2#RESULTS_FILE=\"$readonly_results\"#" \
    "$ROOT/scripts/ci.sh" >"$test_script"
chmod +x "$test_script"

failure_output="$WORK/failure.out"
set +e
run_ci bash "$test_script" --validators-only --fast >"$failure_output" 2>&1
failure_rc=$?
set -e
cat "$failure_output"
[ "$failure_rc" -eq 42 ]
grep -q 'INFRA:' "$failure_output"
! grep -q 'INFRA: 0' "$failure_output"
