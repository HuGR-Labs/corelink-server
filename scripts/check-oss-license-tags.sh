#!/usr/bin/env bash
# check-oss-license-tags.sh — guard against the Wave 33-36 OSS-tag regression.
#
# The workspace default is `[workspace.package] license = "UNLICENSED"`
# (correct: ~85 crates are proprietary server code). OSS-designated crates
# MUST override that with a LITERAL `license = "MIT OR Apache-2.0"` in their
# own [package] table — inheritance silently makes them UNLICENSED, which is
# how the reorg wiped the DEBT-002 tags without anyone noticing.
#
# This is the missing inverse of license-policy.yml: that gate checks our
# DEPENDENCIES' licenses; this gate checks OUR OWN crates' published license.
#
# Runs locally (`bash scripts/check-oss-license-tags.sh`) and in
# license-policy.yml. Exit 1 on any OSS crate that is not literally tagged.
#
# Source of truth for the crate list: docs/internal/OSS-VS-CLOSED-MATRIX.md.
# Add a crate here the same commit you tag it OSS.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

EXPECTED_LICENSE='license = "MIT OR Apache-2.0"'

# OSS crates that must carry the literal license tag, by Cargo.toml path.
# Scoped to the published set; extend as later-phase crates are tagged
# (corelink-openapi, corelink-py, corelink-go, corelink-wasm, corelink-cli).
OSS_CRATES=(
  "crates/corelink-hash/Cargo.toml"
  "crates/corelink-client-verify/Cargo.toml"
  "crates/tenant-path/Cargo.toml"
  "crates/corelink-rate-headers/Cargo.toml"
)

fail=0
for ct in "${OSS_CRATES[@]}"; do
  if [ ! -f "$ct" ]; then
    echo "MISSING: $ct (crate renamed/removed? reconcile the matrix)"
    fail=1
    continue
  fi
  if grep -qF "$EXPECTED_LICENSE" "$ct"; then
    echo "OK:      $ct"
  else
    actual="$(grep -E '^\s*license' "$ct" || echo '(no license field — inherits UNLICENSED)')"
    echo "FAIL:    $ct — expected literal '$EXPECTED_LICENSE', found: ${actual//[$'\n']/ }"
    fail=1
  fi
done

if [ "$fail" -ne 0 ]; then
  echo ""
  echo "OSS license-tag check FAILED. An OSS crate is not literally tagged"
  echo "'MIT OR Apache-2.0' and would publish as UNLICENSED (or not publish)."
  echo "Fix: add the literal license to the crate's [package], do not inherit."
  exit 1
fi

echo ""
echo "OSS license-tag check PASSED (${#OSS_CRATES[@]} crates literally tagged)."
