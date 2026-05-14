#!/usr/bin/env bash
# scripts/build_rs_lint.sh — WI-S12-006 (reproducible builds).
#
# Pre-commit lint: scan every build.rs in the workspace for patterns that
# introduce non-deterministic timestamps into the build output.
#
# The banned patterns bypass SOURCE_DATE_EPOCH and embed wall-clock time or
# UNIX_EPOCH references directly — causing the 2-runner diff to spike.
#
# Exit codes:
#   0 — no violations found
#   1 — one or more violations found (commit should be blocked)
#
# Usage:
#   bash scripts/build_rs_lint.sh            # scan all build.rs files
#   bash scripts/build_rs_lint.sh --fix      # show sed fix hints (no auto-fix)
#   bash scripts/build_rs_lint.sh --verbose  # show each file checked
#
# See docs/build/reproducible.md §6 for the compliant SOURCE_DATE_EPOCH
# honour pattern.
#
# Ref: ADR-0015 §5 (Consequences — build.rs non-determinism residual).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

VERBOSE=0
FIX_HINTS=0
for arg in "$@"; do
    case "${arg}" in
        --verbose) VERBOSE=1 ;;
        --fix)     FIX_HINTS=1 ;;
    esac
done

# ── Banned patterns ────────────────────────────────────────────────────────────
# Each entry is a grep ERE pattern.  If found in any build.rs, the lint fails.
declare -a BANNED_PATTERNS=(
    "chrono::Local::now\(\)"
    "chrono::Utc::now\(\)"
    "SystemTime::now\(\)"
    "UNIX_EPOCH"
)

declare -a BANNED_LABELS=(
    "chrono::Local::now() embeds wall-clock time — use SOURCE_DATE_EPOCH"
    "chrono::Utc::now() embeds wall-clock time — use SOURCE_DATE_EPOCH"
    "SystemTime::now() embeds wall-clock time — use SOURCE_DATE_EPOCH"
    "UNIX_EPOCH used directly — use SOURCE_DATE_EPOCH env var instead"
)

# ── Collect all build.rs files ─────────────────────────────────────────────────
mapfile -t BUILD_RS_FILES < <(find "${WORKSPACE_ROOT}" \
    -name "build.rs" \
    -not -path "*/target/*" \
    -not -path "*/.git/*" \
    2>/dev/null | sort)

if [ "${VERBOSE}" -eq 1 ]; then
    echo "build_rs_lint: checking ${#BUILD_RS_FILES[@]} build.rs file(s)"
    for f in "${BUILD_RS_FILES[@]}"; do
        echo "  ${f#"${WORKSPACE_ROOT}/"}"
    done
fi

VIOLATIONS=0

for build_rs in "${BUILD_RS_FILES[@]}"; do
    rel_path="${build_rs#"${WORKSPACE_ROOT}/"}"
    for i in "${!BANNED_PATTERNS[@]}"; do
        pattern="${BANNED_PATTERNS[$i]}"
        label="${BANNED_LABELS[$i]}"
        if grep -nE "${pattern}" "${build_rs}" > /dev/null 2>&1; then
            VIOLATIONS=$((VIOLATIONS + 1))
            echo "::error file=${rel_path}::build_rs_lint: ${label}"
            grep -nE "${pattern}" "${build_rs}" | while IFS= read -r match; do
                lineno=$(echo "${match}" | cut -d: -f1)
                echo "  ${rel_path}:${lineno}: $(echo "${match}" | cut -d: -f2-)"
            done
            if [ "${FIX_HINTS}" -eq 1 ]; then
                echo ""
                echo "  FIX: Replace with SOURCE_DATE_EPOCH honour pattern:"
                echo "    let epoch = std::env::var(\"SOURCE_DATE_EPOCH\")"
                echo "        .ok()"
                echo "        .and_then(|v| v.parse::<i64>().ok())"
                echo "        .unwrap_or(0);"
                echo "    println!(\"cargo:rustc-env=BUILD_TIMESTAMP={epoch}\");"
                echo ""
            fi
        fi
    done
done

if [ "${VIOLATIONS}" -gt 0 ]; then
    echo ""
    echo "build_rs_lint: ${VIOLATIONS} violation(s) found."
    echo "See docs/build/reproducible.md §3.4 and ADR-0015 §5 for the"
    echo "SOURCE_DATE_EPOCH compliant pattern."
    exit 1
fi

if [ "${VERBOSE}" -eq 1 ]; then
    echo "build_rs_lint: OK — no violations found."
fi
exit 0
