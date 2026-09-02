#!/usr/bin/env bash
# ci-use-host-toolchain.sh — put the WORKSPACE-PINNED host toolchain on PATH.
#
# WHY THIS EXISTS (do not replace with `dtolnay/rust-toolchain` on self-hosted):
#
# All 5 self-hosted Mac runners share ONE `$HOME`, so one `~/.rustup`. The
# toolchain action runs, per its own action.yml:
#
#     rustup toolchain install <tc> --profile minimal --no-self-update
#     rustup default <tc>
#
# Both write to that SHARED tree while other jobs are executing binaries out of
# it. Two distinct failure modes, and the second is worse than the first:
#
#   1. CRASH — `~/.cargo/bin/*` are symlinks to the single `rustup` shim, so a
#      concurrent install can make `rustc` unexecutable mid-build. Signature:
#      `could not execute process rustc … No such file or directory (os error 2)`.
#      This took the whole host down for a day on 2026-06-15 (see the ROOT FIX
#      note in fuzz-nightly.yml).
#
#   2. SILENT REPRODUCIBILITY DRIFT — `rustup default` rewrites the MACHINE-GLOBAL
#      default. The workspace pins 1.91.1 in rust-toolchain.toml precisely because
#      "rustc minor upgrades can introduce LLVM non-determinism that breaks
#      reproducibility" (ADR-0015). A job that defaults the host to `stable`
#      therefore moves every OTHER concurrent job off the reproducibility pin,
#      with no error anywhere. The host default was measured at
#      `stable-x86_64-apple-darwin` on 2026-08-03 — already drifted.
#
# This script provisions NOTHING. It reads the channel from rust-toolchain.toml
# (so the CI step and ADR-0015 can never disagree), points PATH at the
# pre-installed host toolchain, and FAILS LOUDLY if it or a requested target is
# absent — a missing toolchain must stop one job, never silently reshape the fleet.
#
# Usage:  bash scripts/ci-use-host-toolchain.sh [target-triple …]
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# Do not parse TOML with a line regex.  `channel` is meaningful only under
# `[toolchain]`; a homonymous key in another table must neither select a
# toolchain nor make a safe pin fail.  Python's stdlib TOML parser also handles
# the legal whitespace and trailing-comment forms that a regex routinely gets
# wrong.  It prints only the typed `[toolchain].channel` string.
if ! CHANNEL="$(python3 - rust-toolchain.toml <<'PY'
import sys
try:
    import tomllib
    with open(sys.argv[1], "rb") as handle:
        document = tomllib.load(handle)
    toolchain = document.get("toolchain")
    channel = toolchain.get("channel") if isinstance(toolchain, dict) else None
    if not isinstance(channel, str) or not channel:
        raise ValueError("[toolchain].channel must be a non-empty string")
except (OSError, ValueError, tomllib.TOMLDecodeError) as exc:
    print(f"ci-use-host-toolchain: invalid rust-toolchain.toml: {exc}", file=sys.stderr)
    raise SystemExit(2)
print(channel)
PY
)"; then
    echo "::error::ci-use-host-toolchain: could not parse [toolchain] channel from rust-toolchain.toml." >&2
    exit 2
fi

# CHANNEL is interpolated into a PATH that is then appended to $GITHUB_PATH:
#
#     TC="$HOME/.rustup/toolchains/${CHANNEL}-${HOST_TRIPLE}"
#     echo "$TC/bin" >> "$GITHUB_PATH"
#
# so whoever controls `rust-toolchain.toml` controls which directory becomes the
# front of PATH for every later step in the job. Unvalidated, a channel of
# `../../../<somewhere>` escapes ~/.rustup entirely; point it at a directory that
# already holds executable `bin/cargo` and `bin/rustc` and every subsequent
# `cargo` invocation is the attacker's. Found while fixing B-133, where
# `dependabot-policy.yml` ran this script against a `pull_request_target`
# checkout of PR content — the traversal target being the PR's own tree, whose
# executable bits git preserves.
#
# A rustup channel is a closed, documented form: a version (`1.91.1`), a named
# channel (`stable`, `beta`, `nightly`), optionally dated (`nightly-2026-01-01`)
# and optionally host-suffixed. Every legal spelling is alphanumerics, dots,
# underscores and hyphens — so this is a whitelist, not a blacklist of the
# traversals someone thought of. `/` and `..` cannot survive it.
case "$CHANNEL" in
    *[!A-Za-z0-9._-]*)
        echo "::error::ci-use-host-toolchain: refusing toolchain channel '${CHANNEL}' — a channel may contain only letters, digits, dot, underscore and hyphen." >&2
        echo "::error::This string is interpolated into a filesystem path that is prepended to \$GITHUB_PATH; anything else is a path-traversal primitive, not a channel." >&2
        exit 2
        ;;
esac
# `..` is alphanumeric-free but passes the class above (dots are legal in
# `1.91.1`), so the traversal spelling is refused on its own.
case "$CHANNEL" in
    .. | ..* | *..*)
        echo "::error::ci-use-host-toolchain: refusing toolchain channel '${CHANNEL}' — it contains '..', which walks out of ~/.rustup/toolchains." >&2
        exit 2
        ;;
esac

# Host triple: DETECTED, with an explicit override still honoured.
#
# This used to default to `x86_64-apple-darwin` unconditionally, from when the
# only self-hosted fleet was the owner's Macs. The moment a caller runs on the
# Linux fabric (`runs-on: corelink`) that default sends it looking for
# `…/toolchains/1.91.1-x86_64-apple-darwin` on a Linux box, which cannot exist —
# and the script then reports "not installed on this runner host", i.e. it blames
# the host for a triple the script itself invented. Detecting is the fix; a
# second hardcoded triple would just move the assumption.
#
# `uname -m` reports `arm64` on Apple Silicon and `aarch64` on Linux for the same
# architecture, so the mapping is per-OS rather than a single table.
if [ -z "${HOST_TRIPLE:-}" ]; then
    _os="$(uname -s)"
    _arch="$(uname -m)"
    case "${_os}" in
        Darwin)
            case "${_arch}" in
                arm64|aarch64) HOST_TRIPLE="aarch64-apple-darwin" ;;
                x86_64)        HOST_TRIPLE="x86_64-apple-darwin" ;;
                *)             HOST_TRIPLE="${_arch}-apple-darwin" ;;
            esac
            ;;
        Linux)
            case "${_arch}" in
                aarch64|arm64) HOST_TRIPLE="aarch64-unknown-linux-gnu" ;;
                x86_64)        HOST_TRIPLE="x86_64-unknown-linux-gnu" ;;
                *)             HOST_TRIPLE="${_arch}-unknown-linux-gnu" ;;
            esac
            ;;
        *)
            echo "::error::ci-use-host-toolchain: unsupported host OS '${_os}'. Set HOST_TRIPLE explicitly." >&2
            exit 2
            ;;
    esac
fi
TC="$HOME/.rustup/toolchains/${CHANNEL}-${HOST_TRIPLE}"

if [ ! -x "$TC/bin/cargo" ] || [ ! -x "$TC/bin/rustc" ]; then
    echo "::error::ci-use-host-toolchain: the workspace-pinned toolchain ${CHANNEL}-${HOST_TRIPLE} is not installed on this runner host (looked in $TC)." >&2
    echo "::error::Install it ONCE on the host, while no CI is running: rustup toolchain install ${CHANNEL}" >&2
    echo "::error::Do NOT add a toolchain-provisioning action to a self-hosted job — that is the shared-\$HOME hazard this script exists to prevent." >&2
    exit 1
fi

MISSING=""
for t in "$@"; do
    [ -d "$TC/lib/rustlib/$t" ] || MISSING="${MISSING} $t"
done
if [ -n "${MISSING# }" ]; then
    echo "::error::ci-use-host-toolchain: ${CHANNEL} is missing target(s):${MISSING}" >&2
    echo "::error::Add them ONCE on the host, while no CI is running:  rustup target add --toolchain ${CHANNEL}${MISSING}" >&2
    exit 1
fi

echo "$TC/bin" >> "$GITHUB_PATH"
echo "ci-use-host-toolchain: PATH -> $TC/bin (channel ${CHANNEL} from rust-toolchain.toml)"
[ $# -gt 0 ] && echo "ci-use-host-toolchain: verified target(s): $*"
"$TC/bin/rustc" --version
