#!/usr/bin/env bash
# scripts/coverage.sh — R7-1 supply-quality rollup (a) test coverage report.
#
# Generates workspace-wide LLVM source-based coverage via cargo-llvm-cov,
# emitting:
#   - target/coverage/html/index.html  (browsable HTML report, uploaded as
#     a GHA artifact for 90 days by .github/workflows/coverage.yml).
#   - target/coverage/SUMMARY.txt      (per-crate %-covered table; surfaced
#     in PR sticky comment and pasted into the audit baseline).
#
# Default invocation runs `--workspace --no-default-features` to keep the
# build matrix consistent with the cas_foundation gate (the wasm32 worker
# target compiles WITHOUT host-only features). Override via `COV_FLAGS=...`
# when an explicit feature lane is required.
#
# Environment:
#   COV_FLAGS      Extra flags forwarded to `cargo llvm-cov` (default: empty).
#   COV_OUT        Output dir (default: target/coverage).
#
# Exit codes:
#   0  coverage report generated successfully.
#   1  cargo-llvm-cov missing OR coverage run failed.
#
# Charter: no clippy warnings (shell only); set -euo pipefail mandatory.
# Reference: scripts/fuzz-all.sh style.

set -euo pipefail

usage() {
    cat <<'USAGE'
coverage.sh — workspace test-coverage report (cargo-llvm-cov wrapper).

Usage:
  scripts/coverage.sh                 # full workspace, html + summary
  scripts/coverage.sh --summary-only  # skip html, summary.txt only
  scripts/coverage.sh --help          # this message

Environment:
  COV_FLAGS  extra flags appended to `cargo llvm-cov` (e.g. "--all-features")
  COV_OUT    output directory (default: target/coverage)

Outputs:
  $COV_OUT/html/index.html
  $COV_OUT/SUMMARY.txt
USAGE
}

case "${1:-}" in
    --help|-h)
        usage
        exit 0
        ;;
esac

SUMMARY_ONLY=0
if [[ "${1:-}" == "--summary-only" ]]; then
    SUMMARY_ONLY=1
fi

COV_OUT="${COV_OUT:-target/coverage}"
COV_FLAGS="${COV_FLAGS:-}"

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
    echo "error: cargo-llvm-cov not installed." >&2
    echo "  install: cargo install cargo-llvm-cov --locked" >&2
    exit 1
fi

mkdir -p "$COV_OUT"

echo "==> cargo llvm-cov --workspace --no-default-features (cleaning prior data)"
# shellcheck disable=SC2086
cargo llvm-cov clean --workspace

if [[ "$SUMMARY_ONLY" -eq 0 ]]; then
    echo "==> generating HTML report → $COV_OUT/html"
    # shellcheck disable=SC2086
    cargo llvm-cov --workspace --no-default-features --html \
        --output-dir "$COV_OUT/html" $COV_FLAGS
fi

echo "==> generating per-crate summary → $COV_OUT/SUMMARY.txt"
# shellcheck disable=SC2086
cargo llvm-cov report --workspace --summary-only $COV_FLAGS \
    > "$COV_OUT/SUMMARY.txt"

echo "==> coverage report ready:"
echo "    summary : $COV_OUT/SUMMARY.txt"
if [[ "$SUMMARY_ONLY" -eq 0 ]]; then
    echo "    html    : $COV_OUT/html/index.html"
fi
