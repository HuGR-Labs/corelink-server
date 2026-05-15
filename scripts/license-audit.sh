#!/usr/bin/env bash
# scripts/license-audit.sh — R7-1 supply-quality rollup (b) license report.
#
# Enumerates every dependency in the CoreLink workspace via `cargo-license`
# and flags any license that is NOT on the deny.toml allowlist
# (CTRL-SUPPLY-005 + INV-SUPPLY-LICENSE-ALLOWLIST). Acts as a faster,
# human-readable companion to `cargo deny check licenses` — useful for the
# evidence pack and the GitHub PR comment surface.
#
# Allow-list canonical source: deny.toml `[licenses].allow`.
# Block-list (informational only — anything outside allow-list is blocked):
#     GPL-*, AGPL-*, SSPL-*, CDLA-Sharing-*, Commons-Clause, BUSL-*.
#
# Outputs:
#   target/license-audit/licenses.json      raw `cargo license --json`
#   target/license-audit/licenses.tsv       crate \t version \t license
#   target/license-audit/VIOLATIONS.txt     crate \t license  (empty if clean)
#
# Exit codes:
#   0  no disallowed licenses found
#   1  tooling missing
#   2  one or more disallowed licenses found (VIOLATIONS.txt non-empty)
#
# Charter: set -euo pipefail mandatory.

set -euo pipefail

usage() {
    cat <<'USAGE'
license-audit.sh — workspace dependency-license enumeration + policy gate.

Usage:
  scripts/license-audit.sh           # full audit, writes target/license-audit/
  scripts/license-audit.sh --help    # this message

Allow-list (deny.toml canonical): MIT, Apache-2.0, Apache-2.0 WITH
LLVM-exception, BSD-2-Clause, BSD-3-Clause, ISC, MPL-2.0, Unicode-DFS-2016,
Unicode-3.0, Zlib, CC0-1.0, 0BSD, CDLA-Permissive-2.0.

Block-list (everything outside allow-list — esp. GPL/AGPL/SSPL/CDLA-Sharing).

Exit 0 clean / 1 missing tool / 2 violations found.
USAGE
}

case "${1:-}" in
    --help|-h)
        usage
        exit 0
        ;;
esac

OUT_DIR="${OUT_DIR:-target/license-audit}"
mkdir -p "$OUT_DIR"

if ! command -v cargo-license >/dev/null 2>&1; then
    echo "error: cargo-license not installed." >&2
    echo "  install: cargo install cargo-license --locked" >&2
    exit 1
fi

# Allow-list — keep in lock-step with deny.toml [licenses].allow.
# SPDX expressions joined by "OR" / "AND" are flattened by cargo-license,
# but defensive matching is still applied via awk below.
ALLOWED=(
    "MIT"
    "Apache-2.0"
    "Apache-2.0 WITH LLVM-exception"
    "BSD-2-Clause"
    "BSD-3-Clause"
    "ISC"
    "MPL-2.0"
    "Unicode-DFS-2016"
    "Unicode-3.0"
    "Zlib"
    "CC0-1.0"
    "0BSD"
    "CDLA-Permissive-2.0"
)

# Build a regex of allowed tokens (escape regex metacharacters).
ALLOW_RE=$(printf '|%s' "${ALLOWED[@]}")
ALLOW_RE="${ALLOW_RE:1}"   # drop leading |
# Escape "." (none in list) and "+" (none) — only special is space, handled
# by storing the raw string; build EREGEX-safe alternation:
ALLOW_RE_ESC=$(printf '%s' "$ALLOW_RE" | sed 's/\./\\./g')

echo "==> cargo-license --json --avoid-build-deps --avoid-dev-deps"
cargo license --json --avoid-build-deps --avoid-dev-deps \
    > "$OUT_DIR/licenses.json"

# Flatten JSON to TSV: name \t version \t license
python3 - "$OUT_DIR/licenses.json" "$OUT_DIR/licenses.tsv" <<'PY'
import json, sys
src, dst = sys.argv[1], sys.argv[2]
with open(src, "r", encoding="utf-8") as fh:
    data = json.load(fh)
with open(dst, "w", encoding="utf-8") as fh:
    for entry in data:
        name = entry.get("name", "?")
        version = entry.get("version", "?")
        license_ = entry.get("license") or entry.get("license_file") or "UNLICENSED"
        fh.write(f"{name}\t{version}\t{license_}\n")
PY

# Policy check: any license token that, after splitting on "OR" / "AND" /
# slashes, is not on the allow-list, lands in VIOLATIONS.txt.
VIOLATIONS="$OUT_DIR/VIOLATIONS.txt"
: > "$VIOLATIONS"

python3 - "$OUT_DIR/licenses.tsv" "$VIOLATIONS" "${ALLOWED[@]}" <<'PY'
import sys, re
tsv, vio, *allowed = sys.argv[1], sys.argv[2], *sys.argv[3:]
allow = set(a.strip() for a in allowed)
SEP = re.compile(r"\s+OR\s+|\s+AND\s+|/", re.IGNORECASE)
violations = []
with open(tsv, "r", encoding="utf-8") as fh:
    for line in fh:
        parts = line.rstrip("\n").split("\t")
        if len(parts) != 3:
            continue
        crate, version, license_ = parts
        # Strip parens; split by OR/AND/slash. A crate passes if ANY token
        # is on the allow-list (dual-licensed crates only need ONE valid
        # license — matches cargo-deny semantics).
        cleaned = license_.replace("(", "").replace(")", "")
        tokens = [t.strip() for t in SEP.split(cleaned) if t.strip()]
        if not tokens:
            tokens = [cleaned.strip()]
        if not any(tok in allow for tok in tokens):
            violations.append(f"{crate}\t{version}\t{license_}")
with open(vio, "w", encoding="utf-8") as fh:
    for v in violations:
        fh.write(v + "\n")
print(f"violations: {len(violations)}")
PY

if [[ -s "$VIOLATIONS" ]]; then
    echo "==> license policy: FAIL"
    echo "    disallowed crates (see $VIOLATIONS):"
    sed 's/^/      /' "$VIOLATIONS"
    exit 2
fi

echo "==> license policy: PASS (all deps within allow-list)"
echo "    licenses.json : $OUT_DIR/licenses.json"
echo "    licenses.tsv  : $OUT_DIR/licenses.tsv"
