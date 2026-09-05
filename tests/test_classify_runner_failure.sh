#!/usr/bin/env bash
set -euo pipefail
s="$(cd "$(dirname "$0")/.." && pwd)/scripts/classify-runner-failure.py"
set +e
printf '%s\n' 'cargo: could not create incremental directory: No space left on device (os error 28)' | python3 "$s" >/dev/null; rc=$?
set -e; [ "$rc" -eq 42 ]
set +e
printf '%s\n' 'collect2: ld failed with Bus error' | python3 "$s" >/dev/null; rc=$?
set -e; [ "$rc" -eq 42 ]
printf '%s\n' 'assertion failed in PR code' | python3 "$s" | grep -qx CODE_FAILURE
printf '%s\n' 'test fixture says bus error in test output' | python3 "$s" | grep -qx CODE_FAILURE
printf '%s\n' 'collect2: ld: Bus error; error[E0308]: code mismatch' | python3 "$s" | grep -qx CODE_FAILURE
wrapper="$(dirname "$s")/run-with-infra-classification.sh"
chmod +x "$wrapper"
set +e
"$wrapper" sh -c 'echo "cargo: could not create incremental directory: No space left on device (os error 28)" >&2; exit 1' >/dev/null
rc=$?
set -e
[ "$rc" -eq 0 ]
set +e
"$wrapper" sh -c 'echo "assertion failed" >&2; exit 3' >/dev/null
rc=$?
set -e
[ "$rc" -eq 3 ]
echo 'runner failure classification tests: PASS'
