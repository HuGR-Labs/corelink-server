#!/usr/bin/env bash
# Fail-closed full-history census for the custom secret-shape rule.
#
# This is intentionally separate from the PR diff scan: only a complete,
# unmodified commit graph can establish whether the known historical positive
# is still detected.  It prints aggregate counts, never finding contents.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
config="${repo_root}/.gitleaks.toml"
target_ref="${1:-HEAD}"
tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/corelink-gitleaks-history.XXXXXX")"
report="${tmpdir}/report.json"
trap 'rm -rf -- "$tmpdir"' EXIT

fail() { echo "gitleaks history census: $*" >&2; exit 1; }
command -v gitleaks >/dev/null || fail "gitleaks is required"
[[ -f "$config" ]] || fail "missing .gitleaks.toml"

target_commit="$(git -C "$repo_root" rev-parse --verify --quiet "${target_ref}^{commit}")" \
  || fail "target ref is unavailable: ${target_ref}"
[[ "$(git -C "$repo_root" rev-parse --is-shallow-repository)" == false ]] \
  || fail "repository is shallow"
git_dir="$(git -C "$repo_root" rev-parse --git-dir)"
shallow_file="${git_dir}/shallow"
[[ ! -s "$shallow_file" ]] || fail "shallow boundary file is non-empty"
[[ ! -s "${git_dir}/info/grafts" ]] || fail "graft file is non-empty"
[[ -z "$(git -C "$repo_root" replace -l)" ]] || fail "replace refs are active"

commit_count="$(git -C "$repo_root" rev-list --count "$target_commit")"
[[ "$commit_count" -ge 100 ]] || fail "history graph has only ${commit_count} commits (minimum 100)"
known="reports/audits/2026-08-25-comprehensive-audit-and-verification.md"
git -C "$repo_root" cat-file -e "${target_commit}:${known}" \
  || fail "known historical positive is absent at ${target_ref}"
git -C "$repo_root" grep -q 'AUDIT_CHAIN_SIGNING_SEED_HEX[[:space:]]*=' \
  "${target_commit}" -- "$known" \
  || fail "known historical positive marker is absent at ${target_ref}"

set +e
gitleaks detect --source "$repo_root" --config "$config" \
  --enable-rule corelink-secret-shaped-hex --log-opts="$target_commit" \
  --redact --no-banner --exit-code 1 --report-format json --report-path "$report" \
  >"${tmpdir}/gitleaks.out" 2>&1
scan_rc=$?
set -e
[[ "$scan_rc" == 0 || "$scan_rc" == 1 ]] \
  || fail "gitleaks exited unexpectedly: ${scan_rc}"
[[ -s "$report" ]] || fail "gitleaks report is missing or empty"

python3 - "$report" "$known" "$scan_rc" <<'PY'
import json
import pathlib
import sys

report = pathlib.Path(sys.argv[1])
known = sys.argv[2]
scan_rc = int(sys.argv[3])
try:
    findings = json.loads(report.read_text(encoding="utf-8"))
except (OSError, json.JSONDecodeError) as exc:
    raise SystemExit(f"census report is unreadable: {exc}")
if not isinstance(findings, list):
    raise SystemExit("census report is not a finding list")
if any(item.get("RuleID") != "corelink-secret-shaped-hex" for item in findings):
    raise SystemExit("census included a rule outside corelink-secret-shaped-hex")
if not any(item.get("File", "").endswith(known) for item in findings):
    raise SystemExit("known historical positive is absent from the scanner report")
print(f"scan_exit={scan_rc}")
print(f"history_commit_count={len({item.get('Commit', '') for item in findings})}")
print(f"finding_count={len(findings)}")
print(f"path_count={len({item.get('File', '') for item in findings})}")
PY
