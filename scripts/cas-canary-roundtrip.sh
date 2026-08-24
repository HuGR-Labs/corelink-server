#!/usr/bin/env bash
# Authenticated CAS BLAKE3 round-trip against prod — the body of
# `.github/workflows/cas-datacenter-canary`, extracted so the SAME probe can run
# from more than one vantage point without being duplicated (and so `bash -n` +
# the shell-pipeline-safety gate cover it).
#
# The vantage point is the whole point of this canary, so the caller must name
# it: CANARY_VANTAGE is printed with the result and decides nothing else.
#
# Required env: PAT, API, TEN, CANARY_VANTAGE. Optional: RUN_TAG (defaults to $$).
#
# Exit 0 = round-trip OK. Exit 1 = a real failure, with the origin's own error
# envelope dumped (see the 2026-08-01 outage: the canary shouted "500" and left
# no forensic trail at all).
set -uo pipefail

: "${PAT:?CORELINK_CANARY_PAT not set}"
: "${API:?API base not set}"
: "${TEN:?canary tenant not set}"
: "${CANARY_VANTAGE:?CANARY_VANTAGE not set (which network is this probe speaking from?)}"
RUN_TAG="${RUN_TAG:-$$}"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cd "$WORK" || exit 1

# Install b3sum into the scratch dir rather than /usr/local/bin: the hosted
# runner is root and the container fabric is not necessarily, and this probe
# must not need sudo to answer a question about the network.
B3="$WORK/b3sum"
curl -fsSL https://github.com/BLAKE3-team/BLAKE3/releases/download/1.5.4/b3sum_linux_x64_bin -o "$B3"
chmod +x "$B3"

echo "vantage: $CANARY_VANTAGE"
echo "runner public IP: $(curl -s --max-time 10 https://api.ipify.org)"

printf 'cas-canary %s' "$RUN_TAG" > blob.bin
DIGEST=$("$B3" blob.bin | awk '{print $1}')

echo "== PUT =="
put=$(curl -s -o put.out -D put.hdr -w '%{http_code}' -X PUT \
  -H "Authorization: Bearer $PAT" -H "Content-Type: application/octet-stream" \
  --data-binary @blob.bin --max-time 20 "$API/v1/cas/$TEN/$DIGEST")
echo "PUT http=$put"
echo "== GET =="
get=$(curl -s -o got.bin -D get.hdr -w '%{http_code}' \
  -H "Authorization: Bearer $PAT" --max-time 20 "$API/v1/cas/$TEN/$DIGEST")
echo "GET http=$get bytes=$(wc -c < got.bin)"

fail=0
case "$put" in 201|409) ;; *) echo "::error::CAS PUT expected 201/409, got $put"; fail=1;; esac
[ "$get" = "200" ] || { echo "::error::CAS GET expected 200, got $get"; fail=1; }
cmp -s blob.bin got.bin || { echo "::error::CAS round-trip byte mismatch"; fail=1; }

if [ "$fail" = "1" ]; then
  # Redaction: bodies are the API's typed error envelope (code + detail +
  # request id) and never echo credentials — but redact anything token-shaped
  # anyway, since this log is world-readable on a public-workflow re-run.
  # INV-NO-PII-IN-LOGS.
  redact() { sed -E 's/(corelink_pat_[A-Za-z0-9_-]+|Bearer +[A-Za-z0-9._-]+)/[REDACTED]/g'; }

  echo "== origin error envelope (PUT) =="
  head -c 2000 put.out | redact || true; echo
  echo "== origin error envelope (GET) =="
  head -c 2000 got.bin | redact || true; echo

  echo "== CoreLink origin headers (which plane answered, and why) =="
  grep -iE 'x-corelink-|server-timing|x-request-id|cf-worker' put.hdr get.hdr | redact \
    || echo "  (none — the response never reached a CoreLink handler)"

  echo "== response headers (edge-challenge / rate-limit markers) =="
  grep -iE 'cf-ray|cf-mitigated|server:|retry-after|cf-cache' put.hdr get.hdr || true

  case "$put$get" in
    *500*|*502*|*503*|*504*)
      echo "::error::ORIGIN 5xx — this is NOT an edge/bot/rate-limit block (those are 403/429/challenge). The request authenticated and reached CoreLink, then the origin failed. Read the error envelope above; then check the container roll state (running image vs the wrangler.toml pin) and the D1 migration ledger."
      ;;
  esac
  if grep -i 'cf-mitigated: *challenge' put.hdr get.hdr >/dev/null; then
    echo "::error::Cloudflare CHALLENGE on the cache API — Bot Fight Mode / WAF re-enabled for corelink-api? See docs/operator/edge-security-per-plane-sota.md"
  fi
  if [ "$put" = "429" ] || [ "$get" = "429" ]; then
    echo "::error::429 rate-limited — is corelink-api back in the Wave-32 rate-limit rule scope?"
  fi
  exit 1
fi
echo "✅ CAS round-trip OK from $CANARY_VANTAGE (PUT $put / GET $get / bytes identical)"
