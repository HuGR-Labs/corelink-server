#!/usr/bin/env bash
# Bounded static contract for scripts/ci.sh's per-invocation Cargo target.
# This guard deliberately does not run Cargo or the CI runner.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CI_SCRIPT="${ROOT}/scripts/ci.sh"

bash -n "$CI_SCRIPT"

HELP_TMP="$(mktemp -d "${TMPDIR:-/tmp}/corelink-ci-help.XXXXXX")"
HELP_RUNNER="${HELP_TMP}/runner"
HELP_CALLER="${HELP_TMP}/caller"
mkdir -p "$HELP_RUNNER" "$HELP_CALLER"
printf '%s\n' caller-owned >"${HELP_CALLER}/sentinel"
trap 'rm -rf -- "$HELP_TMP"' EXIT

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
require_pattern 'set -m' 'owned process-group launch'
require_pattern 'set \+m' 'validator process-group inheritance'
# shellcheck disable=SC2016  # grep must inspect literal shell variables.
require_pattern 'kill -TERM -- "-\$pgid"' 'process-group termination'
# shellcheck disable=SC2016
require_pattern 'wait "\$RUST_PID"' 'Rust wait before PID clear'
# shellcheck disable=SC2016
require_pattern 'wait "\$VAL_PID"' 'validator wait before PID clear'
require_pattern 'RUST_PID=""' 'Rust PID clearing'
require_pattern 'VAL_PID=""' 'validator PID clearing'

if grep -Eq '(^|[^[:alnum:]_])pgrep([^[:alnum:]_]|$)' "$CI_SCRIPT"; then
    echo 'ci target isolation: cleanup must not depend on pgrep process-tree races' >&2
    exit 1
fi

help_line="$(grep -nF "sed -n '2,/^$/p' \"\$0\"" "$CI_SCRIPT" | head -n 1 | cut -d: -f1)"
log_line="$(grep -n '^LOG_DIR=' "$CI_SCRIPT" | head -n 1 | cut -d: -f1)"
target_line="$(grep -n '^create_ci_target_dir ||' "$CI_SCRIPT" | head -n 1 | cut -d: -f1)"
if [ -z "$help_line" ] || [ -z "$log_line" ] || [ -z "$target_line" ] \
    || [ "$help_line" -ge "$log_line" ] \
    || [ "$help_line" -ge "$target_line" ]; then
    echo 'ci target isolation: --help must parse before logs/target setup' >&2
    exit 1
fi

# Lifecycle check: --help must not create either a runner temp child or a
# target/log directory, and an ambient caller target must remain untouched.
CARGO_TARGET_DIR="$HELP_CALLER" RUNNER_TEMP="$HELP_RUNNER" "$CI_SCRIPT" --help >/dev/null
test -f "${HELP_CALLER}/sentinel"
if find "$HELP_RUNNER" -mindepth 1 -maxdepth 1 -print -prune | grep -q .; then
    echo 'ci target isolation: --help created a temp child' >&2
    exit 1
fi

# The cleanup path must never be widened to an ambient or caller-owned target.
if grep -Eq 'rm -rf -- .*CARGO_TARGET_DIR|rm -rf -- .*CI_CALLER_TARGET_DIR' "$CI_SCRIPT"; then
    echo 'ci target isolation: cleanup must not remove a caller-provided target' >&2
    exit 1
fi

echo 'ci target isolation: PASS (static+lifecycle contract; Cargo/CI gates not executed)'
