#!/usr/bin/env bash
# Stable state-file digest used by every OKF workflow shell. GitHub Actions
# starts each `run:` block in a fresh process, so this must be versioned rather
# than defined as a step-local function.
set -euo pipefail

state_file=${1:?usage: okf-state-sha.sh STATE_FILE}
if command -v sha256sum >/dev/null 2>&1; then
  sha256sum "$state_file" | awk '{print $1}'
else
  shasum -a 256 "$state_file" | awk '{print $1}'
fi
