#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
s="$root/scripts/classify-runner-failure.py"
wrapper="$root/scripts/run-with-infra-classification.sh"
timeout_runner="$root/scripts/exec-with-timeout.py"

python3 -m py_compile "$s" "$timeout_runner"
set +e
printf '%s\n' 'cargo: could not create incremental directory: No space left on device (os error 28)' | python3 "$s" >/dev/null; rc=$?
set -e; [ "$rc" -eq 42 ]
set +e
printf '%s\n' 'collect2: ld failed with Bus error' | python3 "$s" >/dev/null; rc=$?
set -e; [ "$rc" -eq 42 ]
printf '%s\n' 'assertion failed in PR code' | python3 "$s" | grep -qx CODE_FAILURE
printf '%s\n' 'test fixture says bus error in test output' | python3 "$s" | grep -qx CODE_FAILURE
printf '%s\n' 'collect2: ld: Bus error; error[E0308]: code mismatch' | python3 "$s" | grep -qx CODE_FAILURE
chmod +x "$wrapper"

# The wrapper must preserve the original status for infrastructure failures;
# annotation is additive and never a green override.
set +e
infra_output="$($wrapper sh -c 'echo "cargo: could not create incremental directory: No space left on device (os error 28)" >&2; exit 7' 2>&1)"
rc=$?
set -e
[ "$rc" -eq 7 ]
grep -q '^classification: INFRA_FAILURE ' <<<"$infra_output"
grep -q 'original gate status preserved' <<<"$infra_output"

set +e
linker_output="$($wrapper sh -c 'echo "collect2: ld failed with Bus error" >&2; exit 8' 2>&1)"
rc=$?
set -e
[ "$rc" -eq 8 ]
grep -q '^classification: INFRA_FAILURE ' <<<"$linker_output"
grep -q 'original gate status preserved' <<<"$linker_output"

# Ordinary code failures remain non-zero and are not relabeled as infra.
set +e
code_output="$($wrapper sh -c 'echo "assertion failed" >&2; exit 3' 2>&1)"
rc=$?
set -e
[ "$rc" -eq 3 ]
grep -q '^classification: CODE_FAILURE$' <<<"$code_output"
if grep -q 'Infrastructure failure' <<<"$code_output"; then exit 1; fi

# A mixed diagnostic is code-owned even when it contains infra-looking text.
set +e
mixed_output="$($wrapper sh -c 'echo "error[E0308]: code mismatch; collect2: ld failed with Bus error" >&2; exit 9' 2>&1)"
rc=$?
set -e
[ "$rc" -eq 9 ]
grep -q '^classification: CODE_FAILURE$' <<<"$mixed_output"
if grep -q 'classification: INFRA_FAILURE' <<<"$mixed_output"; then exit 1; fi

# Signal exits are converted to the shell convention and remain non-zero.
set +e
signal_output="$($wrapper sh -c 'kill -TERM $$' 2>&1)"
rc=$?
set -e
[ "$rc" -eq 143 ]
grep -q '^classification: CODE_FAILURE$' <<<"$signal_output"

# Timeout is bounded, distinct from infra, and still non-zero.
start=$(date +%s)
set +e
timeout_output="$(CORELINK_GATE_TIMEOUT_SECONDS=1 "$wrapper" sh -c 'sleep 30' 2>&1)"
rc=$?
set -e
elapsed=$(( $(date +%s) - start ))
[ "$rc" -eq 124 ]
[ "$elapsed" -le 5 ]
grep -q '^GATE_TIMEOUT ' <<<"$timeout_output"
grep -q '^classification: CODE_FAILURE$' <<<"$timeout_output"

# The timeout helper itself rejects malformed bounds instead of running an
# unbounded command.
set +e
python3 "$timeout_runner" 0 sh -c true >/dev/null 2>&1
rc=$?
set -e
[ "$rc" -eq 2 ]
echo 'runner failure classification tests: PASS'
