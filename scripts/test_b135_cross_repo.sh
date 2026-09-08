#!/usr/bin/env bash
# B-135 receipt and backlog mutation suite. No network calls or credentials.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHECKER="${ROOT}/scripts/check_b135_cross_repo.py"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/b135-cross-repo.XXXXXX")"
trap 'rm -rf -- "${TMP}"' EXIT

mkdir -p "${TMP}/scripts" "${TMP}/docs/campaigns/remediation"
cp "${CHECKER}" "${TMP}/scripts/check_b135_cross_repo.py"
cp "${ROOT}/BACKLOG.md" "${TMP}/BACKLOG.md"
cp "${ROOT}/docs/campaigns/remediation/B-135-corelink-runners-closure.md" \
  "${TMP}/docs/campaigns/remediation/B-135-corelink-runners-closure.md"

expect_green() {
  python3 "${TMP}/scripts/check_b135_cross_repo.py" --root "${TMP}" >/dev/null
  echo "B135-$1: PASS"
}

expect_red() {
  if python3 "${TMP}/scripts/check_b135_cross_repo.py" --root "${TMP}" >/dev/null 2>&1; then
    echo "B135-$1: mutation survived" >&2
    exit 1
  fi
  echo "B135-$1: PASS (mutation red)"
}

expect_green baseline

sed -i.bak 's/d8124b1ab89cf6afb08682442e94c4f4d18c6ba8/d8124b1ab89cf6afb08682442e94c4f4d18c6ba9/' \
  "${TMP}/docs/campaigns/remediation/B-135-corelink-runners-closure.md"
expect_red tested-head
cp "${TMP}/docs/campaigns/remediation/B-135-corelink-runners-closure.md.bak" \
  "${TMP}/docs/campaigns/remediation/B-135-corelink-runners-closure.md"

sed -i.bak 's/ec9b6d69dbb1f3bd64ef4f9a4ce7e9d9100e69c1/ec9b6d69dbb1f3bd64ef4f9a4ce7e9d9100e69c2/' \
  "${TMP}/docs/campaigns/remediation/B-135-corelink-runners-closure.md"
expect_red merge-commit
cp "${TMP}/docs/campaigns/remediation/B-135-corelink-runners-closure.md.bak" \
  "${TMP}/docs/campaigns/remediation/B-135-corelink-runners-closure.md"

sed -i.bak 's/status: done/status: open/' "${TMP}/BACKLOG.md"
expect_red backlog-status
cp "${ROOT}/BACKLOG.md" "${TMP}/BACKLOG.md"

sed -i.bak '/pull_request` trigger/d' "${TMP}/docs/campaigns/remediation/B-135-corelink-runners-closure.md"
expect_red missing-behavior
cp "${ROOT}/docs/campaigns/remediation/B-135-corelink-runners-closure.md" \
  "${TMP}/docs/campaigns/remediation/B-135-corelink-runners-closure.md"

printf '\ncredential: ghp_NOTAREALCREDENTIAL1234567890\n' >> \
  "${TMP}/docs/campaigns/remediation/B-135-corelink-runners-closure.md"
expect_red credential-leak

echo "B-135 receipt mutations: all controls passed"
