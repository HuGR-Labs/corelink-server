#!/usr/bin/env bash
# Timed, READ-ONLY probe of the live `/cargo/<tenant>/<key>` sccache WebDAV
# surface. Answers ONE question with a number: what does a single sccache cache
# lookup cost in wall-clock, from the box that would be doing the lookup?
#
# WHY THIS EXISTS
# ---------------
# The 2026-08-03 sccache pilot on `corelink-reapi::pr-gate` measured 631 s at a
# 100 % hit rate (827 hits / 0 misses) against a 409-423 s no-cache baseline —
# ~222 s of overhead spread over 827 hits, i.e. ~270 ms of *amortised wall time*
# per hit. That is round-trip territory, not cache-read territory, and it was
# inferred by subtraction rather than measured. This script measures it directly.
#
# WHAT IT DOES *NOT* DO
# ---------------------
# It never writes. Every request is a GET. Keys are freshly-random 64-hex
# strings, so every authenticated request is a MISS and returns 404 without
# touching R2 and without creating anything.
#
# That makes the reported latency a **LOWER BOUND** on a cache HIT: a hit runs
# the identical Worker auth -> container -> PAT-verify -> url-map path and then
# additionally fetches the blob from R2 and streams it back. Read every number
# below as "a hit costs at least this much".
#
# HONEST-FAILURE RULE
# -------------------
# A run that measured nothing must not look like a run that measured something.
# If the token is absent, or the surface answers 401 (bad/expired PAT) instead
# of the expected 404, this script says so and exits non-zero rather than
# printing a fast, meaningless number.
#
# Usage:
#   PROBE_TOKEN=corelink_pat_... ./scripts/probe-cargo-cache-latency.sh [SAMPLES]
#
# Env:
#   PROBE_TOKEN   (required) a PAT with cache read scope on PROBE_TENANT
#   PROBE_TENANT  (default: the internal dogfood tenant used by the pilot)
#   PROBE_BASE    (default: https://corelink-api.humangr.com)
set -euo pipefail

SAMPLES="${1:-30}"
PROBE_BASE="${PROBE_BASE:-https://corelink-api.humangr.com}"
PROBE_TENANT="${PROBE_TENANT:-ee30f7ba-fc25-4d71-939e-ebe130b4c6a3}"
CARGO_BASE="${PROBE_BASE}/cargo/${PROBE_TENANT}"

if [ -z "${PROBE_TOKEN:-}" ]; then
  echo "FATAL: PROBE_TOKEN is empty — this run would measure nothing." >&2
  exit 2
fi

randkey() { openssl rand -hex 32; }

# p50/p90 from a whitespace-separated list of seconds on stdin.
pct() {
  sort -n | awk -v label="$1" '
    { v[NR] = $1; s += $1 }
    END {
      if (NR == 0) { printf "%-34s (no samples)\n", label; exit }
      # Nearest-rank percentiles on the sorted vector (1-indexed).
      p50i = int(NR * 0.50); if (p50i < 1) p50i = 1
      p90i = int(NR * 0.90); if (p90i < 1) p90i = 1
      printf "%-34s n=%-4d min=%6.0f ms  p50=%6.0f ms  p90=%6.0f ms  max=%6.0f ms  mean=%6.0f ms\n",
             label, NR, v[1]*1000, v[p50i]*1000, v[p90i]*1000, v[NR]*1000, (s/NR)*1000
    }'
}

echo "=============================================================="
echo " /cargo cache-lookup latency probe"
echo "   base    : ${PROBE_BASE}"
echo "   tenant  : ${PROBE_TENANT}"
echo "   samples : ${SAMPLES}"
echo "   runner  : ${RUNNER_NAME:-unknown} (${RUNNER_ENVIRONMENT:-unknown})"
echo "   host    : $(uname -sm), $(nproc 2>/dev/null || echo '?') cpu"
echo "=============================================================="
echo

# ---------------------------------------------------------------------------
# Phase 0 — where are we, network-wise?
# `/health` is served by the Worker alone: no container hop, no D1, no R2. It is
# the floor. Anything the /cargo path costs ABOVE this is CoreLink's own
# server-side work, not the box's distance from Cloudflare.
# ---------------------------------------------------------------------------
echo "── phase 0: edge floor (unauthenticated /health, Worker-only) ──"
hdr="$(curl -sS -o /dev/null -D- "${PROBE_BASE}/health" | tr -d '\r')"
ray="$(printf '%s\n' "${hdr}" | awk 'tolower($1) == "cf-ray:" { print $2 }')"
echo "cf-ray: ${ray:-<none>}   (the suffix after '-' is the Cloudflare colo that served us)"
for _ in $(seq 1 5); do
  curl -sS -o /dev/null -w "%{time_connect} %{time_appconnect} %{time_starttransfer}\n" \
    "${PROBE_BASE}/health"
done > /tmp/probe_health.txt
awk '{print $3}' /tmp/probe_health.txt | pct "health TTFB (new conn)"
awk '{print $1}' /tmp/probe_health.txt | pct "  of which TCP connect"
awk '{print $2 - $1}' /tmp/probe_health.txt | pct "  of which TLS handshake"
echo

# ---------------------------------------------------------------------------
# Phase 1 — one authenticated /cargo lookup, cold connection.
# This is the full product path: Worker (PAT verify, scope) -> container DO ->
# PatVerifier (HMAC + D1 + Argon2id) -> MoatCache url-map lookup -> 404.
# ---------------------------------------------------------------------------
echo "── phase 1: authenticated /cargo GET, NEW connection per request ──"
: > /tmp/probe_cold.txt
for _ in $(seq 1 "${SAMPLES}"); do
  curl -sS -o /dev/null \
    -H "Authorization: Bearer ${PROBE_TOKEN}" \
    -w "%{http_code} %{time_connect} %{time_appconnect} %{time_starttransfer}\n" \
    "${CARGO_BASE}/$(randkey)" >> /tmp/probe_cold.txt
done

# Honest-failure rule: assert we actually reached the cache path.
n404="$(awk '$1 == 404' /tmp/probe_cold.txt | wc -l | tr -d ' ')"
n401="$(awk '$1 == 401 || $1 == 403' /tmp/probe_cold.txt | wc -l | tr -d ' ')"
echo "status codes: $(awk '{print $1}' /tmp/probe_cold.txt | sort | uniq -c | tr '\n' ' ')"
if [ "${n401}" -gt 0 ]; then
  echo "FATAL: ${n401} responses were 401/403 — the PAT did not authenticate, so these" >&2
  echo "       timings are an auth rejection, NOT a cache lookup. Measured nothing." >&2
  exit 3
fi
if [ "${n404}" -eq 0 ]; then
  echo "FATAL: expected 404 (authenticated miss on a random key) and got none." >&2
  exit 4
fi
awk '$1 == 404 {print $4}' /tmp/probe_cold.txt | pct "cargo TTFB (new conn)"
awk '$1 == 404 {print $4 - $3}' /tmp/probe_cold.txt | pct "  server time (TTFB - TLS done)"
echo

# ---------------------------------------------------------------------------
# Phase 2 — the number that actually maps onto sccache.
# opendal (sccache's WebDAV backend) drives a pooled reqwest client, so after
# the first lookup the TCP+TLS cost is amortised away and every subsequent
# lookup pays only server time on a warm connection. Passing many URLs to ONE
# curl invocation reproduces exactly that: one connection, N sequential
# requests. THIS is the per-hit cost to compare against the ~270 ms inferred
# from the pilot.
# ---------------------------------------------------------------------------
echo "── phase 2: authenticated /cargo GET, ONE REUSED connection (sccache-shaped) ──"
urls=()
for _ in $(seq 1 "${SAMPLES}"); do urls+=("${CARGO_BASE}/$(randkey)"); done
curl -sS -o /dev/null \
  -H "Authorization: Bearer ${PROBE_TOKEN}" \
  -w "%{http_code} %{time_connect} %{time_starttransfer}\n" \
  "${urls[@]}" > /tmp/probe_warm.txt
# The first request in the batch pays the handshake; drop it.
awk 'NR > 1 && $1 == 404 {print $3}' /tmp/probe_warm.txt | pct "cargo TTFB (reused conn)"
echo

# ---------------------------------------------------------------------------
# Phase 3 — does the surface parallelise?
# cargo runs -j N rustc processes, each of which is one sccache client, so
# lookups are concurrent up to N. If throughput scales with concurrency, the
# per-hit RTT is amortised across the job and the ~270 ms of *amortised* pilot
# overhead implies a much larger true RTT. If throughput is FLAT, the surface
# serialises and 270 ms is close to the real RTT. Measured, not assumed.
# ---------------------------------------------------------------------------
#
# ⚠️ Throughput ALONE is not a scaling measurement, and reading it as one is the
# mistake this block is written to prevent. `PatVerifier` sheds load: a request
# that cannot get an Argon2id permit within ARGON2_PERMIT_WAIT (250 ms) is
# REJECTED, not queued. A rejected request is fast, so a server that is refusing
# most of its traffic posts a HIGHER req/s than one serving all of it. We
# therefore count 404s (served) separately from everything else (shed), and
# report a SERVED throughput. Only the served column means anything.
echo "── phase 3: concurrency sweep (throughput vs parallelism) ──"
printf '%-5s %-6s %-10s %-8s %-8s %-11s %s\n' \
  "P" "reqs" "wall (s)" "404 ok" "other" "served/s" "eff. ms/req"
for P in 1 2 4 8 16; do
  M=$((P * 4))
  : > /tmp/probe_conc.txt
  keys=()
  for _ in $(seq 1 "${M}"); do keys+=("${CARGO_BASE}/$(randkey)"); done
  start="$(date +%s.%N)"
  printf '%s\n' "${keys[@]}" | xargs -P "${P}" -I{} \
    curl -sS -o /dev/null -w "%{http_code}\n" \
      -H "Authorization: Bearer ${PROBE_TOKEN}" {} >> /tmp/probe_conc.txt 2>/dev/null || true
  end="$(date +%s.%N)"
  ok="$(awk '$1 == 404' /tmp/probe_conc.txt | wc -l | tr -d ' ')"
  other=$((M - ok))
  awk -v p="${P}" -v m="${M}" -v s="${start}" -v e="${end}" -v ok="${ok}" -v ot="${other}" \
    'BEGIN { w = e - s
             printf "%-5s %-6s %-10.2f %-8s %-8s %-11.2f %.0f\n",
                    p, m, w, ok, ot, ok/w, (w/m)*1000 }'
  if [ "${other}" -gt 0 ]; then
    echo "      shed/errored status codes: $(awk '$1 != 404 {print $1}' /tmp/probe_conc.txt | sort | uniq -c | tr '\n' ' ')"
  fi
done
echo

echo "=============================================================="
echo "Reminder when reading these numbers:"
echo "  * every request above was a 404 MISS — a real cache HIT costs strictly"
echo "    MORE (it adds the R2 blob fetch + body transfer)."
echo "  * phase-2 'reused conn' is the figure to compare against the ~270 ms"
echo "    per-hit overhead inferred from the 631 s / 827-hit pilot run."
echo "=============================================================="
