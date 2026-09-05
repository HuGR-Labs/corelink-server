#!/usr/bin/env bash
# Execute one gate while annotating known runner-storage/linker exhaustion.
# Classification never changes the command's exit status: an infrastructure
# failure is still non-zero, so the workflow cannot report a false green.
set -uo pipefail
log="$(mktemp "${TMPDIR:-/tmp}/corelink-gate.XXXXXX")"
trap 'rm -f "$log"' EXIT
set +e
# Keep a bounded wall clock even when the job-level timeout is not enforced.
# The default leaves room under corelink-server.yml's 30-minute job timeout;
# tests use a fraction of a second.
timeout_seconds="${CORELINK_GATE_TIMEOUT_SECONDS:-1500}"
python3 "$(dirname "$0")/exec-with-timeout.py" "$timeout_seconds" "$@" >"$log" 2>&1
rc=$?
set -e
cat "$log"
if [ "$rc" -ne 0 ]; then
  set +e
  classification="$(python3 "$(dirname "$0")/classify-runner-failure.py" <"$log")"
  class_rc=$?
  set -e
else
  classification="PASS"
  class_rc=0
fi
printf 'classification: %s\n' "$classification"
if [ "$rc" -eq 124 ] && grep -q '^GATE_TIMEOUT ' "$log"; then
  echo "::error title=Gate timeout::the wrapped gate exceeded ${timeout_seconds}s" >&2
elif [ "$rc" -ne 0 ] && [ "$class_rc" -eq 42 ]; then
  echo "::warning title=Infrastructure failure::runner disk/linker exhaustion; original gate status preserved" >&2
fi
exit "$rc"
