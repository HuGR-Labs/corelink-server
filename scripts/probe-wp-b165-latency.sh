#!/usr/bin/env bash
# Read-only B-165 latency probe.
#
# Default mode is `full`: it measures the unauthenticated refusal paths and then
# requires two distinct customer PATs plus one known-served path per tenant
# before measuring authenticated served paths. `B165_MODE=refusal` is provided
# only to capture the refusal half while the served-path prerequisite is blocked;
# it exits 2 so a partial run cannot be mistaken for a complete proof.
set -euo pipefail

B165_BASE_URL="${B165_BASE_URL:-https://corelink-api.humangr.com}"
B165_MODE="${B165_MODE:-full}"
B165_SAMPLES="${B165_SAMPLES:-10}"

case "$B165_SAMPLES" in
  ''|*[!0-9]*) echo "FATAL: B165_SAMPLES must be an integer >= 3" >&2; exit 2 ;;
esac
if (( B165_SAMPLES < 3 )); then
  echo "FATAL: B165_SAMPLES must be an integer >= 3" >&2
  exit 2
fi
if [[ "$B165_MODE" != full && "$B165_MODE" != refusal ]]; then
  echo "FATAL: B165_MODE must be full or refusal" >&2
  exit 2
fi

case "$B165_BASE_URL" in
  */) B165_BASE_URL="${B165_BASE_URL%/}" ;;
esac

cleanup_paths=()
if [[ -n "${B165_RAW_OUT:-}" ]]; then
  raw="$B165_RAW_OUT"
else
  raw="$(/usr/bin/mktemp /tmp/b165-latency.XXXXXX.tsv)"
  cleanup_paths+=("$raw")
fi
if [[ -n "${B165_TIMING_OUT:-}" ]]; then
  timing_raw="$B165_TIMING_OUT"
else
  timing_raw="$(/usr/bin/mktemp /tmp/b165-server-timing.XXXXXX.tsv)"
  cleanup_paths+=("$timing_raw")
fi
cleanup() {
  if (( ${#cleanup_paths[@]} > 0 )); then
    /bin/rm -f -- "${cleanup_paths[@]}"
  fi
}
trap cleanup EXIT
: > "$raw"
: > "$timing_raw"

sample_numbers=()
for ((i=1; i<=B165_SAMPLES; i++)); do sample_numbers+=("$i"); done

request() {
  local population="$1" surface="$2" path="$3" sample="$4" token="${5:-}"
  local line rc status seconds headers server_total_ms transport_ms
  headers="$(/usr/bin/mktemp /tmp/b165-headers.XXXXXX)"
  if [[ -n "$token" ]]; then
    if line=$(/usr/bin/curl --connect-timeout 10 --max-time 20 -sS -o /dev/null -D "$headers" \
      -H "Authorization: Bearer $token" \
      -w "%{http_code}\t%{time_total}" "$B165_BASE_URL$path" 2>/dev/null); then
      rc=0
    else
      rc=$?
    fi
  else
    if line=$(/usr/bin/curl --connect-timeout 10 --max-time 20 -sS -o /dev/null -D "$headers" \
      -w "%{http_code}\t%{time_total}" "$B165_BASE_URL$path" 2>/dev/null); then
      rc=0
    else
      rc=$?
    fi
  fi
  # A failed command in the assignment is intentionally converted to a row;
  # the final verdict below remains non-zero and names the network failure.
  if [[ "$line" =~ ^([0-9]{3})$'\t'([0-9]+\.?[0-9]*)$ ]]; then
    status="${BASH_REMATCH[1]}"
    seconds="${BASH_REMATCH[2]}"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$population" "$surface" "$path" "$sample" "$status" "$rc" "$seconds" >> "$raw"
    # `Server-Timing: ... total;dur=N` is generated at the edge and excludes
    # client↔colo transport. Keep it separate from curl's end-to-end clock;
    # never infer service latency by comparing raw RTT against an internal SLO.
    server_total_ms="$(/usr/bin/tr -d '\r' < "$headers" | /usr/bin/awk 'BEGIN{IGNORECASE=1}
      /^server-timing:/ {
        sub(/^[^:]+:[[:space:]]*/, "")
        n=split($0, phases, ",")
        for (i=1; i<=n; i++) {
          gsub(/^[[:space:]]+|[[:space:]]+$/, "", phases[i])
          if (phases[i] ~ /^total;dur=[0-9]+([.][0-9]+)?$/) {
            sub(/^total;dur=/, "", phases[i]); print phases[i]; exit
          }
        }
      }')"
    if [[ "$population" == served && -n "$server_total_ms" ]]; then
      transport_ms="$(/usr/bin/awk -v curl_s="$seconds" -v server_ms="$server_total_ms" \
        'BEGIN { printf "%.3f", curl_s * 1000 - server_ms }')"
      printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$surface" "$path" "$sample" "$seconds" "$server_total_ms" "$transport_ms" >> "$timing_raw"
    fi
  else
    printf '%s\t%s\t%s\t%s\tCURL_FAILED\t%s\t-\n' \
      "$population" "$surface" "$path" "$sample" "$rc" >> "$raw"
  fi
  /bin/rm -f "$headers"
}

stats() {
  local population="$1" surface="$2" expected="$3" values n p50 p90
  values="$(/usr/bin/mktemp /tmp/b165-values.XXXXXX)"
  /usr/bin/awk -F '\t' -v pop="$population" -v surf="$surface" -v expected="$expected" \
    '$1 == pop && $2 == surf && $4 > 1 && $5 == expected && $6 == 0 { print $7 }' \
    "$raw" | /usr/bin/sort -n > "$values"
  n=$(/usr/bin/wc -l < "$values" | /usr/bin/tr -d ' ')
  if (( n != B165_SAMPLES - 1 )); then
    echo "FATAL: $population/$surface retained $n of $((B165_SAMPLES - 1)) samples after discarding sample 1" >&2
    /bin/rm -f "$values"
    return 1
  fi
  p50=$(( (n + 1) / 2 ))
  p90=$(( (9 * n + 9) / 10 ))
  /usr/bin/awk -v pop="$population" -v surf="$surface" -v n="$n" -v p50="$p50" -v p90="$p90" \
    '{ v[NR] = $1; sum += $1 }
     END { printf "%s/%s n=%d p50_ms=%.1f p90_ms=%.1f min_ms=%.1f max_ms=%.1f\n",
       pop, surf, n, v[p50] * 1000, v[p90] * 1000, v[1] * 1000, v[n] * 1000 }' "$values"
  /bin/rm -f "$values"
}

echo "B-165 read-only latency probe"
echo "base=$B165_BASE_URL samples=$B165_SAMPLES mode=$B165_MODE"
echo "raw=$raw"
echo "server_timing_raw=$timing_raw"
echo "Refusal population: unauthenticated GET; sample 1 is discarded as cold."

declare -a refusal_paths=(
  "/v1/cas/x/y"
  "/npm/x"
  "/pip/simple/x"
  "/v2/x/manifests/latest"
)
for path in "${refusal_paths[@]}"; do
  surface="${path#/}"
  surface="${surface%%/*}"
  for sample in "${sample_numbers[@]}"; do
    request refusal "$surface" "$path" "$sample"
  done
done

echo "Control population: unauthenticated GET /health; sample 1 is discarded as cold."
for sample in "${sample_numbers[@]}"; do request control health /health "$sample"; done

probe_failure=0
for path in "${refusal_paths[@]}"; do
  surface="${path#/}"
  surface="${surface%%/*}"
  stats refusal "$surface" 401 || probe_failure=1
done
stats control health 200 || probe_failure=1

if (( probe_failure )); then
  echo "BLOCKED: refusal/control population had a network failure, wrong status, or truncated samples." >&2
  exit 3
fi

tenant_a="${B165_TENANT_A:-}"
tenant_b="${B165_TENANT_B:-}"
token_a="${B165_PAT_A:-}"
token_b="${B165_PAT_B:-}"
served_a="${B165_SERVED_PATH_A:-}"
served_b="${B165_SERVED_PATH_B:-}"
if [[ "$B165_MODE" == refusal || -z "$tenant_a" || -z "$tenant_b" || -z "$token_a" || -z "$token_b" || -z "$served_a" || -z "$served_b" ]]; then
  echo "BLOCKED: served-path evidence not measured; requires two distinct customer tenants, two customer PATs, and one known-served path per tenant." >&2
  echo "The refusal-only result above is not a complete B-165 proof and must not close the item." >&2
  exit 2
fi
if [[ "$tenant_a" == "$tenant_b" ]]; then
  echo "FATAL: B165_TENANT_A and B165_TENANT_B must be distinct" >&2
  exit 2
fi
if [[ "$token_a" == "$token_b" ]]; then
  echo "FATAL: B165_PAT_A and B165_PAT_B must be distinct customer credentials" >&2
  exit 2
fi

# Keep the tenant binding structural, not a substring check.  Only the two
# canonical object routes are accepted, and the tenant is one exact path
# segment.  This mirrors verify_b165_latency.py and prevents e.g.
# `tenant-a` from being "proved" by `prefix-tenant-a`.
canonical_tenant() {
  local path="$1" rest tenant object
  case "$path" in
    /v1/cas/*/*)
      rest="${path#/v1/cas/}"
      tenant="${rest%%/*}"
      object="${rest#*/}"
      ;;
    /cargo/*/*)
      rest="${path#/cargo/}"
      tenant="${rest%%/*}"
      object="${rest#*/}"
      ;;
    *)
      return 1
      ;;
  esac
  [[ -n "$tenant" && -n "$object" && "$object" != */* && "$path" != *'//'* ]] || return 1
  printf '%s\n' "$tenant"
}

served_tenant_a="$(canonical_tenant "$served_a")" || {
  echo "FATAL: served path A is not a canonical /v1/cas/<tenant>/<object> or /cargo/<tenant>/<object> path" >&2
  exit 2
}
served_tenant_b="$(canonical_tenant "$served_b")" || {
  echo "FATAL: served path B is not a canonical /v1/cas/<tenant>/<object> or /cargo/<tenant>/<object> path" >&2
  exit 2
}
if [[ "$served_a" == "$served_b" ]]; then
  echo "FATAL: served paths A and B must be distinct tenant paths" >&2
  exit 2
fi
if [[ "$served_tenant_a" != "$tenant_a" ]]; then
  echo "FATAL: served path A does not identify tenant A in its canonical segment" >&2
  exit 2
fi
if [[ "$served_tenant_b" != "$tenant_b" ]]; then
  echo "FATAL: served path B does not identify tenant B in its canonical segment" >&2
  exit 2
fi

echo "Served population: authenticated GET; sample 1 is discarded as cold."
for sample in "${sample_numbers[@]}"; do request served served_a "$served_a" "$sample" "$token_a"; done
for sample in "${sample_numbers[@]}"; do request served served_b "$served_b" "$sample" "$token_b"; done
stats served served_a 200 || probe_failure=1
stats served served_b 200 || probe_failure=1
for surface in served_a served_b; do
  timing_count="$(/usr/bin/awk -F '\t' -v surf="$surface" '$1 == surf { count++ } END { print count + 0 }' "$timing_raw")"
  if (( timing_count != B165_SAMPLES )); then
    echo "BLOCKED: $surface retained $timing_count of $B165_SAMPLES required Server-Timing totals." >&2
    probe_failure=1
  fi
done
if (( probe_failure )); then
  echo "BLOCKED: served path status or Server-Timing population is incomplete." >&2
  exit 3
fi
echo "PASS: both refusal and served populations were measured with the required controls."
