#!/usr/bin/env bash
# generate-changelog.sh — automates the next-release section of CHANGELOG.md
#                        from `git log` between two refs, using Conventional
#                        Commits parsing.
#
# Usage:
#   scripts/generate-changelog.sh <from-ref> [<to-ref>] [--version <vX.Y.Z>]
#
# Examples:
#   scripts/generate-changelog.sh s20-impl-sealed HEAD --version 1.0.0
#   scripts/generate-changelog.sh v1.0.0 HEAD --version 1.0.1
#   scripts/generate-changelog.sh s19-impl-sealed s20-impl-sealed --version 0.20.0
#
# Output: prints a Keep-A-Changelog-formatted section to stdout. To splice
#         it into CHANGELOG.md, redirect into a tmpfile and merge above the
#         existing `## [Unreleased]` block manually (this script never edits
#         CHANGELOG.md in place — committers review the draft first).
#
# Conventional Commits → Keep-A-Changelog mapping:
#   feat:           → ### Added
#   fix:            → ### Fixed
#   perf:           → ### Changed       (perf is a behavior change)
#   refactor:       → ### Changed
#   docs:           → ### Changed       (only if scoped to public surface)
#   security:       → ### Security
#   sec:            → ### Security
#   deprecate:      → ### Deprecated
#   remove:         → ### Removed
#   revert:         → ### Removed
#   BREAKING CHANGE → ### Changed (flagged with **BREAKING** prefix)
#   merge / chore / test / ci / build / style → skipped (internal noise)
#
# Exit codes:
#   0 = success
#   2 = missing required arg
#   3 = invalid ref
#   4 = no commits in range

set -euo pipefail

FROM_REF=""
TO_REF="HEAD"
VERSION=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      VERSION="$2"
      shift 2
      ;;
    -h|--help)
      sed -n '2,40p' "$0"
      exit 0
      ;;
    *)
      if [[ -z "$FROM_REF" ]]; then
        FROM_REF="$1"
      elif [[ "$TO_REF" == "HEAD" ]]; then
        TO_REF="$1"
      else
        echo "error: unexpected positional arg: $1" >&2
        exit 2
      fi
      shift
      ;;
  esac
done

if [[ -z "$FROM_REF" ]]; then
  echo "error: <from-ref> is required" >&2
  echo "usage: $0 <from-ref> [<to-ref>] [--version <vX.Y.Z>]" >&2
  exit 2
fi

git rev-parse --verify "$FROM_REF" >/dev/null 2>&1 || {
  echo "error: invalid from-ref: $FROM_REF" >&2
  exit 3
}
git rev-parse --verify "$TO_REF" >/dev/null 2>&1 || {
  echo "error: invalid to-ref: $TO_REF" >&2
  exit 3
}

DATE="$(git log -1 --format=%cd --date=short "$TO_REF")"
if [[ -z "$VERSION" ]]; then
  VERSION="Unreleased"
  HEADER="## [Unreleased]"
else
  HEADER="## [${VERSION}] - ${DATE}"
fi

# Buckets
declare -a ADDED=()
declare -a CHANGED=()
declare -a DEPRECATED=()
declare -a REMOVED=()
declare -a FIXED=()
declare -a SECURITY=()

# Stream one commit per line:
#   <full-sha><TAB><subject>
# Body inspection (for BREAKING CHANGE) is done with a second `git log` call
# when needed. This avoids the multi-line-in-NUL-record parsing trap.
while IFS=$'\t' read -r sha subject; do
  [[ -z "$sha" ]] && continue
  short_sha="${sha:0:7}"
  body="$(git log -1 --format='%b' "$sha")"

  # Skip merge / chore / ci / test / build / style noise.
  case "$subject" in
    merge*|Merge*|chore:*|ci:*|test:*|build:*|style:*)
      continue
      ;;
  esac

  # Detect Conventional Commits prefix.
  type=""
  cc_re='^([a-zA-Z]+)(\([^)]+\))?!?:[[:space:]]'
  if [[ "$subject" =~ $cc_re ]]; then
    type="${BASH_REMATCH[1]}"
  fi

  # BREAKING CHANGE detection.
  breaking=""
  bang_re='!:'
  if [[ "$subject" =~ $bang_re ]] || grep -q "BREAKING CHANGE" <<<"$body"; then
    breaking="**BREAKING** "
  fi

  entry="- ${breaking}${subject} (\`${short_sha}\`)"

  case "$type" in
    feat) ADDED+=("$entry") ;;
    fix) FIXED+=("$entry") ;;
    perf|refactor) CHANGED+=("$entry") ;;
    docs)
      # Heuristic: only public-surface docs (apps/docs/, README, CHANGELOG)
      if grep -qE "(apps/docs/|README|CHANGELOG)" <<<"$subject"; then
        CHANGED+=("$entry")
      fi
      ;;
    security|sec) SECURITY+=("$entry") ;;
    deprecate) DEPRECATED+=("$entry") ;;
    remove|revert) REMOVED+=("$entry") ;;
    *)
      # No conventional prefix: skip unless it's a `seal(...)` line, which
      # we treat as Added because it lands a WI.
      seal_re='^seal\('
      if [[ "$subject" =~ $seal_re ]]; then
        ADDED+=("$entry")
      fi
      ;;
  esac
done < <(git log --format='%H%x09%s' "${FROM_REF}..${TO_REF}")

if [[ ${#ADDED[@]} -eq 0 && ${#CHANGED[@]} -eq 0 && ${#DEPRECATED[@]} -eq 0 \
   && ${#REMOVED[@]} -eq 0 && ${#FIXED[@]} -eq 0 && ${#SECURITY[@]} -eq 0 ]]; then
  echo "error: no Conventional-Commits entries between ${FROM_REF}..${TO_REF}" >&2
  exit 4
fi

print_section() {
  local heading="$1"
  local -n arr_ref="$2"
  if [[ ${#arr_ref[@]} -gt 0 ]]; then
    echo ""
    echo "### ${heading}"
    echo ""
    for e in "${arr_ref[@]}"; do
      echo "$e"
    done
  fi
}

echo ""
echo "${HEADER}"

print_section "Added"      ADDED
print_section "Changed"    CHANGED
print_section "Deprecated" DEPRECATED
print_section "Removed"    REMOVED
print_section "Fixed"      FIXED
print_section "Security"   SECURITY

echo ""
