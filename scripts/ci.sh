#!/usr/bin/env bash
# scripts/ci.sh — canonical local CI runner (parallel).
#
# Single entrypoint for the full pre-merge gate set: Rust (build / clippy /
# test-compile / wasm32) + Python validators + Shell sanity gates. Runs the
# two big groups concurrently and per-validator inside each group also in
# parallel. Wall-clock ≈ max(rust-pipeline, slowest-validator) instead of
# the sum.
#
# Usage:
#   scripts/ci.sh             # full CI (default)
#   scripts/ci.sh --fast      # skip cargo test-compile + wasm32 (build + clippy + validators only)
#   scripts/ci.sh --rust-only # cargo gates only (no validators)
#   scripts/ci.sh --validators-only  # validators only (no cargo)
#
# Exit codes:
#   0  → all gates passed
#   1+ → number of failed gates
#
# Logs: per-gate stdout/stderr captured under `target/ci-logs/<gate>.log`
# (preserved across runs; overwritten each invocation).
#
# Charter alignment: per `docs/internal/TECHLEAD-CHECKLIST.md` §L1 + §L6.
# Memory feedback: parallel-by-default per user mandate 2026-05-26.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

LOG_DIR="target/ci-logs"
mkdir -p "$LOG_DIR"

# Compile cache. sccache only helps when incremental is OFF (it can't cache
# `-C incremental`), so we enable it HERE — in the CI regime — by exporting
# RUSTC_WRAPPER + CARGO_INCREMENTAL=0. This dedupes the redundant recompiles
# across the build/clippy/test gates and caches across CI runs. The dev loop
# (plain `cargo build`/`nextest`) stays incremental and does NOT use sccache.
# Hard-capped at 4G (this box is disk-tight) so the cache can never overflow.
# Bypass with CORELINK_NO_SCCACHE=1 (e.g. when chasing a cache-masked miscompile).
if [ "${CORELINK_NO_SCCACHE:-0}" != 1 ] && command -v sccache >/dev/null 2>&1; then
    export RUSTC_WRAPPER=sccache
    export CARGO_INCREMENTAL=0
    export SCCACHE_CACHE_SIZE="${SCCACHE_CACHE_SIZE:-4G}"
    sccache --start-server >/dev/null 2>&1 || true
    sccache --zero-stats >/dev/null 2>&1 || true
    echo "[sccache active — cap ${SCCACHE_CACHE_SIZE}, incremental off]" >&2
fi

# --- arg parse ----------------------------------------------------------------
RUN_RUST=1
RUN_VALIDATORS=1
RUN_TEST_COMPILE=1
RUN_WASM32=1
for arg in "$@"; do
    case "$arg" in
        --fast)
            RUN_TEST_COMPILE=0
            RUN_WASM32=0
            ;;
        --rust-only)
            RUN_VALIDATORS=0
            ;;
        --validators-only)
            RUN_RUST=0
            ;;
        -h|--help)
            sed -n '2,/^$/p' "$0"
            exit 0
            ;;
        *)
            echo "unknown arg: $arg" >&2
            exit 2
            ;;
    esac
done

# --- gate registry ------------------------------------------------------------
# Format: "name|command"
# `name` becomes the log filename + status line label.

RUST_GATES=(
    "cargo-build|cargo build --workspace"
    "cargo-clippy|cargo clippy --workspace --all-targets -- -D warnings"
)
if [ "$RUN_TEST_COMPILE" = 1 ]; then
    # nextest RUNS the suite (not just --no-run compile) and parallelises test
    # execution across all cores via the `ci` profile (.config/nextest.toml).
    # Falls back to `cargo test` if nextest is unavailable. This is strictly
    # MORE rigorous than the old --no-run gate, which never executed a test.
    if command -v cargo-nextest >/dev/null 2>&1; then
        RUST_GATES+=("cargo-nextest|cargo nextest run --workspace --profile ci")
    else
        RUST_GATES+=("cargo-test|cargo test --workspace")
    fi
fi
if [ "$RUN_WASM32" = 1 ]; then
    RUST_GATES+=("cargo-wasm32|cargo build --target wasm32-unknown-unknown -p corelink-clerk-cf")
fi

VALIDATOR_GATES=(
    "validate-specs|python3 scripts/validate_specs.py"
    "validate-references|python3 scripts/validate_references.py"
    "validate-inv-inheritance|python3 scripts/validate_inv_inheritance.py"
    "validate-inv-promotion|python3 scripts/validate_inv_promotion.py"
    "validate-canonical|python3 scripts/validate_canonical_consistency.py"
    "validate-dashboards|python3 scripts/validate_dashboards.py"
    "validate-dpia|python3 scripts/validate_dpia.py"
    "validate-secrets-matrix|python3 scripts/validate_secrets_matrix.py"
    "validate-slo-instrumentation|python3 scripts/validate_slo_instrumentation.py"
    "validate-sub-processors|python3 scripts/validate_sub_processors.py"
    "check-migrations-additive|python3 scripts/check_migrations_additive.py"
    "check-error-taxonomy|python3 scripts/check_error_taxonomy.py"
    "check-tla-obligations|python3 scripts/check_tla_obligations.py"
    "check-cost-regression|python3 scripts/check_cost_regression.py"
    "check-proptest-density|bash scripts/check_proptest_density_gate.sh"
)
# Skipped from default ci.sh because they require runtime args / external creds:
#   - validate_privacy_notice.py <notice-dir>  → run per-notice in legal/ pipeline.
#   - check_ac_infra.sh <env>                  → pre-deploy gate; needs CF_API_TOKEN.

# --- helpers ------------------------------------------------------------------
RESULTS_FILE="$(mktemp)"
trap 'rm -f "$RESULTS_FILE"' EXIT

run_gate() {
    # Args: name, command, group
    local name="$1"
    local cmd="$2"
    local group="$3"
    local log="$LOG_DIR/${name}.log"
    local start_ns end_ns dur_ms

    start_ns=$(python3 -c 'import time; print(int(time.time_ns()))')
    if bash -c "$cmd" >"$log" 2>&1; then
        end_ns=$(python3 -c 'import time; print(int(time.time_ns()))')
        dur_ms=$(( (end_ns - start_ns) / 1000000 ))
        printf '%s|%s|PASS|%s|%s\n' "$group" "$name" "$dur_ms" "$log" >>"$RESULTS_FILE"
        printf '  \033[32m✔\033[0m %-32s  %5d ms\n' "$name" "$dur_ms" >&2
    else
        end_ns=$(python3 -c 'import time; print(int(time.time_ns()))')
        dur_ms=$(( (end_ns - start_ns) / 1000000 ))
        printf '%s|%s|FAIL|%s|%s\n' "$group" "$name" "$dur_ms" "$log" >>"$RESULTS_FILE"
        printf '  \033[31m✘\033[0m %-32s  %5d ms  (see %s)\n' "$name" "$dur_ms" "$log" >&2
    fi
}

run_rust_pipeline() {
    # Rust gates run serially within this group (they share cargo target dir;
    # cargo itself parallelises internally per build).
    for entry in "${RUST_GATES[@]}"; do
        local name="${entry%%|*}"
        local cmd="${entry#*|}"
        run_gate "$name" "$cmd" rust
    done
}

run_validator_pool() {
    # Validators run in parallel: max 8 concurrent (12-core box; leave room
    # for the rust group if running concurrently). xargs -P-style via &/wait.
    local max_concurrent=8
    local running=0
    for entry in "${VALIDATOR_GATES[@]}"; do
        local name="${entry%%|*}"
        local cmd="${entry#*|}"
        run_gate "$name" "$cmd" validator &
        running=$(( running + 1 ))
        if [ "$running" -ge "$max_concurrent" ]; then
            wait -n
            running=$(( running - 1 ))
        fi
    done
    wait
}

# --- run groups concurrently --------------------------------------------------
echo "─── CI runner — parallel mode ───" >&2
echo "Logs: $LOG_DIR/" >&2
echo "" >&2
START_NS=$(python3 -c 'import time; print(int(time.time_ns()))')

if [ "$RUN_RUST" = 1 ]; then
    echo "[rust pipeline launched]" >&2
    run_rust_pipeline &
    RUST_PID=$!
fi
if [ "$RUN_VALIDATORS" = 1 ]; then
    echo "[validator pool launched]" >&2
    run_validator_pool &
    VAL_PID=$!
fi

[ "$RUN_RUST" = 1 ] && wait "$RUST_PID"
[ "$RUN_VALIDATORS" = 1 ] && wait "$VAL_PID"

END_NS=$(python3 -c 'import time; print(int(time.time_ns()))')
TOTAL_MS=$(( (END_NS - START_NS) / 1000000 ))

# --- summary ------------------------------------------------------------------
echo "" >&2
echo "─── Summary ───" >&2
# grep -c exits 1 when match count is 0; capture exit and force a clean integer.
PASS_COUNT=$(grep -c '|PASS|' "$RESULTS_FILE" 2>/dev/null); [ -z "$PASS_COUNT" ] && PASS_COUNT=0
FAIL_COUNT=$(grep -c '|FAIL|' "$RESULTS_FILE" 2>/dev/null); [ -z "$FAIL_COUNT" ] && FAIL_COUNT=0
echo "  PASS: $PASS_COUNT" >&2
echo "  FAIL: $FAIL_COUNT" >&2
echo "  Wall: ${TOTAL_MS} ms" >&2
if [ "${CORELINK_NO_SCCACHE:-0}" != 1 ] && command -v sccache >/dev/null 2>&1; then
    echo "  --- sccache ---" >&2
    sccache --show-stats 2>/dev/null | grep -E 'Compile requests|Cache hits|Cache misses|Cache size|Max cache size' | sed 's/^/  /' >&2
fi

if [ "$FAIL_COUNT" -gt 0 ]; then
    echo "" >&2
    echo "─── Failures ───" >&2
    grep '|FAIL|' "$RESULTS_FILE" | while IFS='|' read -r group name status dur log; do
        printf '  \033[31m✘\033[0m %s/%s — see %s\n' "$group" "$name" "$log" >&2
    done
fi

exit "$FAIL_COUNT"
