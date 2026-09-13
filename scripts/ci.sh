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
# Cargo target: each invocation uses a fresh private directory (under
# `$RUNNER_TEMP` when usable, otherwise the system temp parent) and removes it
# after all selected pipelines have been awaited normally.  Cancellation keeps
# it for runner-temp garbage collection because descendants may still be
# unwinding.  An ambient caller-provided `CARGO_TARGET_DIR` is intentionally
# shadowed and never removed; this is the safest behavior for shared runners.
#
# Charter alignment: per `docs/internal/TECHLEAD-CHECKLIST.md` §L1 + §L6.
# Memory feedback: parallel-by-default per user mandate 2026-05-26.

set -uo pipefail

# Parse arguments before changing directory, creating logs, starting sccache,
# or allocating a Cargo target.  `--help` is intentionally side-effect-free.
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

# The target directory is deliberately private to this invocation.  In
# particular, do not reuse a caller-provided CARGO_TARGET_DIR: doing so would
# re-introduce cross-run races, and cleaning it on exit could destroy data that
# the caller owns.  The caller's value is left untouched on disk and is only
# shadowed in this process for the duration of the CI run.
CI_TARGET_DIR=""
CI_TARGET_DIR_MARKER=""
CI_TARGET_DIR_CREATED=0
CI_CALLER_TARGET_SET=0
CI_CALLER_TARGET_DIR=""
CI_CLEANUP_RUNNING=0
CI_PRESERVE_TARGET=0
CI_PIPELINES_AWAITED=0
RUST_PID=""
VAL_PID=""

if [ "${CARGO_TARGET_DIR+x}" = x ]; then
    CI_CALLER_TARGET_SET=1
    CI_CALLER_TARGET_DIR="$CARGO_TARGET_DIR"
fi

# shellcheck disable=SC2329  # invoked indirectly by the EXIT trap below.
cleanup_ci() {
    local status=$?

    # A trap can be entered more than once while a child is being reaped.  Do
    # not run any destructive cleanup twice.
    if [ "$CI_CLEANUP_RUNNING" -eq 1 ]; then
        exit "$status"
    fi
    CI_CLEANUP_RUNNING=1
    trap - EXIT HUP INT TERM

    # Remove only after every selected pipeline was awaited normally and both
    # direct-PID slots were cleared.  On cancellation or an incomplete/error
    # path, leave the target for RUNNER_TEMP/TMPDIR garbage collection: a
    # descendant may still be unwinding and could otherwise race this rm.
    if [ "$CI_TARGET_DIR_CREATED" -eq 1 ] \
        && [ -n "$CI_TARGET_DIR" ] \
        && [ -n "$CI_TARGET_DIR_MARKER" ] \
        && [ "$CI_TARGET_DIR_MARKER" = "$CI_TARGET_DIR/.corelink-ci-owned" ] \
        && [[ "$CI_TARGET_DIR" == */corelink-ci-target.* ]]; then
        if [ "$CI_PRESERVE_TARGET" -eq 0 ] \
            && [ "$CI_PIPELINES_AWAITED" -eq 1 ] \
            && [ -z "${RUST_PID:-}" ] \
            && [ -z "${VAL_PID:-}" ]; then
            rm -rf -- "$CI_TARGET_DIR"
        else
            printf '[ci] preserving Cargo target after interrupted/incomplete run: %s\n' "$CI_TARGET_DIR" >&2
        fi
    fi

    if [ -n "${RESULTS_FILE:-}" ]; then
        rm -f -- "$RESULTS_FILE"
    fi
    if [ -n "${INFRA_FILE:-}" ]; then
        rm -f -- "$INFRA_FILE"
    fi

    # This script is normally a child process, but restore the environment if
    # it is ever embedded by a caller that uses `source`.
    if [ "$CI_CALLER_TARGET_SET" -eq 1 ]; then
        export CARGO_TARGET_DIR="$CI_CALLER_TARGET_DIR"
    else
        unset CARGO_TARGET_DIR
    fi
    exit "$status"
}

# shellcheck disable=SC2329  # invoked indirectly by the signal trap below.
terminate_ci_children() {
    local pid
    for pid in "${RUST_PID:-}" "${VAL_PID:-}"; do
        # The PID belongs to a direct child while its slot is populated.  Do
        # not wait/reap here: target preservation is the safe cancellation
        # behavior when descendants may still be running.
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            kill -TERM "$pid" 2>/dev/null || true
        fi
    done
}

# shellcheck disable=SC2329  # invoked indirectly by the HUP/INT/TERM traps.
on_ci_signal() {
    local signal="$1"
    local status=143
    case "$signal" in
        HUP) status=129 ;;
        INT) status=130 ;;
        TERM) status=143 ;;
    esac
    trap - HUP INT TERM
    CI_PRESERVE_TARGET=1
    printf '[ci] received %s; stopping child gates\n' "$signal" >&2
    terminate_ci_children
    exit "$status"
}

trap cleanup_ci EXIT
trap 'on_ci_signal HUP' HUP
trap 'on_ci_signal INT' INT
trap 'on_ci_signal TERM' TERM

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT" || exit 2

LOG_DIR="target/ci-logs"
mkdir -p "$LOG_DIR" || {
    echo "ci.sh: unable to create $LOG_DIR" >&2
    exit 2
}

create_ci_target_dir() {
    local runner_temp="${RUNNER_TEMP:-}"
    local target_parent=""

    # RUNNER_TEMP is already isolated on GitHub-hosted runners.  Require it
    # to be an existing, writable, non-root directory so a malformed caller
    # value cannot turn cleanup into a broad-directory operation.
    if [ -n "$runner_temp" ]; then
        target_parent="${runner_temp%/}"
        if [ -n "$target_parent" ] \
            && [ "$target_parent" != "/" ] \
            && [ -d "$target_parent" ] \
            && [ -w "$target_parent" ]; then
            CI_TARGET_DIR="$(mktemp -d "$target_parent/corelink-ci-target.XXXXXX" 2>/dev/null)" || CI_TARGET_DIR=""
        fi
    fi

    # Local runs (and runners with an unusable RUNNER_TEMP) get a fresh
    # directory from mktemp's private template.  It is never shared with a
    # previous invocation, even when an old run left evidence behind.
    if [ -z "$CI_TARGET_DIR" ]; then
        CI_TARGET_DIR="$(mktemp -d "${TMPDIR:-/tmp}/corelink-ci-target.XXXXXX")" || {
            echo "ci.sh: unable to create a private Cargo target directory" >&2
            return 1
        }
    fi

    CI_TARGET_DIR_MARKER="$CI_TARGET_DIR/.corelink-ci-owned"
    # Mark ownership before any operation that can fail, so EXIT cleanup also
    # removes a freshly-created directory if marker creation itself fails.
    CI_TARGET_DIR_CREATED=1
    if ! : >"$CI_TARGET_DIR_MARKER"; then
        echo "ci.sh: unable to mark private Cargo target directory" >&2
        return 1
    fi
    export CARGO_TARGET_DIR="$CI_TARGET_DIR"

    if [ "$CI_CALLER_TARGET_SET" -eq 1 ]; then
        echo "[ci] ignoring caller-provided CARGO_TARGET_DIR; using a fresh private target" >&2
    fi
    echo "[ci] Cargo target: $CARGO_TARGET_DIR (removed on exit)" >&2
}

create_ci_target_dir || exit 2

# Keep the CI target bounded even when the optional sccache binary is absent or
# cannot start.  Incremental state is useful for a developer's edit loop, but
# this invocation already gets a fresh target and runs the complete gate set;
# retaining it only duplicates multi-gigabyte compiler state.  CI also does not
# need debugger symbols to compile or execute tests.  These Cargo profile
# overrides change artifact size, not test selection, assertions, or runtime
# behavior, so the full workspace suite remains covered.
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0

# Compile cache. sccache only helps when incremental is OFF (it can't cache
# `-C incremental`), so we enable it HERE — in the CI regime — by exporting
# RUSTC_WRAPPER alongside the bounded profile above. This dedupes the redundant
# recompiles
# across the build/clippy/test gates and caches across CI runs. The dev loop
# (plain `cargo build`/`nextest`) stays incremental and does NOT use sccache.
# Hard-capped at 4G (this box is disk-tight) so the cache can never overflow.
# Bypass with CORELINK_NO_SCCACHE=1 (e.g. when chasing a cache-masked miscompile).
if [ "${CORELINK_NO_SCCACHE:-0}" != 1 ] && command -v sccache >/dev/null 2>&1; then
    export RUSTC_WRAPPER=sccache
    export SCCACHE_CACHE_SIZE="${SCCACHE_CACHE_SIZE:-4G}"
    sccache --start-server >/dev/null 2>&1 || true
    sccache --zero-stats >/dev/null 2>&1 || true
    echo "[sccache active — cap ${SCCACHE_CACHE_SIZE}, incremental off]" >&2
fi

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
        # Drain every test binary in one bundle run so independent failures are
        # reported together instead of forcing serial full-workspace reruns.
        RUST_GATES+=("cargo-test|cargo test --workspace --no-fail-fast")
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
RESULTS_FILE="$(mktemp)" || exit 2
INFRA_FILE="$(mktemp)" || exit 2

record_result() {
    # A missing result is an infrastructure failure, never an absent gate.
    local record="$1"
    if ! printf '%s\n' "$record" >>"$RESULTS_FILE"; then
        printf '%s\n' "${record%%|*}/result-record" >>"$INFRA_FILE" || true
        return 42
    fi
}

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
        record_result "$group|$name|PASS|$dur_ms|$log" || return 42
        printf '  \033[32m✔\033[0m %-32s  %5d ms\n' "$name" "$dur_ms" >&2
    else
        end_ns=$(python3 -c 'import time; print(int(time.time_ns()))')
        dur_ms=$(( (end_ns - start_ns) / 1000000 ))
        if classification=$(python3 scripts/classify-runner-failure.py <"$log"); then
            record_result "$group|$name|FAIL|$dur_ms|$log" || return 42
            printf '  \033[31m✘\033[0m %-32s  %5d ms  (see %s)\n' "$name" "$dur_ms" "$log" >&2
        else
            classify_rc=$?
            if [ "$classify_rc" -eq 42 ]; then
                record_result "$group|$name|INFRA|$dur_ms|$log" || return 42
                if ! printf '%s/%s\n' "$group" "$name" >>"$INFRA_FILE"; then
                    return 42
                fi
                printf '  ⚠ %s\n' "$classification" >&2
            else
                record_result "$group|$name|FAIL|$dur_ms|$log" || return 42
                printf '  \033[31m✘\033[0m %-32s  %5d ms  (see %s)\n' "$name" "$dur_ms" "$log" >&2
            fi
        fi
    fi
}

run_rust_pipeline() {
    # Rust gates run serially within this group (they share cargo target dir;
    # cargo itself parallelises internally per build).
    local pipeline_rc=0
    for entry in "${RUST_GATES[@]}"; do
        local name="${entry%%|*}"
        local cmd="${entry#*|}"
        run_gate "$name" "$cmd" rust || pipeline_rc=42
    done
    return "$pipeline_rc"
}

run_validator_pool() {
    # Validators run in parallel: max 8 concurrent (12-core box; leave room
    # for the rust group if running concurrently). xargs -P-style via &/wait.
    local max_concurrent=8
    local running=0
    local pipeline_rc=0
    for entry in "${VALIDATOR_GATES[@]}"; do
        local name="${entry%%|*}"
        local cmd="${entry#*|}"
        run_gate "$name" "$cmd" validator &
        running=$(( running + 1 ))
        if [ "$running" -ge "$max_concurrent" ]; then
            if ! wait -n; then
                pipeline_rc=42
            fi
            running=$(( running - 1 ))
        fi
    done
    if ! wait; then
        pipeline_rc=42
    fi
    return "$pipeline_rc"
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

if [ "$RUN_RUST" = 1 ]; then
    if ! wait "$RUST_PID"; then
        printf 'rust/pipeline\n' >>"$INFRA_FILE" || true
        PIPELINE_INFRA=1
    fi
    RUST_PID=""
fi
if [ "$RUN_VALIDATORS" = 1 ]; then
    if ! wait "$VAL_PID"; then
        printf 'validator/pipeline\n' >>"$INFRA_FILE" || true
        PIPELINE_INFRA=1
    fi
    VAL_PID=""
fi
CI_PIPELINES_AWAITED=1

END_NS=$(python3 -c 'import time; print(int(time.time_ns()))')
TOTAL_MS=$(( (END_NS - START_NS) / 1000000 ))

# --- summary ------------------------------------------------------------------
echo "" >&2
echo "─── Summary ───" >&2
# grep -c exits 1 when match count is 0; capture exit and force a clean integer.
PASS_COUNT=$(grep -c '|PASS|' "$RESULTS_FILE" 2>/dev/null); [ -z "$PASS_COUNT" ] && PASS_COUNT=0
FAIL_COUNT=$(grep -c '|FAIL|' "$RESULTS_FILE" 2>/dev/null); [ -z "$FAIL_COUNT" ] && FAIL_COUNT=0
INFRA_COUNT=$(grep -c . "$INFRA_FILE" 2>/dev/null); [ -z "$INFRA_COUNT" ] && INFRA_COUNT=0
if [ "${PIPELINE_INFRA:-0}" -eq 1 ]; then
    INFRA_COUNT=$((INFRA_COUNT + 1))
fi
echo "  PASS: $PASS_COUNT" >&2
echo "  FAIL: $FAIL_COUNT" >&2
echo "  INFRA: $INFRA_COUNT" >&2
echo "  Wall: ${TOTAL_MS} ms" >&2
if [ "${CORELINK_NO_SCCACHE:-0}" != 1 ] && command -v sccache >/dev/null 2>&1; then
    echo "  --- sccache ---" >&2
    sccache --show-stats 2>/dev/null | grep -E 'Compile requests|Cache hits|Cache misses|Cache size|Max cache size' | sed 's/^/  /' >&2
fi

if [ "$FAIL_COUNT" -gt 0 ]; then
    echo "" >&2
    echo "─── Failures ───" >&2
    grep '|FAIL|' "$RESULTS_FILE" | while IFS='|' read -r group name _status _dur log; do
        printf '  \033[31m✘\033[0m %s/%s — see %s\n' "$group" "$name" "$log" >&2
    done
fi

if [ "$INFRA_COUNT" -gt 0 ]; then exit 42; fi
exit "$FAIL_COUNT"
