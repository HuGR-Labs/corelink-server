#!/usr/bin/env bash
# B-104 / issue #1660 — read-only authenticated cache-miss probe.
#
# The request population is GET /cargo/<tenant>/<random-64-hex-key>.  The
# response must be an authenticated 404.  The probe records wall time and the
# fixed Server-Timing phases separately because `total` includes the deliberate
# 404 timing pad.  No request body, URL key, response header, PAT, or curl
# diagnostic is printed.
set -euo pipefail
umask 077

PROBE_BASE="${PROBE_BASE:-}"
PROBE_TENANT="${PROBE_TENANT:-ee30f7ba-fc25-4d71-939e-ebe130b4c6a3}"
PROBE_SAMPLES="${PROBE_SAMPLES:-10}"
PROBE_TIMEOUT="${PROBE_TIMEOUT:-15}"

die() {
  echo "B-104 INDETERMINATE: $1" >&2
  exit 2
}

[ -n "${PROBE_TOKEN:-}" ] || die "PROBE_TOKEN is required; no measurement was taken"
[ -n "${PROBE_VERSION:-}" ] || die "PROBE_VERSION is required; deployed version is unknown"
[ -n "${PROBE_BASE}" ] || die "PROBE_BASE is required; choose an approved HTTPS origin"
[[ "${PROBE_VERSION}" =~ ^[A-Za-z0-9._-]{1,128}$ ]] || die "PROBE_VERSION has an invalid shape"
[[ "${PROBE_TENANT}" =~ ^[0-9a-fA-F-]{36}$ ]] || die "PROBE_TENANT must be a UUID"
[[ "${PROBE_SAMPLES}" =~ ^[0-9]+$ ]] || die "PROBE_SAMPLES is not an integer"
[ "${PROBE_SAMPLES}" -ge 10 ] || die "PROBE_SAMPLES must be at least 10"
[[ "${PROBE_TIMEOUT}" =~ ^[0-9]+([.][0-9]+)?$ ]] || die "PROBE_TIMEOUT is not numeric"
awk -v timeout="${PROBE_TIMEOUT}" 'BEGIN { exit !(timeout > 0) }' || die "PROBE_TIMEOUT must be positive"
for command_name in curl openssl awk sort mktemp; do
  command -v "${command_name}" >/dev/null 2>&1 || die "required command is unavailable: ${command_name}"
done
[[ "${PROBE_BASE}" =~ ^https://[A-Za-z0-9._:-]+$ ]] || \
  die "PROBE_BASE must be an https origin without userinfo, query, or fragment"

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/corelink-b104.XXXXXX")" || die "cannot create private temporary directory"
trap 'rm -rf "${TMP_DIR}"' EXIT HUP INT TERM
AUTH_FILE="${TMP_DIR}/authorization.header"
printf 'Authorization: Bearer %s\n' "${PROBE_TOKEN}" >"${AUTH_FILE}"
chmod 600 "${AUTH_FILE}"

REQUEST_NO=0
REQUEST_STATUS=""
REQUEST_TIME=""
REQUEST_HEADER_FILE=""

randkey() {
  local key
  key="$(openssl rand -hex 32 2>/dev/null)" || return 1
  [[ "${key}" =~ ^[0-9a-f]{64}$ ]] || return 1
  printf '%s' "${key}"
}

# Run one request and retain only curl's status/timing line and private headers.
request() {
  local url="$1"
  local with_auth="$2"
  local body_file="${TMP_DIR}/body-${REQUEST_NO}"
  local header_file="${TMP_DIR}/headers-${REQUEST_NO}"
  local error_file="${TMP_DIR}/curl-error-${REQUEST_NO}"
  local result status_field time_field extra_field
  REQUEST_NO=$((REQUEST_NO + 1))
  if [ "${with_auth}" = "yes" ]; then
    if ! result="$(curl --silent --show-error --connect-timeout "${PROBE_TIMEOUT}" \
      --max-time "${PROBE_TIMEOUT}" --output "${body_file}" \
      --dump-header "${header_file}" --header "@${AUTH_FILE}" \
      --write-out '%{http_code}\t%{time_total}' "${url}" 2>"${error_file}")"; then
      return 1
    fi
  else
    if ! result="$(curl --silent --show-error --connect-timeout "${PROBE_TIMEOUT}" \
      --max-time "${PROBE_TIMEOUT}" --output "${body_file}" \
      --dump-header "${header_file}" --write-out '%{http_code}\t%{time_total}' \
      "${url}" 2>"${error_file}")"; then
      return 1
    fi
  fi
  [ -s "${body_file}" ] || return 1
  IFS=$'\t' read -r status_field time_field extra_field <<<"${result}"
  if [ -n "${extra_field}" ] || [[ ! "${status_field}" =~ ^[0-9]{3}$ ]] || \
    [[ ! "${time_field}" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
    return 1
  fi
  awk -v seconds="${time_field}" 'BEGIN { exit !(seconds >= 0) }' || return 1
  REQUEST_STATUS="${status_field}"
  REQUEST_TIME="${time_field}"
  REQUEST_HEADER_FILE="${header_file}"
}

# Parse only the bounded phase vocabulary owned by the edge/container timing
# contract. `oother` is accepted as the rollout alias for `ohandler`; it is not
# emitted if the canonical name is present, so aliases cannot double-count.
parse_timing() {
  local sample="$1"
  local headers="$2"
  awk -v sample="${sample}" '
    BEGIN { IGNORECASE = 1 }
    tolower($0) !~ /^server-timing:/ { next }
    {
      line = $0
      sub(/^[^:]*:[[:space:]]*/, "", line)
      n = split(line, entries, ",")
      for (i = 1; i <= n; i++) {
        entry = entries[i]
        sub(/^[[:space:]]*/, "", entry)
        name = entry
        sub(/[[:space:];].*$/, "", name)
        if (name !~ /^(auth|wdb|qtier|qbatch|qresid|origin|ohop|opat|oquota|ostore|oaccounting|ohandler|oother|total)$/) next
        if (match(entry, /;[[:space:]]*dur=[0-9]+([.][0-9]+)?([eE][+-]?[0-9]+)?/)) {
          raw = substr(entry, RSTART, RLENGTH)
          sub(/^.*dur=[[:space:]]*/, "", raw)
          if (name == "oother") { legacy = raw; legacy_seen = 1 }
          else { print sample "\t" name "\t" raw; if (name == "ohandler") canonical_seen = 1 }
        } else {
          bad = 1
        }
      }
    }
    END {
      if (legacy_seen && !canonical_seen) print sample "\tohandler\t" legacy
      if (bad) exit 3
    }
  ' "${headers}"
}

phase_stats() {
  local phase="$1"
  local values
  values="$(awk -F '\t' -v phase="${phase}" '$2 == phase { print $3 }' "${TIMING_ROWS}" | LC_ALL=C sort -n)"
  [ -n "${values}" ] || return 0
  printf '%s\n' "${values}" | awk -v phase="${phase}" '
    { values[NR] = $1 }
    END {
      n = NR
      if (n % 2 == 0) median = (values[n / 2] + values[n / 2 + 1]) / 2
      else median = values[int(n / 2) + 1]
      rank = 0.90 * n; p90 = int(rank); if (rank > p90) p90++
      printf "phase=%s median_ms=%.0f p90_ms=%.0f n=%d\n", phase, median, values[p90], n
    }
  '
}

printf 'B-104 authenticated 404 probe\n'
# Keep the origin out of the retained log. The workflow artifact is safe to
# share with the issue, while the operator still supplies the exact staging
# origin through the private workflow environment.
printf 'version=%s samples=%s target=staging-origin-redacted/cargo/<tenant>/<random-key>\n' \
  "${PROBE_VERSION}" "${PROBE_SAMPLES}"

if ! request "${PROBE_BASE}/health" no; then
  die "health control failed (transport, timeout, empty, or malformed response)"
fi
[ "${REQUEST_STATUS}" = "200" ] || die "health control returned ${REQUEST_STATUS}, expected 200"
printf 'control health=200\n'

if ! MISS_KEY="$(randkey)"; then die "random-key generator failed"; fi
MISS_PATH="/cargo/${PROBE_TENANT}/${MISS_KEY}"
if ! request "${PROBE_BASE}${MISS_PATH}" no; then
  die "unauthorized control failed (transport, timeout, empty, or malformed response)"
fi
case "${REQUEST_STATUS}" in
  401|403) printf 'control unauthorized=%s\n' "${REQUEST_STATUS}" ;;
  *) die "unauthorized control returned ${REQUEST_STATUS}, expected 401 or 403" ;;
esac

TIMING_ROWS="${TMP_DIR}/timing.tsv"
: >"${TIMING_ROWS}"
sample_no=1
while [ "${sample_no}" -le "${PROBE_SAMPLES}" ]; do
  if ! key="$(randkey)"; then die "random-key generator failed during population"; fi
  if ! request "${PROBE_BASE}/cargo/${PROBE_TENANT}/${key}" yes; then
    die "authenticated sample failed (transport, timeout, empty, or malformed response)"
  fi
  [ "${REQUEST_STATUS}" = "404" ] || die "authenticated sample returned ${REQUEST_STATUS}, expected served 404"
  timing_output="$(parse_timing "${sample_no}" "${REQUEST_HEADER_FILE}")" || \
    die "authenticated sample has malformed Server-Timing"
  [ -n "${timing_output}" ] || die "authenticated sample has no Server-Timing attribution"
  printf '%s\n' "${timing_output}" >>"${TIMING_ROWS}"
  wall_ms="$(awk -v seconds="${REQUEST_TIME}" 'BEGIN { printf "%.0f", seconds * 1000 }')"
  printf '%s\twall\t%s\n' "${sample_no}" "${wall_ms}" >>"${TIMING_ROWS}"
  printf 'sample=%s status=404 wall_ms=%s\n' "${sample_no}" "${wall_ms}"
  sample_no=$((sample_no + 1))
done

for required_phase in auth wdb origin opat ohandler total; do
  count="$(awk -F '\t' -v phase="${required_phase}" '$2 == phase { n++ } END { print n + 0 }' "${TIMING_ROWS}")"
  [ "${count}" -eq "${PROBE_SAMPLES}" ] || \
    die "required Server-Timing phase '${required_phase}' missing from one or more samples"
done

for phase in wall auth wdb qtier qbatch qresid origin ohop opat oquota ostore oaccounting ohandler total; do
  phase_stats "${phase}"
done
printf 'result=MEASURED (all controls, 404 statuses, and attribution phases passed)\n'
