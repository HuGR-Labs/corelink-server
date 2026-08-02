#!/usr/bin/env bash
# CoreLink TLC runner — reusable wrapper para CI gate per spec.
#
# Usage:
#   ./scripts/run_tlc_corelink.sh <spec_basename>
#   ./scripts/run_tlc_corelink.sh <spec_basename> <cfg_basename>
# Examples:
#   ./scripts/run_tlc_corelink.sh dsr_erasure_atomicity
#   ./scripts/run_tlc_corelink.sh tenant_isolation tenant_isolation_nightly
#
# When <cfg_basename> is omitted, defaults to <spec_basename> (i.e. the PR
# lane: tenant_isolation.tla + tenant_isolation.cfg). Pass an explicit cfg
# basename to run the nightly extended bounds (per WI-S01-007 §6.1.7).
#
# Reuses ADR-0042 §A1 TLC v1.8.0 SHA-256 pinned bootstrap ceremony.
# Expected pinned SHA-256: e22f8ffb4bacdea0a871f444dd94fe5fb0d8013b3388ae39e82e26f852c735d5

set -euo pipefail

SPEC_NAME="${1:?Usage: $0 <spec_basename> [cfg_basename]}"
CFG_NAME="${2:-${SPEC_NAME}}"
SPEC_DIR="$(git rev-parse --show-toplevel)/specs/tla"
TLC_VERSION_REQUIRED="1.8.0"
TLC_SHA256_PINNED="e22f8ffb4bacdea0a871f444dd94fe5fb0d8013b3388ae39e82e26f852c735d5"

if [[ ! -f "${SPEC_DIR}/${SPEC_NAME}.tla" ]]; then
  echo "ERROR: spec ${SPEC_DIR}/${SPEC_NAME}.tla not found" >&2
  exit 2
fi

if [[ ! -f "${SPEC_DIR}/${CFG_NAME}.cfg" ]]; then
  echo "ERROR: cfg ${SPEC_DIR}/${CFG_NAME}.cfg not found" >&2
  exit 2
fi

# Verify TLC SHA-256 pin (per ADR-0042 §A1) — MANDATORY supply-chain gate.
# Lote 10.11.0-bis-prime cycle 5 fix: SHA verification targets the actual TLC artifact
# (tla2tools.jar OR a single-file binary), NOT a wrapper script. Workflow installer
# may set TLC_JAR=/path/to/tla2tools.jar to direct the hash check at the artifact.
# Opt-OUT only via TLC_SHA256_SKIP=1 (for local dev with newer TLC; CI MUST verify).
TLC_BIN="${TLC_BIN:-tlc}"
if ! command -v "${TLC_BIN}" >/dev/null 2>&1; then
  echo "ERROR: TLC binary not found in PATH (expected at \`${TLC_BIN}\`)" >&2
  exit 4
fi

# Determine artifact to hash:
#   1. Explicit TLC_JAR env var (workflow-friendly; canonical for CI).
#   2. Fall back to TLC_BIN (covers single-file native binary distributions).
ACTUAL_TLC_PATH="$(command -v "${TLC_BIN}")"
ARTIFACT_PATH="${TLC_JAR:-${ACTUAL_TLC_PATH}}"
if [[ ! -f "${ARTIFACT_PATH}" ]]; then
  echo "ERROR: TLC artifact not found at \`${ARTIFACT_PATH}\`" >&2
  echo "       Set TLC_JAR=/path/to/tla2tools.jar OR ensure ${TLC_BIN} is the artifact itself." >&2
  exit 4
fi

if [[ "${TLC_SHA256_SKIP:-0}" == "1" ]]; then
  echo "WARNING: TLC SHA-256 verification SKIPPED via TLC_SHA256_SKIP=1 (local dev only; CI MUST not skip)." >&2
else
  ACTUAL_SHA="$(shasum -a 256 "${ARTIFACT_PATH}" | awk '{print $1}')"
  if [[ "${ACTUAL_SHA}" != "${TLC_SHA256_PINNED}" ]]; then
    echo "ERROR: TLC SHA-256 supply-chain pin mismatch (ADR-0042 §A1 violation)" >&2
    echo "       expected: ${TLC_SHA256_PINNED}" >&2
    echo "       actual:   ${ACTUAL_SHA}" >&2
    echo "       artifact: ${ARTIFACT_PATH}" >&2
    echo "       Either install pinned TLC v${TLC_VERSION_REQUIRED} (set TLC_JAR=jar path) or set TLC_SHA256_SKIP=1 (NOT for CI)." >&2
    exit 3
  fi
fi

cd "${SPEC_DIR}"
echo "Running TLC: ${SPEC_NAME}.tla with config ${CFG_NAME}.cfg (artifact verified: ${ARTIFACT_PATH})"

# Canonical TLC execution flags per WI-S01-007 §6.1.7:
#   -workers 2     parallel BFS (GitHub `ubuntu-latest` has 4 vCPU; 2 leaves
#                  headroom for other PR jobs in same runner pool).
#   -coverage 60   prints coverage statistics every 60s (catches "gate is
#                  theatre" — zero-coverage states surface explicitly).
#   -fp 32         32-bit fingerprints for compactness on these small bounds
#                  (default 64-bit; 32-bit ~1 in 4B collision risk acceptable
#                  given <10^7 states).
#   -checkpoint 0  disable checkpointing (CI runners are ephemeral; no
#                  resumption use-case; checkpoint I/O is pure overhead).
#
# NOTE on deadlock detection: each cfg explicitly sets `CHECK_DEADLOCK FALSE`
# because the canonical S-01 specs are bounded by `MaxOps`/`MaxEvents` —
# reaching the bound is a terminal state, NOT a real deadlock. Disabling the
# detector avoids spurious false-positives at termination. The original
# WI §6.1.7 text mentioned `-deadlock` mandatory, but that conflicts with
# bounded-state-space semantics; v1.2.0 of the WI clarifies this.
exec "${TLC_BIN}" \
  -config "${CFG_NAME}.cfg" \
  -workers 2 \
  -coverage 60 \
  -fp 32 \
  -checkpoint 0 \
  "${SPEC_NAME}.tla"
