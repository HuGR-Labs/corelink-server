#!/usr/bin/env bash
# WP-B163 focused fixture for the raw-curl directory-upload documentation.
# It intentionally has no credentials or network dependency: tests put a
# curl recorder earlier in PATH and inspect the bytes it was given.

set -eu

: "${CORELINK_PAT:?CORELINK_PAT must be set}"
: "${CORELINK_TENANT:?CORELINK_TENANT must be set}"
: "${CORELINK_BASE:?CORELINK_BASE must be set}"

ARCHIVE="$(mktemp "${TMPDIR:-/tmp}/corelink-dist.XXXXXX")"
trap 'rm -f "$ARCHIVE"' EXIT
tar -czf "$ARCHIVE" ./dist/

DIGEST="$(b3sum "$ARCHIVE" | awk '{print $1}')"
[ -n "$DIGEST" ] || { echo "Could not compute an archive digest" >&2; exit 1; }
case "$DIGEST" in
  ''|*[!0-9a-f]*)
    echo "Could not compute a valid archive digest" >&2
    exit 1
    ;;
esac

curl --fail-with-body -sS -X PUT \
  -H "Authorization: Bearer $CORELINK_PAT" \
  -H "Content-Type: application/octet-stream" \
  --data-binary "@$ARCHIVE" \
  "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$DIGEST"

printf 'Uploaded as %s\n' "$DIGEST"
