#!/usr/bin/env bash
# Safe, reproducible census for the custom raw-hex rule.
#
# It deliberately prints only counts, never finding text, paths, commits, or
# secret bytes. The scheduled workflow's full-history checkout has one target
# ref, rather than every locally-fetched PR branch; use that same model here.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
target_ref="${1:-origin/main}"
config="${repo_root}/.gitleaks.toml"
tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/corelink-gitleaks-history.XXXXXX")"
report="${tmpdir}/report.json"
trap 'test -n "${tmpdir:-}" && test -d "$tmpdir" && rm -rf -- "$tmpdir"' EXIT

command -v gitleaks >/dev/null || { echo "gitleaks is required" >&2; exit 2; }
test -f "$config" || { echo "missing .gitleaks.toml" >&2; exit 2; }
git -C "$repo_root" rev-parse --verify --quiet "${target_ref}^{commit}" >/dev/null \
  || { echo "target ref is unavailable: ${target_ref}" >&2; exit 2; }

set +e
gitleaks detect --source "$repo_root" --config "$config" \
  --enable-rule corelink-secret-shaped-hex --log-opts="$target_ref" \
  --redact --no-banner --exit-code 1 --report-format json --report-path "$report" \
  >"${tmpdir}/gitleaks.out" 2>&1
scan_rc=$?
set -e

python3 - "$report" "$scan_rc" <<'PY'
import json, sys
from collections import Counter

report, scan_rc = sys.argv[1:]
try:
    findings = json.load(open(report))
except (OSError, json.JSONDecodeError) as exc:
    raise SystemExit(f"census report is unreadable: {exc}")
if any(finding.get("RuleID") != "corelink-secret-shaped-hex" for finding in findings):
    raise SystemExit("census included a rule outside corelink-secret-shaped-hex")
if int(scan_rc) not in (0, 1):
    raise SystemExit(f"gitleaks exited unexpectedly: {scan_rc}")
print(f"scan_exit={scan_rc}")
print(f"finding_count={len(findings)}")
print(f"path_count={len({finding.get('File', '') for finding in findings})}")
print(f"commit_count={len({finding.get('Commit', '') for finding in findings})}")
PY
