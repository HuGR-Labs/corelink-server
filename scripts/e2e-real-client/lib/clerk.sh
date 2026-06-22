#!/usr/bin/env bash
# clerk.sh — bootstrap a real CoreLink user the REAL way (no internal mints).
#
# Flow (black-box, exactly what a paying signup does):
#   1. Create a Clerk user via the Clerk Backend API (POST /v1/users).
#   2. The signup-worker (Svix webhook → signup orchestration) provisions a
#      tenant + a read-write PAT and writes the PAT plaintext into the user's
#      Clerk `private_metadata.pat_plaintext` (scrubbed hourly, so poll fast).
#   3. Poll GET /v1/users/{id} until private_metadata has pat_plaintext +
#      public_metadata has tenant_id → export PAT + tenant.
#
# DSR cleanup: DELETE /v1/users/{id} — Clerk's user.deleted webhook enqueues
# the DSR/GDPR erasure for that tenant (the real account-deletion path), so the
# Clerk delete IS the DSR-delete. Called from run.sh's EXIT trap for both users.
#
# Hard rules:
#   - pat_plaintext is NEVER echoed (only length + last4 via common.sh::last4).
#   - CLERK secret is NEVER echoed.
#   - Every function returns non-zero on failure; the caller decides PASS/GATED.
#
# Requires: curl, jq, and a Clerk secret in CLERK_LIVE_SECRET_KEY or
# CLERK_SECRET_KEY (env or .env.local — loaded by run.sh).
#
# shellcheck shell=bash

CLERK_API="${CLERK_API:-https://api.clerk.com/v1}"
CLERK_POLL_INTERVAL="${CLERK_POLL_INTERVAL:-3}"
CLERK_POLL_MAX="${CLERK_POLL_MAX:-120}"   # seconds; metadata lands well within

# clerk_create_user <email> → echoes the Clerk user id on stdout (nothing else).
# Returns non-zero on any error.
clerk_create_user() {
  local email="$1"
  local password resp uid
  # 24-char password that satisfies Clerk's policy (mixed classes + entropy).
  password="E2e!real$(openssl rand -hex 8)"

  resp=$(curl -sS --max-time 30 \
    -X POST \
    -H "Authorization: Bearer ${CLERK_SECRET}" \
    -H "Content-Type: application/json" \
    -d "{\"email_address\":[\"${email}\"],\"password\":\"${password}\",\"skip_password_checks\":false,\"skip_password_requirement\":false}" \
    "${CLERK_API}/users" 2>&1) || return 1

  uid=$(printf '%s' "$resp" | jq -r '.id // empty' 2>/dev/null || true)
  if [ -z "$uid" ] || [ "$uid" = "null" ]; then
    warn "Clerk user create failed: $(printf '%s' "$resp" | head -c 300)"
    return 1
  fi
  printf '%s' "$uid"
}

# clerk_poll_provisioning <user_id>
#   Polls GET /v1/users/{id} until private_metadata.pat_plaintext AND
#   public_metadata.tenant_id are present. On success sets the GLOBALS:
#       CLERK_PAT      — the read-write PAT plaintext (secret; do not echo)
#       CLERK_TENANT   — the tenant id
#   Returns non-zero on timeout.
clerk_poll_provisioning() {
  local uid="$1"
  local start now elapsed resp pat tenant
  start=$(date +%s)
  CLERK_PAT=""
  CLERK_TENANT=""

  while true; do
    resp=$(curl -sS --max-time 15 \
      -H "Authorization: Bearer ${CLERK_SECRET}" \
      "${CLERK_API}/users/${uid}" 2>&1) || resp=""

    tenant=$(printf '%s' "$resp" | jq -r '.public_metadata.tenant_id // empty' 2>/dev/null || true)
    pat=$(printf '%s' "$resp" | jq -r '.private_metadata.pat_plaintext // empty' 2>/dev/null || true)

    if [ -n "$tenant" ] && [ "$tenant" != "null" ] && \
       [ -n "$pat" ] && [ "$pat" != "null" ]; then
      # Globals consumed by run.sh (the sourcing orchestrator).
      # shellcheck disable=SC2034
      CLERK_TENANT="$tenant"
      # shellcheck disable=SC2034
      CLERK_PAT="$pat"
      now=$(date +%s); elapsed=$((now - start))
      info "provisioned after ${elapsed}s (tenant=${tenant}, PAT len=${#pat} last4=$(last4 "$pat"))"
      return 0
    fi

    now=$(date +%s); elapsed=$((now - start))
    if [ "$elapsed" -ge "$CLERK_POLL_MAX" ]; then
      warn "provisioning not complete after ${CLERK_POLL_MAX}s (tenant='${tenant}', pat_present=$([ -n "$pat" ] && echo yes || echo no))"
      return 1
    fi
    info "poll: waiting for tenant+PAT (${elapsed}s elapsed)…"
    sleep "$CLERK_POLL_INTERVAL"
  done
}

# clerk_delete_user <user_id> → DELETE the user (triggers DSR erasure).
# Best-effort: prints a warn on non-200 but never aborts (called from trap).
clerk_delete_user() {
  local uid="$1"
  [ -z "$uid" ] && return 0
  local code
  code=$(curl -sS -o /dev/null -w "%{http_code}" --max-time 30 \
    -X DELETE \
    -H "Authorization: Bearer ${CLERK_SECRET}" \
    "${CLERK_API}/users/${uid}" 2>&1) || code="curl_error"
  if [ "$code" = "200" ]; then
    info "DSR-delete: Clerk user ${uid} deleted (erasure enqueued)"
  else
    warn "DSR-delete: Clerk DELETE for ${uid} returned ${code} — manual cleanup may be needed"
  fi
}
