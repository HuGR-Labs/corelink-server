#!/usr/bin/env bash
# test-wrangler-smoke.sh — Phase B wrangler dev smoke test.
#
# Starts wrangler dev --local, waits for it to be ready, runs smoke curl
# checks, then tears down. Exits 0 on success, 1 on failure.
#
# Usage: bash scripts/test-wrangler-smoke.sh

set -euo pipefail

WRANGLER="${WRANGLER:-worker/node_modules/.bin/wrangler}"
PORT="${PORT:-8787}"
BASE_URL="http://localhost:${PORT}"
TMPLOG=$(mktemp /tmp/wrangler-smoke-XXXXXX.log)

cleanup() {
  if [[ -n "${DEV_PID:-}" ]]; then
    kill "$DEV_PID" 2>/dev/null || true
  fi
  rm -f "$TMPLOG"
}
trap cleanup EXIT

echo "[smoke] Starting wrangler dev --local on port $PORT ..."
"$WRANGLER" dev \
  --local \
  --config wrangler.toml \
  --env="" \
  --port "$PORT" \
  --inspector-port=0 \
  --enable-containers=false \
  2>&1 | tee "$TMPLOG" &
DEV_PID=$!

# Wait for ready signal
TIMEOUT=30
ELAPSED=0
until grep -q "Ready on" "$TMPLOG" 2>/dev/null; do
  sleep 1
  ELAPSED=$((ELAPSED + 1))
  if [[ $ELAPSED -ge $TIMEOUT ]]; then
    echo "[smoke] FAIL: wrangler dev did not become ready within ${TIMEOUT}s"
    exit 1
  fi
done

echo "[smoke] wrangler dev ready. Running smoke checks ..."

# Test 1: GET /health → 200
HEALTH=$(curl -sf "${BASE_URL}/health" 2>&1) || {
  echo "[smoke] FAIL: GET /health returned non-200"
  exit 1
}
echo "$HEALTH" | python3 -c "import json,sys; b=json.load(sys.stdin); assert b['status']=='ok', f'bad status: {b}'" || {
  echo "[smoke] FAIL: /health body not {status:ok}: $HEALTH"
  exit 1
}
echo "[smoke] PASS: GET /health → 200 {status:ok}"

# Test 2: GET /v2/ unauthenticated → 401 with OCI error envelope
V2_RESP=$(curl -sf -o /dev/null -w "%{http_code}" "${BASE_URL}/v2/" 2>&1)
if [[ "$V2_RESP" != "401" ]]; then
  echo "[smoke] FAIL: GET /v2/ expected 401, got $V2_RESP"
  exit 1
fi
echo "[smoke] PASS: GET /v2/ unauthenticated → 401"

# Test 3: Unknown path → 404
UNKNOWN_RESP=$(curl -sf -o /dev/null -w "%{http_code}" "${BASE_URL}/totally-unknown-path-xyz" 2>&1)
if [[ "$UNKNOWN_RESP" != "404" ]]; then
  echo "[smoke] FAIL: unknown path expected 404, got $UNKNOWN_RESP"
  exit 1
fi
echo "[smoke] PASS: unknown path → 404"

# Test 4: Docker-Distribution-Api-Version header on /v2/
DDAV=$(curl -sf -I "${BASE_URL}/v2/" 2>&1 | grep -i "docker-distribution-api-version" | tr -d '\r')
if [[ -z "$DDAV" ]]; then
  echo "[smoke] FAIL: missing Docker-Distribution-Api-Version header on /v2/"
  exit 1
fi
echo "[smoke] PASS: Docker-Distribution-Api-Version header present: $DDAV"

echo "[smoke] All smoke checks PASSED."
