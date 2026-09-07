#!/usr/bin/env bash
# Bounded static contract for scripts/ci.sh's per-invocation Cargo target.
# This guard deliberately does not run Cargo or the CI runner.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CI_SCRIPT="${ROOT}/scripts/ci.sh"

bash -n "$CI_SCRIPT"

require_pattern() {
    local pattern="$1"
    local description="$2"
    if ! grep -Eq "$pattern" "$CI_SCRIPT"; then
        echo "ci target isolation: missing ${description}" >&2
        exit 1
    fi
}

require_pattern 'RUNNER_TEMP' 'RUNNER_TEMP preference'
require_pattern 'mktemp -d .*corelink-ci-target\.XXXXXX' 'unique mktemp target'
# shellcheck disable=SC2016  # grep must inspect the literal shell variable.
require_pattern 'export CARGO_TARGET_DIR="\$CI_TARGET_DIR"' 'Cargo target export'
require_pattern 'trap cleanup_ci EXIT' 'EXIT cleanup trap'
require_pattern "trap 'on_ci_signal (HUP|INT|TERM)'" 'signal cleanup traps'
require_pattern 'CI_TARGET_DIR_CREATED' 'owned-directory cleanup guard'
require_pattern 'target/ci-logs' 'preserved CI log directory'

# The cleanup path must never be widened to an ambient or caller-owned target.
if grep -Eq 'rm -rf -- .*CARGO_TARGET_DIR|rm -rf -- .*CI_CALLER_TARGET_DIR' "$CI_SCRIPT"; then
    echo 'ci target isolation: cleanup must not remove a caller-provided target' >&2
    exit 1
fi

echo 'ci target isolation: PASS (static contract; Cargo/CI not executed)'
