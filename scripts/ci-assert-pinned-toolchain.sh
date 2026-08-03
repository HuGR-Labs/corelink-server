#!/usr/bin/env bash
# ci-assert-pinned-toolchain.sh — prove the job compiles with the PINNED toolchain.
#
# WHY THIS EXISTS
#
# The Rust lanes that run on `runs-on: corelink` (CoreLink's own ephemeral
# runner fabric) no longer call `dtolnay/rust-toolchain`. That is safe for one
# specific reason, and this script is the machine-checked statement of it:
#
#   `rust-toolchain.toml` pins channel 1.91.1 together with its components
#   (rustc, cargo, rust-std, rust-src, rustfmt, clippy) and its targets
#   (wasm32-unknown-unknown, x86_64-unknown-linux-musl). rustup's shims honour
#   that file over ANY `rustup default`, and provision it on first use. So
#   inside this repo, cargo resolves to 1.91.1 regardless of what the image
#   happens to ship — measured on `cf-runner-cdeec405` (2026-08-03): the image
#   defaults to 1.96.0 with only x86_64-unknown-linux-gnu, and the first
#   in-repo `cargo --version` pulled 1.91.1 with rustfmt 1.8.0, clippy 0.1.91,
#   wasm32 and musl in 15 s. `rustup target add wasm32-unknown-unknown` was
#   then a 0 s no-op.
#
# The action was therefore never what decided the compiling toolchain — it asked
# for `stable` while the workspace pins 1.91.1, and the workspace pin won. On a
# GitHub-hosted runner that disagreement was merely wasteful; on an EPHEMERAL box
# it is wasteful on EVERY run, because nothing is cached between jobs.
#
# THE RISK THIS CLOSES
#
# Dropping the action means the pin is now the ONLY thing standing between a gate
# and the wrong compiler. Images drift — this one already did (the 2026-08-01
# inventory recorded rustc 1.91.1 on the box; two days later it was 1.96.0). If a
# future image ever ships a rustup that cannot reach static.rust-lang.org, or a
# rust-toolchain.toml edit lands without the matching ADR-0015 amendment, a lane
# could silently compile with a different rustc and still go green. ADR-0015
# exists precisely because "rustc minor upgrades can introduce LLVM
# non-determinism". So: assert, loudly, in-band, on every run.
#
# This is a STRENGTHENING of the gates that call it. Nothing here relaxes a
# check; it adds one that no lane had before.
#
# Usage:  bash scripts/ci-assert-pinned-toolchain.sh [target-triple …]
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

CHANNEL="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' rust-toolchain.toml | head -1)"
if [ -z "${CHANNEL:-}" ]; then
    echo "::error::ci-assert-pinned-toolchain: could not parse [toolchain] channel from rust-toolchain.toml." >&2
    exit 2
fi

# The first cargo invocation in the repo is what provisions the pinned channel
# (components + targets come from rust-toolchain.toml). Time it so the per-run
# toolchain tax is visible in the log rather than hidden inside a build step.
start=$(date +%s)
ACTUAL_RUSTC="$(rustc --version)"
ACTUAL_CARGO="$(cargo --version)"
elapsed=$(( $(date +%s) - start ))

echo "ci-assert-pinned-toolchain: rust-toolchain.toml channel = ${CHANNEL}"
echo "ci-assert-pinned-toolchain: resolved in ${elapsed}s"
echo "  rustc : ${ACTUAL_RUSTC}"
echo "  cargo : ${ACTUAL_CARGO}"

# `rustc --version` prints e.g. "rustc 1.91.1 (ed61e7d7e 2025-11-07)". Compare the
# VERSION FIELD exactly, so 1.91.10 can never satisfy a 1.91.1 pin.
ACTUAL_VER="$(printf '%s\n' "$ACTUAL_RUSTC" | awk '{print $2}')"
if [ "$ACTUAL_VER" != "$CHANNEL" ]; then
    echo "::error::ci-assert-pinned-toolchain: this job is compiling with rustc ${ACTUAL_VER}, but rust-toolchain.toml pins ${CHANNEL}." >&2
    echo "::error::ADR-0015 pins the channel because rustc minor upgrades can introduce LLVM non-determinism that breaks reproducibility." >&2
    echo "::error::Do NOT paper over this by re-adding a toolchain action — find out why rustup did not honour the pin." >&2
    exit 1
fi

# Any target the caller is about to build for must already be present. The pin
# declares wasm32-unknown-unknown and x86_64-unknown-linux-musl, so on a healthy
# box this is a no-op; if it is not, fail here with a clear message instead of
# inside a confusing `cargo build --target` error.
MISSING=""
if [ $# -gt 0 ]; then
    INSTALLED="$(rustup target list --installed)"
    for t in "$@"; do
        printf '%s\n' "$INSTALLED" | grep -qx "$t" || MISSING="${MISSING} $t"
    done
fi
if [ -n "${MISSING# }" ]; then
    echo "::error::ci-assert-pinned-toolchain: ${CHANNEL} is missing target(s):${MISSING}" >&2
    echo "::error::rust-toolchain.toml is supposed to provision these on first use — check its [toolchain].targets list." >&2
    exit 1
fi
[ $# -gt 0 ] && echo "ci-assert-pinned-toolchain: verified target(s): $*"

# Components the migrated lanes rely on. Printed, not asserted by name: the
# version strings above are the real proof they came from the pinned toolchain,
# and a lane that needs a component it lacks fails on its own command anyway.
echo "  rustfmt: $(cargo fmt --version 2>&1 | head -1)"
echo "  clippy : $(cargo clippy --version 2>&1 | head -1)"
