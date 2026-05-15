#!/usr/bin/env bash
# scripts/fuzz-all.sh — run every cargo-fuzz target across the CoreLink
# workspace for `${FUZZ_DURATION:-300}` seconds each. Used by the
# pre-pentest hardening lane (BYOK envelope + tenant-path extended +
# audit-chain Merkle/JCS + HKDF) plus the legacy WI-S15-006 CLI
# targets.
#
# Invocation:
#
#   FUZZ_DURATION=300 scripts/fuzz-all.sh             # PR-gate (5 min each)
#   FUZZ_DURATION=1800 scripts/fuzz-all.sh            # nightly (30 min each)
#   FUZZ_DURATION=60 scripts/fuzz-all.sh --smoke      # smoke (1 min each)
#
# Requirements:
#   - nightly rust toolchain (`rustup install nightly`)
#   - cargo-fuzz 0.13.1 pinned (`cargo install cargo-fuzz --locked --version =0.13.1`)
#
# Fail-closed: any non-zero exit from any target aborts the whole run.

set -euo pipefail

FUZZ_DURATION="${FUZZ_DURATION:-300}"

# (crate_path, target_name) pairs. Update this list when adding a new
# fuzz target to the workspace — there is no other registry.
TARGETS=(
  # WI-S15-006 CLI ship-gate targets
  "corelink-cli:cli_input"
  "corelink-cli:config_toml"
  "corelink-cli:json_deserialize"
  "corelink-cli:auth_resolution"
  "corelink-cli:secret_redaction_check"
  # WI-S01-001 baseline tenant-path
  "tenant-path:derive_prefix"
  # WI-S01 corelink-hash
  "corelink-hash:digest_parse"
  "corelink-hash:verify_body"
  # Fuzz expansion lane (pre-pentest hardening) — NEW
  "corelink-byok:wrapped_dek_parse"
  "corelink-byok:envelope_roundtrip"
  "tenant-path:derive_prefix_extended"
  "corelink-audit-chain:merkle_append"
  "corelink-audit-chain:jcs_canonicalize"
  "corelink-ac:hkdf_expand"
)

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

echo "fuzz-all.sh: ${#TARGETS[@]} targets x ${FUZZ_DURATION}s each"
echo

FAILED=()
for pair in "${TARGETS[@]}"; do
  crate="${pair%%:*}"
  target="${pair##*:}"
  echo "── ${crate}::${target} (${FUZZ_DURATION}s) ─────────────────────────"
  pushd "crates/${crate}" > /dev/null

  if cargo +nightly fuzz run "${target}" -- -max_total_time="${FUZZ_DURATION}"; then
    echo "  PASS ${crate}::${target}"
  else
    echo "  FAIL ${crate}::${target}"
    FAILED+=("${crate}::${target}")
  fi

  popd > /dev/null
  echo
done

if [ "${#FAILED[@]}" -gt 0 ]; then
  echo "FAILED (${#FAILED[@]}):"
  for f in "${FAILED[@]}"; do
    echo "  - ${f}"
  done
  exit 1
fi

echo "fuzz-all.sh: all ${#TARGETS[@]} targets PASSED at ${FUZZ_DURATION}s each"
