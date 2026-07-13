#!/bin/bash
# audit_proptest_density.sh — proptest density audit across all crates.
#
# Per-crate ratio = (count of `#[test]` declarations inside `proptest!{}`
# blocks) / (count of distinct `INV-*` references in src/ + tests/).
#
# Outputs a pipe-delimited table; sortable with `sort -t'|' -k5 -n`.
#
# Usage:
#   bash scripts/audit_proptest_density.sh > /tmp/proptest_audit.csv
#   awk -F'|' 'NR>1 && $4>0 && $5!="N/A" && $5+0<1.0' /tmp/proptest_audit.csv
#
# Source audit: specs/_audits/sealed/2026-05-15-proptest-density.md
# Charter ref: CoreLink autonomous-execution charter / R-PREP lane.

set -u
LC_NUMERIC=C
export LC_NUMERIC

# Resolve repo root assuming this script lives in scripts/.
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT" || exit 1

echo "crate|prop_macros|prop_tests|inv_refs|ratio"
for c in crates/*/; do
  name=$(basename "$c")
  # 1. Count top-level proptest! macros (header signal).
  pm=$(grep -rE "^[[:space:]]*proptest!" "$c"tests "$c"src 2>/dev/null | wc -l | tr -d ' ')

  # 2. Count #[test] functions inside any proptest! { ... } block.
  pt=0
  for f in $(find "$c"tests "$c"src -name "*.rs" 2>/dev/null); do
    if grep -qE "^[[:space:]]*proptest!" "$f" 2>/dev/null; then
      # Count #[test] fns ONLY while inside a balanced `proptest! { ... }` span.
      # (Previously `in_block` was set on the first `proptest!` and NEVER reset,
      #  so every plain #[test] later in the file counted as a proptest → ~4x
      #  over-count. Track brace depth and clear the flag when the block closes.)
      n=$(awk '
        { line=$0
          if (!inb && line ~ /proptest!/) { inb=1; depth=0 }
          if (inb) {
            if (line ~ /#\[test\]/) c++
            ob=gsub(/{/,"{",line); cb=gsub(/}/,"}",line)
            depth += ob - cb
            if (depth <= 0 && ob+cb > 0) inb=0
          }
        }
        END { print c+0 }' "$f")
      pt=$((pt + n))
    fi
  done

  # 3. Attribute-style proptests (proptest-attr / proptest-derive).
  pa=$(grep -rE "#\[test_proptest|#\[proptest\(|#\[proptest\]" "$c" 2>/dev/null | wc -l | tr -d ' ')
  pt=$((pt + pa))

  # 4. Count distinct INV-* references.
  inv=$(grep -rho "INV-[A-Z][A-Z0-9_-]*" "$c" 2>/dev/null | sort -u | wc -l | tr -d ' ')

  if [ "$inv" -eq 0 ]; then
    ratio="N/A"
  else
    ratio=$(awk -v p="$pt" -v i="$inv" 'BEGIN { printf "%.2f", p / i }')
  fi
  echo "$name|$pm|$pt|$inv|$ratio"
done
