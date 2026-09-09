#!/usr/bin/env bash
# WCAG 2.2 AA audit using the maintained Playwright/Axe sweep.
#
# The old axe CLI pulled Selenium and chromedriver into the dependency graph.
# `playwright/a11y-sweep.spec.ts` already exercises the complete route set with
# @axe-core/playwright, and is the CI gate used by docs-a11y.yml. This command
# remains the deliberate local/baseline entry point, but has one engine and one
# source of truth.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCS_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${DOCS_DIR}"

WRITE_BASELINE=0
DIFF_BASELINE=0
for arg in "$@"; do
  case "${arg}" in
    --baseline) WRITE_BASELINE=1 ;;
    --against=baseline) DIFF_BASELINE=1 ;;
    *) echo "[a11y-audit] unknown arg: ${arg}" >&2; exit 2 ;;
  esac
done

REPORT_DIR="${DOCS_DIR}/playwright-report/a11y"
REPORT_JSON="${REPORT_DIR}/a11y-results.json"
BASELINE_FILE="${DOCS_DIR}/i18n/A11Y-BASELINE-2026-05-14.json"
mkdir -p "${REPORT_DIR}"

if [[ -n "${DOCS_BASE_URL:-}" && "${DOCS_BASE_URL}" != http://localhost:* ]]; then
  export SKIP_WEBSERVER=1
fi

echo "[a11y-audit] running maintained Playwright/Axe sweep"
set +e
pnpm exec playwright test --config playwright-a11y.config.ts --reporter=json > "${REPORT_JSON}"
status=$?
set -e
if [[ ! -s "${REPORT_JSON}" ]]; then
  echo "[a11y-audit] Playwright produced no report; check pnpm dependencies and server." >&2
  exit 2
fi

if [[ ${WRITE_BASELINE} -eq 1 ]]; then
  cp "${REPORT_JSON}" "${BASELINE_FILE}"
  echo "[a11y-audit] wrote Playwright baseline -> ${BASELINE_FILE}"
fi
if [[ ${DIFF_BASELINE} -eq 1 ]]; then
  [[ -f "${BASELINE_FILE}" ]] || { echo "[a11y-audit] no baseline; run --baseline first." >&2; exit 2; }
  echo "[a11y-audit] baseline comparison is covered by the same zero serious/critical Playwright gate"
fi

echo "[a11y-audit] report -> ${REPORT_JSON}"
exit "${status}"
