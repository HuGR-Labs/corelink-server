#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PAGE="$ROOT/apps/docs/docs/integrations/bazel.md"
HELPER="$ROOT/examples/bazel-starter/.bazel/corelink-credential-helper.sh"
PUBLISHED_HELPER="$ROOT/apps/docs/static/downloads/corelink-credential-helper.sh"
BAZELRC="$ROOT/examples/bazel-starter/.bazelrc"

assert_all_helpers_scoped() {
  local file="$1" line value
  while IFS= read -r line; do
    case "$line" in
      build\ --credential_helper=*)
        value="${line#build --credential_helper=}"
        case "$value" in
          *=*) ;;
          *) return 1 ;;
        esac
        ;;
    esac
  done <"$file"
}

assert_helper_link() {
  local page="$1" href target
  href="$(sed -nE 's#.*\[Bazel starter example\]\(([^)]*)\).*#\1#p' "$page")"
  test "$href" = 'pathname:///downloads/corelink-credential-helper.sh' || return 1
  target="$PUBLISHED_HELPER"
  test -f "$target" || return 1
}

assert_safe() {
  local page="$1"
  grep -qE '^[^#]*credential_helper=corelink-api\.humangr\.com=' "$page" || return 1
  assert_all_helpers_scoped "$page" || return 1
  if grep -qE '^[^#]*remote_header=Authorization=Bearer' "$page"; then return 1; fi
  if grep -qE '^[^#]*curl[^#]*(--header|-H)[^#]*Authorization:[[:space:]]*Bearer[^#]*\$(\{)?CORELINK_PAT(\})?' "$page"; then return 1; fi
  if grep -qE '^[^#]*(set[[:space:]]+-x|xtrace|echo[^#]*\$CORELINK_PAT|printf[^#]*\$CORELINK_PAT)' "$page"; then return 1; fi
  grep -q 'curl --silent --config -' "$page"
}

assert_safe "$PAGE"
assert_helper_link "$PAGE"
test -f "$HELPER"
cmp --silent "$HELPER" "$PUBLISHED_HELPER"
assert_all_helpers_scoped "$BAZELRC"
grep -qE '^build --credential_helper=corelink-api\.humangr\.com=' "$BAZELRC"
sentinel='corelink_pat_SENTINEL.behavior-test'
helper_stderr="$(mktemp)"
missing_stderr="$(mktemp)"
helper_xtrace_stderr="$(mktemp)"
trap 'rm -f "$helper_stderr" "$missing_stderr" "$helper_xtrace_stderr"' EXIT
helper_stdout="$(printf '{\"uri\":\"https://corelink-api.humangr.com/bazel/cache\"}\n' | env LC_ALL=C LANG=C CORELINK_PAT="$sentinel" bash "$HELPER" 2>"$helper_stderr")"
test "$helper_stdout" = "{\"headers\":{\"Authorization\":[\"Bearer $sentinel\"]}}"
if [ -s "$helper_stderr" ]; then
  echo 'B-157 helper wrote unexpected stderr' >&2
  exit 1
fi
if missing_stdout="$(env -u CORELINK_PAT LC_ALL=C LANG=C bash "$HELPER" </dev/null 2>"$missing_stderr")"; then
  echo 'B-157 helper unexpectedly accepted missing PAT' >&2
  exit 1
fi
test -z "$missing_stdout"
if grep -q "$sentinel" "$missing_stderr"; then
  echo 'B-157 missing-PAT diagnostic leaked sentinel' >&2
  exit 1
fi
grep -q 'CORELINK_PAT environment variable is not set' "$missing_stderr"

xtrace_stdout="$(printf '{\"uri\":\"https://corelink-api.humangr.com/bazel/cache\"}\n' | env LC_ALL=C LANG=C CORELINK_PAT="$sentinel" bash -x "$HELPER" 2>"$helper_xtrace_stderr")"
test "$xtrace_stdout" = "{\"headers\":{\"Authorization\":[\"Bearer $sentinel\"]}}"
if grep -q "$sentinel" "$helper_xtrace_stderr"; then
  echo 'B-157 helper xtrace leaked sentinel' >&2
  exit 1
fi

tmp="$(mktemp)"
trap 'rm -f "$tmp" "$helper_stderr" "$missing_stderr" "$helper_xtrace_stderr"' EXIT
awk '{gsub(/pathname:\/\/\/downloads\/corelink-credential-helper\.sh/, "pathname:///downloads/missing-helper.sh"); print}' "$PAGE" >"$tmp"
if assert_helper_link "$tmp"; then
  echo 'B-157 link mutation unexpectedly passed' >&2
  exit 1
fi
cp "$BAZELRC" "$tmp"
printf '\nbuild --credential_helper=%%workspace%%/.bazel/leaky-helper.sh\n' >>"$tmp"
if assert_all_helpers_scoped "$tmp"; then
  echo 'B-157 unscoped helper mutation unexpectedly passed' >&2
  exit 1
fi
cp "$PAGE" "$tmp"
printf '\nbuild --credential_helper=%%workspace%%/.bazel/leaky-helper.sh\n' >>"$tmp"
if assert_safe "$tmp"; then
  echo 'B-157 page unscoped helper mutation unexpectedly passed' >&2
  exit 1
fi
cp "$PAGE" "$tmp"
for pat_ref in '$CORELINK_PAT' '${CORELINK_PAT}'; do
  cp "$PAGE" "$tmp"
  printf '\ncurl -H "Authorization: Bearer %s" https://example.invalid\n' "$pat_ref" >>"$tmp"
  if assert_safe "$tmp"; then
    echo "B-157 PAT argv mutation unexpectedly passed: $pat_ref" >&2
    exit 1
  fi
done
cp "$PAGE" "$tmp"
printf '\nset -x\necho "$CORELINK_PAT"\n' >>"$tmp"
if assert_safe "$tmp"; then
  echo 'B-157 logging mutation unexpectedly passed' >&2
  exit 1
fi
cp "$PAGE" "$tmp"
printf '\nset -o xtrace\necho "$CORELINK_PAT"\n' >>"$tmp"
if assert_safe "$tmp"; then
  echo 'B-157 xtrace mutation unexpectedly passed' >&2
  exit 1
fi
echo 'B-157 credential recipe mutation: PASS'
