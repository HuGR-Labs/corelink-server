#!/usr/bin/env bash
# Validate every Prometheus rule file with the pinned promtool on PATH.
set -euo pipefail

shopt -s nullglob
files=(dashboards/alerts/*.yml)
if (( ${#files[@]} == 0 )); then
  echo "::error::'dashboards/alerts/*.yml' matched ZERO files." >&2
  echo "This gate will not report green over an empty match set." >&2
  exit 1
fi

echo "rule files to validate: ${#files[@]}"
printf '  %s\n' "${files[@]}"
promtool --version
promtool check rules "${files[@]}"
