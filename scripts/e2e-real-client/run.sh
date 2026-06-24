#!/usr/bin/env bash
# run.sh — the GO-LIVE MOAT: a real-CLIENT conformance harness for CoreLink.
#
# Drives the ACTUAL client toolchains (docker, cargo+sccache, brew, curl) end to
# end against PROD as a repeatable SHIP / NO-SHIP go-live certificate. It codifies
# the manual validation that already proved docker login+push+pull, cargo writes,
# and brew all work — so the launch can re-run a single command and get a verdict.
#
# What it does:
#   1. Bootstrap a real user the REAL way (no internal mints): create a Clerk user
#      via the Clerk Backend API → poll the user's Clerk private_metadata for the
#      signup-worker-provisioned tenant + RW PAT. Also create a SECOND user for
#      cross-tenant tests. DSR-delete both at the end (Clerk DELETE → erasure).
#   2. Exercise each real client (docker / cargo+sccache / brew / native CAS+AC /
#      bazel REAPI v2 / turbo), each a PASS / GATED / FAIL line. GATED if the tool
#      is absent or a known client/env limitation; FAIL only on a real contract
#      violation.
#   3. Emit a SHIP / NO-SHIP cert (FAIL ⇒ NO-SHIP, exit non-zero; GATED never
#      blocks) + a machine-readable JSON summary.
#
# Idempotent + safe to re-run (content-addressed payloads are unique per run;
# users are throwaway + cleaned up). Black-box only: HTTP + the real binaries.
#
# Usage:
#   bash scripts/e2e-real-client/run.sh                 # full run vs prod
#   bash scripts/e2e-real-client/run.sh --help
#   CORELINK_E2E_API_HOST=… CORELINK_E2E_OCI_HOST=… bash scripts/e2e-real-client/run.sh
#
# Credentials (env first, then .env.local): CLERK_LIVE_SECRET_KEY or
# CLERK_SECRET_KEY. No secret is ever echoed.
#
# Exit codes:
#   0 — SHIP    (no FAIL AND PASS ≥ CORELINK_E2E_MIN_PASS floor, GATED allowed)
#   1 — NO-SHIP (≥1 FAIL, OR a below-floor "green-by-vacuum" run: PASS < floor.
#                Default floor = 1, so a run that gates EVERYTHING — e.g. no
#                Clerk secret — is NO-SHIP, not a silent green. Set
#                CORELINK_E2E_MIN_PASS=0 to allow an all-gated run to SHIP.)
#   2 — could not even start (missing curl/jq/openssl)
#   3 — usage error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
ENV_FILE="${REPO_ROOT}/.env.local"

# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"
# shellcheck source=lib/clerk.sh
. "${SCRIPT_DIR}/lib/clerk.sh"
# shellcheck source=lib/clients.sh
. "${SCRIPT_DIR}/lib/clients.sh"

# ── config ───────────────────────────────────────────────────────────────────
API_HOST="${CORELINK_E2E_API_HOST:-https://corelink-api.humangr.com}"
OCI_HOST="${CORELINK_E2E_OCI_HOST:-https://corelink-oci.humangr.com}"
# OUT_JSON is read by emit_cert (defined in lib/common.sh, sourced above).
# shellcheck disable=SC2034
OUT_JSON="${CORELINK_E2E_OUT_JSON:-${SCRIPT_DIR}/last-run.json}"
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/e2e-real-client.XXXXXX")"
export WORKDIR

# Populated during bootstrap.
USER_A=""; USER_B=""
PAT=""; TENANT=""; PAT_B=""; TENANT_B=""
CLERK_SECRET=""

usage() {
  sed -n '2,40p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

# ── trap: DSR-delete both users + scrub the workdir, then write the cert ──────
# Invoked indirectly via `trap cleanup EXIT`.
# shellcheck disable=SC2329
cleanup() {
  local rc=$?
  step "cleanup — DSR-delete test users + scrub scratch"
  if [ -n "${CLERK_SECRET}" ]; then
    [ -n "$USER_A" ] && clerk_delete_user "$USER_A"
    [ -n "$USER_B" ] && clerk_delete_user "$USER_B"
  fi
  rm -rf "$WORKDIR" 2>/dev/null || true
  exit "$rc"
}
trap cleanup EXIT

# ── arg parse ────────────────────────────────────────────────────────────────
case "${1:-}" in
  -h|--help) usage 0 ;;
  "" ) : ;;
  *  ) warn "unknown argument: $1"; usage 3 ;;
esac

# ── load Clerk secret (env first, then .env.local) ───────────────────────────
step "0 — load credentials"
for cmd in curl jq openssl; do
  if ! have "$cmd"; then
    fail "bootstrap" "deps" "required command not found: ${cmd}"
    echo "FATAL: ${cmd} is required to bootstrap a real user." >&2
    exit 2
  fi
done

CLERK_SECRET="${CLERK_LIVE_SECRET_KEY:-${CLERK_SECRET_KEY:-}}"
if [ -z "$CLERK_SECRET" ] && [ -f "$ENV_FILE" ]; then
  # Source only the two keys we care about (don't pollute env).
  while IFS='=' read -r key val; do
    case "$key" in
      \#*|"") continue ;;
    esac
    key="${key//[[:space:]]/}"; val="${val//[[:space:]]/}"
    case "$key" in
      CLERK_LIVE_SECRET_KEY) [ -z "$CLERK_SECRET" ] && CLERK_SECRET="$val" ;;
      CLERK_SECRET_KEY)      [ -z "$CLERK_SECRET" ] && CLERK_SECRET="$val" ;;
    esac
  done < "$ENV_FILE"
fi

if [ -z "$CLERK_SECRET" ]; then
  # No Clerk secret → we cannot bootstrap a real user. The WHOLE run gates
  # (recorded, not a silent skip). This used to exit 0 "SHIP" — a green that
  # asserted NOTHING. The anti-vacuum floor in emit_cert now flips this to
  # NO-SHIP (vacuum) by default (0 PASS < floor 1), so a credential-less run
  # can no longer false-certify a launch. Set CORELINK_E2E_MIN_PASS=0 to opt
  # into the old "all-gated is acceptable" behaviour in throwaway environments.
  gated "bootstrap" "clerk user" "no CLERK_LIVE_SECRET_KEY / CLERK_SECRET_KEY (env or .env.local) — cannot provision a real user"
  emit_cert
  exit "$E2E_VERDICT_RC"
fi
pass "bootstrap" "credentials" "Clerk secret loaded (last4=$(last4 "$CLERK_SECRET"))"
info "API host: ${API_HOST}"
info "OCI host: ${OCI_HOST}"

# ── bootstrap: two real users via the real signup path ───────────────────────
step "1 — bootstrap real user A (primary tenant)"
ts=$(date +%s)
EMAIL_A="e2e-real-${ts}-a@example.com"
EMAIL_B="e2e-real-${ts}-b@example.com"

if USER_A=$(clerk_create_user "$EMAIL_A"); then
  pass "bootstrap" "clerk user A" "created (${USER_A})"
else
  fail "bootstrap" "clerk user A" "Clerk user creation failed for ${EMAIL_A}"
  emit_cert; exit 1
fi

if clerk_poll_provisioning "$USER_A"; then
  PAT="$CLERK_PAT"; TENANT="$CLERK_TENANT"
  pass "bootstrap" "provision A" "tenant + RW PAT provisioned for user A"
else
  fail "bootstrap" "provision A" "signup-worker did not provision tenant+PAT for user A within timeout"
  emit_cert; exit 1
fi

step "2 — bootstrap real user B (second tenant, cross-tenant tests)"
if USER_B=$(clerk_create_user "$EMAIL_B"); then
  pass "bootstrap" "clerk user B" "created (${USER_B})"
  if clerk_poll_provisioning "$USER_B"; then
    PAT_B="$CLERK_PAT"; TENANT_B="$CLERK_TENANT"
    pass "bootstrap" "provision B" "tenant + RW PAT provisioned for user B"
  else
    gated "bootstrap" "provision B" "user B not provisioned in time — cross-tenant probes will GATE"
  fi
else
  gated "bootstrap" "clerk user B" "could not create second user — cross-tenant probes will GATE"
fi
export PAT TENANT PAT_B TENANT_B API_HOST OCI_HOST

# ── exercise every real client ───────────────────────────────────────────────
probe_identity
probe_native_cas
probe_bazel
probe_turbo
probe_docker
probe_cargo_sccache
probe_brew

# ── verdict + cert (cleanup trap writes nothing; we emit here) ────────────────
emit_cert

# E2E_VERDICT_RC is 1 on any FAIL OR a below-floor (vacuum) run, else 0.
exit "$E2E_VERDICT_RC"
