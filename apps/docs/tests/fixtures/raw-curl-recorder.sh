#!/bin/sh
# WP-B163 curl recorder: no network, no credential capture, bytes only.

set -eu
: "${CORELINK_CURL_RECORDER:?CORELINK_CURL_RECORDER must be set}"
url=
body=
while [ "$#" -gt 0 ]; do
  case "$1" in
    --data-binary)
      [ "$#" -ge 2 ] || exit 64
      body=${2#@}
      shift 2
      ;;
    -X|--request|-H|--header)
      [ "$#" -ge 2 ] || exit 64
      shift 2
      ;;
    http://*|https://*)
      url=$1
      shift
      ;;
    *)
      shift
      ;;
  esac
done
[ -n "$url" ] || { echo "recorder: zero URL" >&2; exit 64; }
[ -n "$body" ] && [ -f "$body" ] || {
  echo "recorder: zero/unparsed body" >&2
  exit 64
}
cp "$body" "$CORELINK_CURL_RECORDER/uploaded.bin"
printf '%s\n' "$url" > "$CORELINK_CURL_RECORDER/url"
printf '%s\n' PUT > "$CORELINK_CURL_RECORDER/method"
printf '%s\n' "$(b3sum "$body" | awk '{print $1}')" \
  > "$CORELINK_CURL_RECORDER/uploaded.digest"
