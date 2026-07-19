#!/usr/bin/env bash
# public-flip-smoke.sh — automated UNAUTHENTICATED smoke for the public flip.
#
# Layer 1 of the public-flip smoke harness (see scripts/smoke/README.md).
# Pure curl, read-only GETs, NO secrets, NO credentials (CTRL-CRED-001).
#
# WHY THIS EXISTS (azp-incident, 2026-06-10):
#   A latent `401 clerk session azp invalid` on the user-facing host
#   (corelink-app.humangr.com) survived every prior smoke because none of
#   them exercised the REAL public entry points a customer hits. This layer
#   pins the canonical unauthenticated expectations of every prod host;
#   layer 2 (authenticated-smoke.spec.ts) drives the real Clerk session.
#
# Canonical expectations (discovered from worker routes + admin-ui source):
#   corelink-app.humangr.com      OpenNext Worker (apps/admin-ui/wrangler.toml
#                                 routes[]) — public app entry point.
#       /                  → 200 + security headers (middleware.ts
#                            STATIC_SECURITY_HEADERS + CSP, src/lib/csp.ts)
#       /upgrade?plan=solo → 307 to /en/upgrade?plan=solo
#                            (src/app/upgrade/route.ts), redirect chain ends
#                            200 (signed-out → Clerk /sign-in round-trip per
#                            src/app/[locale]/upgrade/page.tsx)
#       /sign-up           → 200 (Clerk widget page)
#   humangr.com    same OpenNext Worker (operator domain).
#       /                  → 200 + security headers
#   corelink-docs.humangr.com     docs Pages (apps/docs/docusaurus.config.ts).
#       /                  → 200
#   corelink-api.humangr.com      main Worker (wrangler.toml env.prod.routes).
#       /                  → 404 fail-closed JSON (NOT_FOUND) — the API root
#                            must NEVER serve content unauthenticated
#       /health            → 200, body contains "status":"ok"
#                            (worker/src/index.ts health route)
#       /_health           → 200 (smoke-prod deep-probe path)
#
# Usage:
#   bash scripts/smoke/public-flip-smoke.sh
#
# Exit code: 0 = all probes PASS; 1 = at least one FAIL.

set -euo pipefail

APP_URL="https://corelink-app.humangr.com"
ADMIN_URL="https://humangr.com"
DOCS_URL="https://corelink-docs.humangr.com"
API_URL="https://corelink-api.humangr.com"

TIMEOUT=15
UA="corelink-public-flip-smoke/1.0"

FAIL_COUNT=0
PASS_COUNT=0

pass() { printf '[PASS] %s\n' "$*"; PASS_COUNT=$((PASS_COUNT + 1)); }
fail() { printf '[FAIL] %s\n' "$*" >&2; FAIL_COUNT=$((FAIL_COUNT + 1)); }
step() { printf '\n── %s ──\n' "$*"; }

# http_code URL [extra curl args...] — status code or 000 on transport error.
http_code() {
  local url="$1"
  shift
  curl -s -o /dev/null -m "$TIMEOUT" -A "$UA" -w '%{http_code}' "$@" "$url" \
    || printf '000'
}

# probe_status NAME URL EXPECTED [extra curl args...]
probe_status() {
  local name="$1" url="$2" expected="$3"
  shift 3
  local code
  code=$(http_code "$url" "$@")
  if [ "$code" = "$expected" ]; then
    pass "$name → $code"
  else
    fail "$name → $code (expected $expected)"
  fi
}

# probe_security_headers NAME URL — assert the admin-ui hardening headers
# (middleware.ts STATIC_SECURITY_HEADERS + the CSP) are present. Values are
# matched loosely where infra (CF zone settings) may legitimately rewrite
# them (e.g. HSTS max-age), strictly where drift would be a regression.
probe_security_headers() {
  local name="$1" url="$2"
  local headers
  headers=$(curl -sI -m "$TIMEOUT" -A "$UA" "$url" | tr -d '\r') || {
    fail "$name security headers → curl transport error"
    return
  }
  local h_ok=1
  local missing=""
  check_header() {
    local pattern="$1" label="$2"
    if ! printf '%s\n' "$headers" | grep -qiE "$pattern"; then
      h_ok=0
      missing="$missing $label"
    fi
  }
  check_header '^x-frame-options: *deny' "X-Frame-Options=DENY"
  check_header '^x-content-type-options: *nosniff' "X-Content-Type-Options=nosniff"
  check_header '^strict-transport-security:.*max-age=' "Strict-Transport-Security"
  check_header '^referrer-policy: *strict-origin-when-cross-origin' "Referrer-Policy"
  check_header '^permissions-policy:.*interest-cohort' "Permissions-Policy"
  check_header '^content-security-policy:.*frame-ancestors' "Content-Security-Policy"
  if [ "$h_ok" = 1 ]; then
    pass "$name security headers (XFO/XCTO/HSTS/Referrer/Permissions/CSP)"
  else
    fail "$name security headers — missing:$missing"
  fi
}

printf 'public-flip-smoke — %s\n' "$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

# ── corelink-app.humangr.com (public app entry point) ──────────────────────
step "corelink-app.humangr.com (OpenNext Worker — public app)"
probe_status "app landing GET /" "$APP_URL/" 200
probe_security_headers "app landing" "$APP_URL/"

# /upgrade?plan=solo — the money path's front door (#49). Two assertions:
#   (1) the locale-less forwarder must 307 to /en/upgrade?plan=solo
#   (2) the full redirect chain (signed-out → /sign-in round-trip) ends 200.
# A 404 here means the admin-ui deploy is missing the /upgrade route — the
# exact class of miss this harness exists to catch.
upgrade_first_hop=$(curl -s -o /dev/null -m "$TIMEOUT" -A "$UA" \
  -w '%{http_code} %{redirect_url}' "$APP_URL/upgrade?plan=solo" || printf '000')
upgrade_code=${upgrade_first_hop%% *}
upgrade_loc=${upgrade_first_hop#* }
if [ "$upgrade_code" = "307" ] && printf '%s' "$upgrade_loc" | grep -q '/en/upgrade?plan=solo'; then
  pass "app GET /upgrade?plan=solo → 307 /en/upgrade?plan=solo"
else
  fail "app GET /upgrade?plan=solo → $upgrade_code ${upgrade_loc:-<no Location>} (expected 307 → /en/upgrade?plan=solo)"
fi
probe_status "app GET /upgrade?plan=solo (follow redirects → sign-in)" \
  "$APP_URL/upgrade?plan=solo" 200 -L
probe_status "app GET /sign-up" "$APP_URL/sign-up" 200

# ── humangr.com (operator domain, same Worker) ──────────────
step "humangr.com (OpenNext Worker — operator domain)"
probe_status "admin landing GET /" "$ADMIN_URL/" 200
probe_security_headers "admin landing" "$ADMIN_URL/"

# ── corelink-docs.humangr.com (docs Pages) ──────────────────────────────────
step "corelink-docs.humangr.com (docs Pages)"
probe_status "docs landing GET /" "$DOCS_URL/" 200

# ── corelink-api.humangr.com (main Worker) ───────────────────────────────────
step "corelink-api.humangr.com (main Worker)"
# Fail-closed root: anything other than 404 means an unauthenticated surface
# opened up (or the route binding broke).
probe_status "api GET / (fail-closed root)" "$API_URL/" 404
health_body=$(curl -s -m "$TIMEOUT" -A "$UA" "$API_URL/health" || printf '')
health_code=$(http_code "$API_URL/health")
if [ "$health_code" = "200" ] && printf '%s' "$health_body" | grep -q '"status":"ok"'; then
  pass "api GET /health → 200 + status:ok"
else
  fail "api GET /health → $health_code (expected 200 + \"status\":\"ok\"; body: ${health_body:0:120})"
fi
probe_status "api GET /_health" "$API_URL/_health" 200

# ── Summary ──────────────────────────────────────────────────────────────────
printf '\n── summary ──\n'
printf '%d PASS / %d FAIL\n' "$PASS_COUNT" "$FAIL_COUNT"
if [ "$FAIL_COUNT" -gt 0 ]; then
  printf 'PUBLIC-FLIP SMOKE: RED — do not flip / roll back the deploy.\n' >&2
  exit 1
fi
printf 'PUBLIC-FLIP SMOKE: GREEN (unauthenticated layer). Run the authenticated\n'
printf 'layer next: see scripts/smoke/README.md (authenticated-smoke.spec.ts).\n'
