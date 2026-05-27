#!/usr/bin/env bash
# byok-feature-validate.sh
#
# Regression guard for the BYOK orchestrator's mutually-exclusive
# real-provider cargo feature constraint.
#
# Authoritative reference:
#   specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md
#   specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md §7
#
# Verifies, in order:
#
#   V1. apps/server/src/byok_orchestrator.rs exists.
#   V2. It declares ALL 6 pairwise `compile_error!` macros for the
#       4-choose-2 BYOK real-provider feature pairs.
#   V3. Each of the four `byok-*-real` features is declared in
#       apps/server/Cargo.toml.
#   V4. ADR-S30-001 exists and is ACCEPTED.
#   V5. The wave-15 baseline audit references the ADR.
#
# Exit codes:
#   0  — all checks pass (mutually-exclusive constraint is in place).
#   1  — V1 failed (orchestrator file missing).
#   2  — V2 failed (missing one or more `compile_error!` macros).
#   3  — V3 failed (feature flag declaration missing).
#   4  — V4 failed (ADR missing or not ACCEPTED).
#   5  — V5 failed (baseline audit does not cite ADR).
#
# This script is invoked by:
#   - CI lane "byok-feature-matrix" (wave-30+).
#   - /techlead L0.6 + L1.3a verification for BYOK-touching branches.
#   - Manual: `bash scripts/byok-feature-validate.sh` from repo root.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

ORCH="apps/server/src/byok_orchestrator.rs"
SERVER_TOML="apps/server/Cargo.toml"
ADR="specs/03_architecture/adrs/ADR-S30-001-byok-mutually-exclusive-providers.md"
BASELINE_AUDIT="specs/_audits/sealed/2026-05-15-byok-real-provider-pattern.md"

PROVIDERS=("aws" "gcp" "azure" "vault")

fail() {
  echo "FAIL [$1]: $2" >&2
  exit "$1"
}

ok() {
  echo "PASS: $1"
}

# V1: orchestrator file exists.
if [ ! -f "$ORCH" ]; then
  fail 1 "$ORCH not found"
fi
ok "V1: $ORCH present"

# V2: all 6 pairwise compile_error! macros declared.
MISSING_PAIRS=()
for i in "${!PROVIDERS[@]}"; do
  for j in "${!PROVIDERS[@]}"; do
    if [ "$i" -ge "$j" ]; then
      continue
    fi
    a="byok-${PROVIDERS[$i]}-real"
    b="byok-${PROVIDERS[$j]}-real"
    if ! grep -qE "cfg\\(all\\(feature = \"$a\", feature = \"$b\"\\)\\)" "$ORCH"; then
      MISSING_PAIRS+=("$a + $b")
    fi
  done
done
if [ "${#MISSING_PAIRS[@]}" -gt 0 ]; then
  printf 'Missing pairwise compile_error! for pairs:\n' >&2
  printf '  - %s\n' "${MISSING_PAIRS[@]}" >&2
  fail 2 "$ORCH missing ${#MISSING_PAIRS[@]} pairwise compile_error! guard(s) (expected 6 total)"
fi
COMPILE_ERROR_COUNT=$(grep -cE '^compile_error!' "$ORCH" || true)
if [ "$COMPILE_ERROR_COUNT" -lt 6 ]; then
  fail 2 "$ORCH declares $COMPILE_ERROR_COUNT compile_error! macros; expected ≥ 6"
fi
ok "V2: all 6 pairwise compile_error! macros present ($COMPILE_ERROR_COUNT total compile_error! calls)"

# V3: each byok-*-real feature flag declared in apps/server/Cargo.toml.
if [ ! -f "$SERVER_TOML" ]; then
  fail 3 "$SERVER_TOML not found"
fi
for p in "${PROVIDERS[@]}"; do
  flag="byok-${p}-real"
  if ! grep -qE "^${flag}\\s*=" "$SERVER_TOML"; then
    fail 3 "$SERVER_TOML does not declare feature '$flag'"
  fi
done
ok "V3: all 4 byok-*-real feature flags declared in $SERVER_TOML"

# V4: ADR exists + is ACCEPTED.
if [ ! -f "$ADR" ]; then
  fail 4 "$ADR not found"
fi
if ! grep -qE '^\*\*ACCEPTED' "$ADR" && ! grep -qE '^ACCEPTED' "$ADR" && ! grep -qE 'ACCEPTED' "$ADR"; then
  fail 4 "$ADR does not contain an ACCEPTED status marker"
fi
ok "V4: $ADR present and ACCEPTED"

# V5: baseline audit cross-references the ADR.
if [ ! -f "$BASELINE_AUDIT" ]; then
  fail 5 "$BASELINE_AUDIT not found"
fi
if ! grep -qE 'ADR-S30-001' "$BASELINE_AUDIT"; then
  fail 5 "$BASELINE_AUDIT does not reference ADR-S30-001"
fi
ok "V5: $BASELINE_AUDIT cites ADR-S30-001"

echo
echo "byok-feature-validate: all checks PASS"
echo "  mutually-exclusive constraint is in place (ADR-S30-001)"
exit 0
