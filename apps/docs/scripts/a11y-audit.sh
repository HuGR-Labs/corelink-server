#!/usr/bin/env bash
# WCAG 2.2 AA audit using the maintained Playwright/Axe sweep.
#
# The old axe CLI pulled Selenium and chromedriver into the dependency graph.
# `playwright/a11y-sweep.spec.ts` already exercises the complete route set with
# @axe-core/playwright, and is the CI gate used by docs-ci.yml. This command
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
if [[ ${WRITE_BASELINE} -eq 1 && ${DIFF_BASELINE} -eq 1 ]]; then
  echo "[a11y-audit] --baseline and --against=baseline are mutually exclusive." >&2
  exit 2
fi

REPORT_DIR="${DOCS_DIR}/playwright-report/a11y"
REPORT_JSON="${REPORT_DIR}/report.json"
REPORT_MD="${REPORT_DIR}/report.md"
BASELINE_FILE="${DOCS_DIR}/i18n/A11Y-BASELINE-2026-05-14.json"
mkdir -p "${REPORT_DIR}"
rm -f "${REPORT_JSON}" "${REPORT_MD}"

if [[ -n "${DOCS_BASE_URL:-}" && "${DOCS_BASE_URL}" != http://localhost:* ]]; then
  export SKIP_WEBSERVER=1
fi

echo "[a11y-audit] running maintained Playwright/Axe sweep"
export A11Y_AUDIT_REPORT="${REPORT_JSON}"
export A11Y_AUDIT_SUMMARY="${REPORT_MD}"
set +e
pnpm exec playwright test --config playwright-a11y.config.ts
status=$?
set -e
if [[ ! -s "${REPORT_JSON}" ]]; then
  echo "[a11y-audit] Playwright produced no report; check pnpm dependencies and server." >&2
  exit 2
fi

if [[ ${WRITE_BASELINE} -eq 1 ]]; then
  if [[ ${status} -ne 0 ]]; then
    echo "[a11y-audit] refusing to update baseline after a failed Playwright audit." >&2
    exit "${status}"
  fi
  pnpm exec tsx scripts/promote-a11y-baseline.ts "${REPORT_JSON}" "${BASELINE_FILE}"
  echo "[a11y-audit] wrote Playwright baseline -> ${BASELINE_FILE}"
fi
if [[ ${DIFF_BASELINE} -eq 1 ]]; then
  [[ -f "${BASELINE_FILE}" ]] || { echo "[a11y-audit] no baseline; run --baseline first." >&2; exit 2; }
  node - "${REPORT_JSON}" "${BASELINE_FILE}" <<'NODE'
const fs = require("node:fs");
const [currentPath, baselinePath] = process.argv.slice(2);
const current = JSON.parse(fs.readFileSync(currentPath, "utf8"));
const baseline = JSON.parse(fs.readFileSync(baselinePath, "utf8"));
const key = (result, violation) => `${result.route}|${violation.id}`;
const accepted = new Set(
  baseline.flatMap((result) =>
    result.violations
      .filter((violation) => violation.impact === "critical")
      .map((violation) => key(result, violation)),
  ),
);
const regressions = current.flatMap((result) =>
  result.violations
    .filter(
      (violation) =>
        violation.impact === "critical" && !accepted.has(key(result, violation)),
    )
    .map((violation) => `${result.route} :: ${violation.id} (${violation.help})`),
);
if (regressions.length > 0) {
  console.error("[a11y-audit] NEW CRITICAL violations vs baseline:");
  for (const regression of regressions) console.error(`  - ${regression}`);
  process.exit(1);
}
console.log("[a11y-audit] no new CRITICAL violations vs baseline.");
NODE
fi

echo "[a11y-audit] report -> ${REPORT_JSON}"
echo "[a11y-audit] summary -> ${REPORT_MD}"
exit "${status}"
