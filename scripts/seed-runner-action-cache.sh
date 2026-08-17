#!/usr/bin/env bash
#
# seed-runner-action-cache.sh — pre-populate the self-hosted runner's
# read-only action-archive cache so jobs stop cold-downloading pinned
# actions from codeload.github.com on every run.
#
# WHY THIS EXISTS
# ---------------
# All our self-hosted runners egress through ONE IP (the shared Mac). When a
# broad PR fans out ~20 gates at once, each job's "Set up job" step downloads
# `actions/checkout` (and friends) from codeload.github.com. Concurrent cold
# downloads on a single IP trip codeload's per-IP rate limit:
#
#     ##[error]Response status code does not indicate success:
#     429 (Too Many Requests).
#     Failed to download archive 'actions/checkout/...' after 3 attempts.
#
# The job then fails at setup having run NO test — a self-inflicted red that
# looks like a real failure. It cleared #1133 only after ~25-min drain cycles.
#
# THE FIX (proven by use 2026-08-17)
# ----------------------------------
# The runner (>= 2.336) honours `ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE=<dir>`: a
# READ-ONLY, pre-seeded mirror of action archives. On each `uses:` it probes
#     <dir>/<owner>_<repo>/<sha>.tar.gz
# and, on a hit, extracts locally instead of hitting codeload — so no download,
# so no 429. It NEVER writes back (verified against actions/runner source), so
# the cache must be seeded out-of-band. That is this script.
#
# It is idempotent: only missing tarballs are fetched, so re-run it freely
# after a dependabot pin bump introduces a new SHA.
#
# SETUP (once per runner host, alongside this seeding)
# ----------------------------------------------------
# In each corelink-server runner's `.env` (e.g. ~/.gh-runners/runner-N/.env):
#     ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE=/Users/<you>/.gh-runners/_action_archive_cache
#     ACTIONS_RUNNER_SYMLINK_CACHED_ACTIONS=1
# then restart the listener so it re-reads .env
#     launchctl kickstart -k gui/$(id -u)/actions.runner.<org>-corelink-server.corelink-builder-N
# The ephemeral CF/Firecracker runners get the same cache baked into the
# corelink-runners image (separate repo) — see that repo's image build.
#
# Usage:
#   ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE=/path/to/cache ./scripts/seed-runner-action-cache.sh
#   ./scripts/seed-runner-action-cache.sh --cache /path/to/cache
#
# Requires: gh (authenticated), gzip. Reads pins from .github/workflows/.
set -euo pipefail

CACHE="${ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE:-}"
while [ $# -gt 0 ]; do
  case "$1" in
    --cache) CACHE="$2"; shift 2 ;;
    -h|--help) grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

if [ -z "$CACHE" ]; then
  echo "ERROR: set ACTIONS_RUNNER_ACTION_ARCHIVE_CACHE or pass --cache <dir>" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
wf_dir="$repo_root/.github/workflows"
[ -d "$wf_dir" ] || { echo "ERROR: no $wf_dir" >&2; exit 2; }

mkdir -p "$CACHE"
echo "seeding action-archive cache: $CACHE"

# All SHA-pinned refs referenced by any workflow (owner/repo[/subpath]@40-hex).
# Captures subpath actions (github/codeql-action/analyze@sha) too; the leading
# `uses:` and reusable-workflow refs (…/.github/workflows/foo.yml@sha, which are
# NOT downloaded through the action-archive cache) are filtered below.
pins="$(grep -rhoE '[A-Za-z0-9._-]+/[A-Za-z0-9._/.-]+@[0-9a-f]{40}' "$wf_dir" \
          | grep -vE '\.ya?ml@[0-9a-f]{40}$' \
          | sort -u)"

fetched=0 hits=0 fail=0
while IFS= read -r pin; do
  [ -n "$pin" ] || continue
  path="${pin%@*}"             # owner/repo[/subpath]
  sha="${pin#*@}"              # 40-hex
  # The runner caches by REPO (ResolvedNameWithOwner) — first two path
  # segments — regardless of any subpath after it.
  owner_repo="$(printf '%s' "$path" | cut -d/ -f1-2)"
  dir="$CACHE/${owner_repo/\//_}"
  out="$dir/$sha.tar.gz"
  if [ -f "$out" ] && gzip -t "$out" 2>/dev/null; then
    hits=$((hits + 1)); continue
  fi
  mkdir -p "$dir"
  if gh api "repos/$owner_repo/tarball/$sha" > "$out.tmp" 2>/dev/null && gzip -t "$out.tmp" 2>/dev/null; then
    mv "$out.tmp" "$out"
    fetched=$((fetched + 1))
    echo "  + $owner_repo@${sha:0:12}"
  else
    rm -f "$out.tmp"
    fail=$((fail + 1))
    echo "  ! FAILED $owner_repo@${sha:0:12}" >&2
  fi
done <<< "$pins"

echo "done: $fetched fetched, $hits already-cached, $fail failed"
[ "$fail" -eq 0 ]
