#!/usr/bin/env bash
# Static contract test for the base-owned host-toolchain helper.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
SCRIPT="$HERE/ci-use-host-toolchain.sh"
[ -f "$SCRIPT" ] || { echo "B133: host-toolchain helper missing" >&2; exit 1; }
[ ! -L "$SCRIPT" ] || { echo "B133: host-toolchain helper must not be a symlink" >&2; exit 1; }
grep -q 'set -euo pipefail' "$SCRIPT"
grep -q 'rust-toolchain.toml' "$SCRIPT"
grep -q 'HOST_TRIPLE' "$SCRIPT"
grep -q 'MISSING' "$SCRIPT"
if grep -Ev '^[[:space:]]*#' "$SCRIPT" | grep -Eq '^[[:space:]]*rustup (toolchain install|default|target add)'; then
  echo "B133: host helper must never provision or mutate rustup" >&2
  exit 1
fi
echo "B133: host-toolchain helper is fail-closed and non-provisioning"
