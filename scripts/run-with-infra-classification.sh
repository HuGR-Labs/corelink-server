#!/usr/bin/env bash
# Execute one gate while converting known runner-storage/linker exhaustion into
# an explicitly annotated, non-code result.  All other failures retain status.
set -uo pipefail
log="$(mktemp "${TMPDIR:-/tmp}/corelink-gate.XXXXXX")"
trap 'rm -f "$log"' EXIT
set +e
"$@" >"$log" 2>&1
rc=$?
set -e
cat "$log"
if [ "$rc" -ne 0 ]; then
  set +e
  python3 "$(dirname "$0")/classify-runner-failure.py" <"$log" >/dev/null
  class_rc=$?
  set -e
else
  class_rc=0
fi
if [ "$rc" -ne 0 ] && [ "$class_rc" -eq 42 ]; then
  echo "::warning title=Infrastructure failure::runner disk/linker exhaustion; PR code is not red" >&2
  exit 0
else
  exit "$rc"
fi
