#!/usr/bin/env bash
# common.sh — shared logging + PASS/GATED/FAIL accounting + JSON record sink.
#
# Sourced by run.sh and the lib/clients/* probes. Defines the verdict model
# that the whole harness shares (mirrors the Rust e2e suite's PASS/GATED/FAIL):
#
#   PASS  — the real client round-trip succeeded against PROD.
#   GATED — a prerequisite was absent (tool not installed, cred not set, or a
#           known client/env limitation). RECORDED, never silently skipped,
#           NEVER blocks the ship verdict.
#   FAIL  — the client ran and the live contract was violated → NO-SHIP.
#
# No credential is ever echoed by these helpers; callers pass only labels and
# already-redacted detail strings.
#
# shellcheck shell=bash

# ── verdict counters (global) ────────────────────────────────────────────────
E2E_PASS=0
E2E_GATED=0
E2E_FAIL=0

# Accumulated machine-readable result rows (JSON objects, comma-joined later).
# We build a JSON array by hand to avoid a jq dependency for emission (jq is
# still used for parsing API responses where present).
E2E_JSON_ROWS=()

# ── colour helpers (no-op when stdout is not a tty) ──────────────────────────
_C_RED=""; _C_GREEN=""; _C_YELLOW=""; _C_BLUE=""; _C_RESET=""
if [ -t 1 ]; then
  _C_RED='\033[0;31m'; _C_GREEN='\033[0;32m'; _C_YELLOW='\033[1;33m'
  _C_BLUE='\033[0;34m'; _C_RESET='\033[0m'
fi

# ── JSON string escaper (no jq required) ─────────────────────────────────────
# Escapes the minimal set so a label/detail can ride inside a JSON string.
json_escape() {
  local s="$1"
  s="${s//\\/\\\\}"   # backslash first
  s="${s//\"/\\\"}"   # double-quote
  s="${s//$'\n'/ }"   # newline → space (keep one-line records)
  s="${s//$'\t'/ }"   # tab → space
  s="${s//$'\r'/}"    # strip CR
  printf '%s' "$s"
}

# ── result emitters ──────────────────────────────────────────────────────────
# Each records ONE result row: a printed line + a JSON object appended to
# E2E_JSON_ROWS. Args: <surface> <label> [detail].
_record() {
  local status="$1" surface="$2" label="$3" detail="${4:-}"
  E2E_JSON_ROWS+=("$(printf '{"surface":"%s","step":"%s","status":"%s","detail":"%s"}' \
    "$(json_escape "$surface")" \
    "$(json_escape "$label")" \
    "$status" \
    "$(json_escape "$detail")")")
}

pass() {
  # pass <surface> <label> [detail]
  E2E_PASS=$((E2E_PASS + 1))
  printf "${_C_GREEN}[PASS]${_C_RESET}  %-10s %s\n" "$1" "$2"
  _record PASS "$1" "$2" "${3:-}"
}

gated() {
  # gated <surface> <label> <reason>
  E2E_GATED=$((E2E_GATED + 1))
  printf "${_C_YELLOW}[GATED]${_C_RESET} %-10s %s — %s\n" "$1" "$2" "${3:-no reason}"
  _record GATED "$1" "$2" "${3:-}"
}

fail() {
  # fail <surface> <label> <detail>
  E2E_FAIL=$((E2E_FAIL + 1))
  printf "${_C_RED}[FAIL]${_C_RESET}  %-10s %s — %s\n" "$1" "$2" "${3:-violation}" >&2
  _record FAIL "$1" "$2" "${3:-}"
}

info() { printf '        %s\n' "$*"; }
step() { printf "\n${_C_BLUE}==> %s${_C_RESET}\n" "$*"; }
warn() { printf "${_C_YELLOW}[warn]${_C_RESET}  %s\n" "$*" >&2; }

# ── tool gating ──────────────────────────────────────────────────────────────
# have <cmd> → 0 if on PATH, 1 otherwise. Probes use this to GATE (not FAIL)
# when the real client is absent.
have() { command -v "$1" >/dev/null 2>&1; }

# ── HTTP status classifier (the M6 false-confidence fix) ─────────────────────
# Classify one HTTP status string into exactly one verdict class, echoed to
# stdout: "ok2xx" | "client4xx" | "server5xx" | "redirect3xx" | "none".
#
# RULE: a 5xx is NEVER a PASS. The committed last-run.json once graded
# `brew auth … HTTP 502` as PASS — but a 502 is the EXACT `_public` fail-closed
# signature the Rust suite treats as a hard FAIL (the server refused to serve).
# So 5xx ⇒ "server5xx" and every caller must FAIL (or GATED with the explicit
# upstream reason) — never PASS. An empty status (no response) ⇒ "none" (GATED:
# could be a network/connectivity gap, not a contract violation).
classify_http() {
  local code="${1:-}"
  case "$code" in
    "")                          printf 'none' ;;
    2[0-9][0-9])                 printf 'ok2xx' ;;
    3[0-9][0-9])                 printf 'redirect3xx' ;;
    4[0-9][0-9])                 printf 'client4xx' ;;
    5[0-9][0-9])                 printf 'server5xx' ;;
    [0-9]*)                      printf 'other' ;;
    *)                           printf 'none' ;;
  esac
}

# Last 4 chars of a secret, for safe logging.
last4() {
  local s="$1"
  if [ "${#s}" -le 4 ]; then
    printf '****'
  else
    printf '%s' "${s: -4}"
  fi
}

# ── the SHIP / NO-SHIP certificate ───────────────────────────────────────────
# emit_cert — prints the human verdict box + writes the machine-readable JSON
# summary to ${OUT_JSON} (set by run.sh). Verdict: any FAIL ⇒ NO-SHIP; otherwise
# SHIP (GATED never blocks). Safe to call more than once (idempotent write).
# E2E_VERDICT_RC is set by emit_cert to the exit code run.sh should use:
#   0 — SHIP (no FAIL and the pass-floor was met)
#   1 — NO-SHIP (≥1 FAIL, OR a below-floor "green-by-vacuum" run)
# run.sh reads this instead of recomputing the verdict, so the floor cannot be
# bypassed by an exit path that only checks E2E_FAIL.
E2E_VERDICT_RC=0
export E2E_VERDICT_RC  # read cross-file by run.sh's exit paths (silences SC2034)

emit_cert() {
  local verdict ts min_pass below_floor
  ts="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

  # ── anti-vacuum FLOOR (mirrors the Rust suite's CORELINK_E2E_MIN_PASS) ──────
  # A run that GATES everything (no Clerk secret, no tools) used to print
  # "SHIP" and exit 0 — a green that asserted NOTHING (the owner's
  # "green CI ≠ validated" nightmare). Require a floor of real PASSes, else
  # NO-SHIP. Default 1 (at least one positive assertion ran). A provisioned
  # prod run should set this high (e.g. 12) so a silent provisioning/probe
  # regression that re-gates the suite cannot sail through as SHIP.
  min_pass="${CORELINK_E2E_MIN_PASS:-1}"
  case "$min_pass" in
    ''|*[!0-9]*) min_pass=1 ;;
  esac
  below_floor=0
  if [ "$E2E_PASS" -lt "$min_pass" ]; then below_floor=1; fi

  if [ "$E2E_FAIL" -gt 0 ]; then
    verdict="NO-SHIP"
    E2E_VERDICT_RC=1
  elif [ "$below_floor" -eq 1 ]; then
    verdict="NO-SHIP (vacuum)"
    E2E_VERDICT_RC=1
  else
    verdict="SHIP"
    E2E_VERDICT_RC=0
  fi

  printf '\n'
  printf '╔══════════════════════════════════════════════════════════════════╗\n'
  printf '║  CoreLink real-client conformance — GO-LIVE CERTIFICATE           ║\n'
  printf '╠══════════════════════════════════════════════════════════════════╣\n'
  printf '║  PASS: %-4d  GATED: %-4d  FAIL: %-4d                                ║\n' \
    "$E2E_PASS" "$E2E_GATED" "$E2E_FAIL"
  printf '║  VERDICT: %-56s║\n' "$verdict"
  printf '╚══════════════════════════════════════════════════════════════════╝\n'
  if [ "$E2E_FAIL" -gt 0 ]; then
    printf '%bNO-SHIP%b: %d real-client contract violation(s) — DO NOT launch until green.\n' \
      "$_C_RED" "$_C_RESET" "$E2E_FAIL"
  elif [ "$below_floor" -eq 1 ]; then
    printf '%bNO-SHIP (vacuum)%b: only %d PASS < floor %d — nothing substantive ran; this is NOT a green.\n' \
      "$_C_RED" "$_C_RESET" "$E2E_PASS" "$min_pass"
    printf '        Provision creds/tools (Clerk secret + real clients) or lower CORELINK_E2E_MIN_PASS.\n'
  else
    printf '%bSHIP%b: every exercised real client round-tripped against PROD (%d PASS).\n' \
      "$_C_GREEN" "$_C_RESET" "$E2E_PASS"
  fi

  # Build the JSON array body from the recorded rows.
  local body="" row
  for row in "${E2E_JSON_ROWS[@]:-}"; do
    [ -z "$row" ] && continue
    if [ -z "$body" ]; then body="$row"; else body="${body},${row}"; fi
  done

  local out="${OUT_JSON:-${TMPDIR:-/tmp}/e2e-real-client-last.json}"
  {
    printf '{\n'
    printf '  "schema": "corelink.e2e-real-client.v1",\n'
    printf '  "timestamp": "%s",\n' "$ts"
    printf '  "endpoint": "%s",\n' "$(json_escape "${API_HOST:-}")"
    printf '  "oci_endpoint": "%s",\n' "$(json_escape "${OCI_HOST:-}")"
    printf '  "verdict": "%s",\n' "$verdict"
    printf '  "counts": { "pass": %d, "gated": %d, "fail": %d },\n' \
      "$E2E_PASS" "$E2E_GATED" "$E2E_FAIL"
    printf '  "results": [%s]\n' "$body"
    printf '}\n'
  } > "$out"
  info "machine-readable summary → ${out}"
}
