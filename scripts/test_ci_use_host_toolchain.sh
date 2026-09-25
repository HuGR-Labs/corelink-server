#!/usr/bin/env bash
# Contract test for the base-owned host-toolchain helper.
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

TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT
MOCK_HOME="$TMP_ROOT/home"
MOCK_BIN="$TMP_ROOT/bin"
MOCK_TRIPLE="test-host"
MOCK_TOOLCHAIN="$MOCK_HOME/.rustup/toolchains/1.91.1-$MOCK_TRIPLE"
GITHUB_PATH_FILE="$TMP_ROOT/github-path"
RUSTUP_LOG="$TMP_ROOT/rustup.log"
RUSTUP_COMPONENTS="$TMP_ROOT/components"
mkdir -p "$MOCK_TOOLCHAIN/bin" "$MOCK_TOOLCHAIN/lib/rustlib/wasm32-unknown-unknown" "$MOCK_BIN"
: > "$GITHUB_PATH_FILE"

cat > "$MOCK_TOOLCHAIN/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
exit 0
CARGO
cat > "$MOCK_TOOLCHAIN/bin/rustc" <<'RUSTC'
#!/usr/bin/env bash
echo 'rustc 1.91.1 (fixture)'
RUSTC
chmod +x "$MOCK_TOOLCHAIN/bin/cargo" "$MOCK_TOOLCHAIN/bin/rustc"

cat > "$MOCK_BIN/rustup" <<'RUSTUP'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$RUSTUP_LOG"
if [[ "$*" != "component list --toolchain 1.91.1 --installed" ]]; then
  echo "unexpected rustup command: $*" >&2
  exit 9
fi
cat "$RUSTUP_COMPONENTS"
RUSTUP
chmod +x "$MOCK_BIN/rustup"

run_helper() {
  HOME="$MOCK_HOME" \
    HOST_TRIPLE="$MOCK_TRIPLE" \
    GITHUB_PATH="$GITHUB_PATH_FILE" \
    RUSTUP_LOG="$RUSTUP_LOG" \
    RUSTUP_COMPONENTS="$RUSTUP_COMPONENTS" \
    PATH="$MOCK_BIN:$PATH" \
    bash "$SCRIPT" "$@"
}

cat > "$RUSTUP_COMPONENTS" <<EOF
clippy-$MOCK_TRIPLE
rustfmt-$MOCK_TRIPLE
llvm-tools-$MOCK_TRIPLE
EOF
if ! output="$(run_helper --component clippy --component rustfmt --component llvm-tools-preview wasm32-unknown-unknown 2>&1)"; then
  echo "B133: installed target/components should pass: $output" >&2
  exit 1
fi
grep -Fqx "$MOCK_TOOLCHAIN/bin" "$GITHUB_PATH_FILE"
grep -Fq 'verified target(s): wasm32-unknown-unknown' <<<"$output"
grep -Fq 'verified component: llvm-tools-preview' <<<"$output"

printf 'clippy-%s\nrustfmt-%s\n' "$MOCK_TRIPLE" "$MOCK_TRIPLE" > "$RUSTUP_COMPONENTS"
if output="$(run_helper --component llvm-tools-preview 2>&1)"; then
  echo "B133: missing llvm-tools-preview was accepted" >&2
  exit 1
fi
grep -Fq 'llvm-tools-preview is not installed' <<<"$output"

if output="$(run_helper missing-target 2>&1)"; then
  echo "B133: missing target was accepted" >&2
  exit 1
fi
grep -Fq 'missing target(s): missing-target' <<<"$output"
if grep -Eq 'toolchain install|component add|rustup default|target add' "$RUSTUP_LOG"; then
  echo "B133: helper attempted a rustup mutation" >&2
  exit 1
fi
echo "B133: host toolchain, required components, and targets are verified without provisioning"
