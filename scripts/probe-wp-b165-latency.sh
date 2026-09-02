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

raw_owned=1
if [[ -n "${B165_RAW_OUT:-}" ]]; then
  raw="$B165_RAW_OUT"
  raw_owned=0
else
  raw="$(/usr/bin/mktemp /tmp/b165-latency.XXXXXX.tsv)"
fi
trap 'if (( raw_owned )); then /bin/rm -f "$raw"; fi' EXIT
: > "$raw"

sample_numbers=()
for ((i=1; i<=B165_SAMPLES; i++)); do sample_numbers+=("$i"); done

request() {
  local population="$1" surface="$2" path="$3" sample="$4" token="${5:-}"
  local line rc status seconds
  if [[ -n "$token" ]]; then
    if line=$(/usr/bin/curl --connect-timeout 10 --max-time 20 -sS -o /dev/null \
      -H "Authorization: Bearer $token" \
      -w "%{http_code}\t%{time_total}" "$B165_BASE_URL$path" 2>/dev/null); then
      rc=0
    else
      rc=$?
    fi
  else
    if line=$(/usr/bin/curl --connect-timeout 10 --max-time 20 -sS -o /dev/null \
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
  else
    printf '%s\t%s\t%s\t%s\tCURL_FAILED\t%s\t-\n' \
      "$population" "$surface" "$path" "$sample" "$rc" >> "$raw"
  fi
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
for served_path in "$served_a" "$served_b"; do
  case "$served_path" in
    /*) ;;
    *) echo "FATAL: served paths must begin with /" >&2; exit 2 ;;
  esac
done
if [[ "$served_a" == "$served_b" ]]; then
  echo "FATAL: served paths A and B must be distinct tenant paths" >&2
  exit 2
fi
case "$served_a" in *"$tenant_a"*) ;; *) echo "FATAL: served path A does not identify tenant A" >&2; exit 2 ;; esac
case "$served_b" in *"$tenant_b"*) ;; *) echo "FATAL: served path B does not identify tenant B" >&2; exit 2 ;; esac

echo "Served population: authenticated GET; sample 1 is discarded as cold."
for sample in "${sample_numbers[@]}"; do request served served_a "$served_a" "$sample" "$token_a"; done
for sample in "${sample_numbers[@]}"; do request served served_b "$served_b" "$sample" "$token_b"; done
stats served served_a 200 || probe_failure=1
stats served served_b 200 || probe_failure=1
if (( probe_failure )); then
  echo "BLOCKED: served path did not return HTTP 200 for every retained sample." >&2
  exit 3
fi
echo "PASS: both refusal and served populations were measured with the required controls."
