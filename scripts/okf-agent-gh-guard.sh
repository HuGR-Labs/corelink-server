#!/usr/bin/env bash
set -u
if [[ -n "${OKF_GUARD_MARKER-}" ]]; then
  printf 'gh %s\n' "$*" >> "$OKF_GUARD_MARKER" 2>/dev/null || true
fi
echo "OKF guard: denied gh (PR/API operation)" >&2
exit 97
