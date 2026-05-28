#!/usr/bin/env bash
# scripts/test-affected.sh — run tests ONLY for crates/packages changed since a
# git ref. Dev-loop accelerator for fast feedback; NOT a replacement for the
# full pre-merge gate (scripts/ci.sh). The full gate stays authoritative.
#
# Usage:
#   scripts/test-affected.sh                # vs. working tree + index (default: HEAD)
#   scripts/test-affected.sh main           # vs. main
#   scripts/test-affected.sh origin/main    # vs. a remote ref
#
# Behaviour:
#   - Maps each changed file under crates/<dir>/ to that crate's package name.
#   - If a workspace-wide file changed (root Cargo.toml / Cargo.lock / deny.toml
#     / .cargo/config.toml / rust-toolchain*), runs the FULL nextest suite —
#     those affect everything, so a partial run would be misleading.
#   - Runs the changed Rust crates with `cargo nextest run -p ... -p ...`.
#   - Runs changed JS workspace packages with `pnpm --filter ... test`.
#
# NOTE: this runs the CHANGED crates' own tests. It does not transitively pull
# in dependents — that's a deliberate speed/coverage trade for the dev loop.
# Run scripts/ci.sh before merging.

set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

BASE_REF="${1:-HEAD}"

# Changed files: committed-since-base ∪ staged ∪ unstaged.
mapfile -t CHANGED < <(
    {
        git diff --name-only "$BASE_REF" 2>/dev/null
        git diff --name-only --cached 2>/dev/null
        git diff --name-only 2>/dev/null
    } | sort -u
)

if [ "${#CHANGED[@]}" -eq 0 ]; then
    echo "test-affected: no changes vs '$BASE_REF' — nothing to do." >&2
    exit 0
fi

# Workspace-wide triggers → full suite.
WS_WIDE_RE='^(Cargo\.toml|Cargo\.lock|deny\.toml|\.cargo/config\.toml|rust-toolchain)'
for f in "${CHANGED[@]}"; do
    if [[ "$f" =~ $WS_WIDE_RE ]]; then
        echo "test-affected: '$f' is workspace-wide → running FULL nextest suite." >&2
        exec cargo nextest run --workspace
    fi
done

# Map changed crate files → package names (dedup).
declare -A CRATES=()
declare -A JS_PKGS=()
for f in "${CHANGED[@]}"; do
    if [[ "$f" =~ ^crates/([^/]+)/ ]]; then
        dir="crates/${BASH_REMATCH[1]}"
        if [ -f "$dir/Cargo.toml" ]; then
            name="$(awk -F'"' '/^name[[:space:]]*=/{print $2; exit}' "$dir/Cargo.toml")"
            [ -n "$name" ] && CRATES["$name"]=1
        fi
    elif [[ "$f" =~ ^(worker|apps/[^/]+)/ ]]; then
        JS_PKGS["${BASH_REMATCH[1]}"]=1
    fi
done

rc=0

if [ "${#CRATES[@]}" -gt 0 ]; then
    PKG_ARGS=()
    for c in "${!CRATES[@]}"; do PKG_ARGS+=("-p" "$c"); done
    echo "test-affected: nextest for ${#CRATES[@]} changed crate(s): ${!CRATES[*]}" >&2
    cargo nextest run "${PKG_ARGS[@]}" || rc=$?
else
    echo "test-affected: no changed Rust crates." >&2
fi

if [ "${#JS_PKGS[@]}" -gt 0 ]; then
    for p in "${!JS_PKGS[@]}"; do
        echo "test-affected: pnpm test for JS package '$p'" >&2
        ( cd "$p" && pnpm test ) || rc=$?
    done
fi

exit "$rc"
