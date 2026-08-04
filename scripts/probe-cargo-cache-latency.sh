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
# Phase 2b — WHERE that server time goes, read off the wire.
#
# Phase 2 says how much server time a warm lookup costs. It does not say which
# tier spends it, and after the Argon2id memo shipped (#1022) that became the
# open question: ~300 ms still sits on a request that only authenticates and
# 404s, and it is NOT Argon2id (the phase-3 throughput curve now scales with
# parallelism, which a CPU-bound stage cannot do).
#
# The Worker already publishes the split and nobody was reading it. Every authed
# data-plane response carries `Server-Timing` (worker/src/index.ts):
#   auth   — the Worker's PAT verify, with `desc` naming the cache tier that
#            served the row (l1 / kv / d1)
#   wdb    — the Worker-side quota + residency reads to the ENAM primary, broken
#            into the awaits it is made of:
#              qtier  — tier resolve  (L1 isolate -> KV -> D1)
#              qbatch — ONE D1 round trip carrying BOTH uncached quota
#                       statements: the monthly request-counter UPSERT and the
#                       storage SUM(bytes_used) read, issued as a `db.batch`
#              qresid — residency resolve (L1 isolate -> KV -> D1)
#            `qbatch` is the phase that used to be `qmeter` + `qstor`, two SERIAL
#            round trips. This probe is what showed they were the entire `wdb`
#            (2026-08-04, warm, n=30: qmeter 152/158/163 + qstor 120/126/130 =
#            wdb 277/284/302, with qtier and qresid at 0), which is why they were
#            merged. A Worker deployed BEFORE that merge still emits the old two
#            names; both sets are queried below so the probe reads either.
#            The cache-backed phases can still cost a full D1 round trip on an
#            isolate-cold request — which is exactly why these are measured
#            rather than assumed.
#   origin — the DO + container subrequest, broken into the phases it is made of.
#            It measured 281/300/329 ms of a 443/457/547 ms total on 2026-08-04
#            — 66 % of the request in ONE opaque block — which is why it was
#            split the same way `wdb` was:
#              ohop   — the DO hop: Worker->DO dispatch + placement, the DO's own
#                       prologue (lifecycle bind, ensureContainerRunning, the
#                       getAlarm re-arm), the DO->container wire, and the
#                       response travelling back. DERIVED by the Worker as
#                       `origin - Σ(the container's phases)`, because only the
#                       Worker can see both ends of the hop.
#              opat   — the container's OWN per-request D1 `pat` row read (#1022
#                       deliberately kept it so a revocation takes effect at once)
#              oquota — the per-tenant monthly $-ceiling check/accrue (ADR-0068),
#                       a D1 round trip on every billable request
#              ostore — the moat storage lookup: the url->content-hash map read
#                       plus, on a hit, the CAS/R2 blob fetch
#              oother — the container's own residue: routing, the rate-limit
#                       layer, HMAC, Argon2id (or its memo hit), response
#                       assembly. Computed by the CONTAINER against its own
#                       whole-request clock.
#            The container reports opat/oquota/ostore/oother on the subresponse's
#            own `Server-Timing` and the Worker merges them; the five ALWAYS sum
#            to `origin` exactly. A Worker deployed BEFORE the container image is
#            repinned emits `origin` alone (the container says nothing to merge),
#            so the o* rows read ABSENT — that is "prod is behind this branch",
#            not "the hop was free". A container report the Worker cannot
#            reconcile is refused WHOLE and shows up as a single
#            `ohop;dur=<origin>;desc="unreconciled"` — never as a split that does
#            not add up.
#   total  — the Worker's own view of the request
#
# ⚠️ These durations are coarse by construction: `Date.now()` in a Worker only
# advances across I/O, so a purely-CPU stretch can measure 0. Read them as
# directional attribution, never as a profile.
# ---------------------------------------------------------------------------
echo "── phase 2b: where that server time goes (Worker Server-Timing split) ──"
urls_st=()
for _ in $(seq 1 "${SAMPLES}"); do urls_st+=("${CARGO_BASE}/$(randkey)"); done
curl -sS -o /dev/null -D /tmp/probe_hdrs.txt \
  -H "Authorization: Bearer ${PROBE_TOKEN}" \
  "${urls_st[@]}" || true
tr -d '\r' < /tmp/probe_hdrs.txt | grep -i '^server-timing:' > /tmp/probe_st.txt || true

if [ ! -s /tmp/probe_st.txt ]; then
  echo "  NO Server-Timing header on any response."
  echo "  That is itself the finding: the /cargo adapter route does not reach the"
  echo "  Worker's timing emission, so this surface ships with no latency"
  echo "  attribution at all — for us OR for a customer debugging a slow cache."
else
  # `n=` in each row is load-bearing. The Worker emits a phase that RAN even at
  # `dur=0` and omits only a phase that did NOT run, so `n == SAMPLES` with
  # `p50=0` means "ran every time, cheaper than the clock can resolve" and a low
  # `n` means "skipped on that many requests". Never read a missing row as a fast
  # row.
  #
  # ⚠️ That contract holds only against a Worker at or past the commit that
  # introduced it. An OLDER deployed Worker suppressed `auth`/`wdb`/`origin` at
  # 0 ms on a strict `>`, which is why the 2026-08-04 probe run showed `auth n=3`
  # out of 30 responses: those 27 were KV-served in under a millisecond, NOT
  # skipped. If you see a low `n` on `auth`/`wdb`/`origin`, check what is actually
  # deployed before concluding anything — that number is the reason this contract
  # was made explicit.
  # `qmeter`/`qstor` are the PRE-merge names of `qbatch` — kept in the list so a
  # probe run against an older deployed Worker still attributes its `wdb`
  # instead of silently reporting an unexplained aggregate. The `origin`
  # sub-phases (`ohop` … `oother`) are queried on the SAME terms: they exist only
  # on a Worker+container at or past the commit that introduced them, and asking
  # for them costs nothing on an older deployment beyond an honest ABSENT row.
  # THAT is why the last transition was measurable — the probe knew both
  # vocabularies across the deploy, so a run before the deploy still attributed.
  for ph in auth wdb qtier qbatch qresid qmeter qstor origin ohop opat oquota ostore oother total; do
    # `dur` is milliseconds; `pct` takes seconds.
    #
    # An ABSENT phase must not kill the probe. Under `set -euo pipefail` a `grep`
    # that matches nothing exits 1 and takes the whole script down, so asking
    # about a phase the DEPLOYED Worker does not emit yet would abort the run
    # before `origin` and `total` ever print — the probe would report LESS the
    # moment we taught it to look for more. `|| true` keeps the run alive and the
    # empty case is reported EXPLICITLY: a phase nobody emitted is a fact worth
    # seeing (it means prod is behind this branch, or the phase was skipped on
    # every request), and it must never be silently mistaken for a fast phase.
    vals="$(grep -oE "(^|[ ,])${ph};dur=[0-9]+" /tmp/probe_st.txt \
      | grep -oE '[0-9]+$' \
      | awk '{ print $1 / 1000 }' || true)"
    if [ -z "${vals}" ]; then
      printf '  %-32s ABSENT — not emitted on ANY of the %s responses (the deployed Worker does not publish this phase, or it was skipped on every request — NOT "it was fast")\n' \
        "${ph}" "${SAMPLES}"
      continue
    fi
    printf '%s\n' "${vals}" | pct "  ${ph}"
  done
  echo -n "  auth served from  : "
  grep -oE 'desc="[a-z0-9]+"' /tmp/probe_st.txt | sort | uniq -c | tr '\n' ' '
  echo

  # --- does the origin split RECONCILE on the wire? -------------------------
  # A split that does not add up is worse than no split: it invites a confident
  # wrong conclusion about which tier to attack. The Worker guarantees the
  # identity by construction, so a mismatch here means the header was rewritten
  # in transit (a proxy, a CDN feature) or the deployed Worker is not the one
  # this script documents. Report it per-response, loudly, rather than averaging
  # over it — an average hides exactly the responses worth looking at.
  echo -n "  origin split      : "
  awk '
    { origin = -1; sum = 0; parts = 0; unrec = 0
      if ($0 ~ /desc="unreconciled"/) unrec = 1
      n = split($0, e, ",")
      for (i = 1; i <= n; i++) {
        if (match(e[i], /origin;dur=[0-9]+/))                { origin = substr(e[i], RSTART + 11, RLENGTH - 11) + 0 }
        else if (match(e[i], /(ohop|opat|oquota|ostore|oother);dur=[0-9]+/)) {
          f = substr(e[i], RSTART, RLENGTH); split(f, kv, ";dur="); sum += kv[2] + 0; parts++
        }
      }
      if (origin < 0) next
      if (unrec) { u++; next }
      if (parts == 0) { absent++; next }
      total++
      if (sum == origin) ok++; else { bad++; badmsg = badmsg sprintf("\n      MISMATCH: origin=%dms but the sub-phases sum to %dms", origin, sum) }
    }
    END {
      if (u > 0)      printf "%d response(s) carried desc=\"unreconciled\" — the container reported a split the Worker refused. ", u
      if (absent > 0) printf "%d response(s) had NO sub-phases (the deployed container predates them — NOT a free hop). ", absent
      if (total == 0) { printf "no decomposed response to check.\n"; exit }
      printf "%d/%d reconcile exactly (ohop + opat + oquota + ostore + oother == origin).", ok, total
      if (bad > 0) printf "%s\n      ^ the accounting is NOT trustworthy; do not act on the split above.", badmsg
      printf "\n"
    }' /tmp/probe_st.txt

  echo "  (ohop is the DO hop — dispatch + placement + the DO's prologue + the wire;"
  echo "   opat/oquota are D1 round trips the CONTAINER makes on every request;"
  echo "   ostore is the storage lookup; oother is everything else in-container."
  echo "   A large wdb instead means it is the Worker's own uncached D1 reads.)"
fi
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

# ---------------------------------------------------------------------------
# Phase 4 — what a shed response actually SAYS.
# Phase 3 shows requests being refused above the per-tenant Argon2id sub-cap,
# with a status code that is NOT what an overload should return. This phase keeps
# the body so the refusal can be characterised rather than guessed at.
#
# Measured answer (2026-08-04): `401 authentication failed (ref: <uuid>)`. The
# body is deliberately opaque -- it does NOT distinguish "your PAT is invalid"
# from "the verifier was overloaded", which is correct for the response and
# exactly why the STATUS CODE has to carry that distinction and does not. The
# `ref` is the handle for correlating one refusal with its container log line.
# ---------------------------------------------------------------------------
echo "── phase 4: the body of a refused request (8-wide, above the sub-cap) ──"
rm -f /tmp/probe_body_*.txt /tmp/probe_body_codes.txt
: > /tmp/probe_body_codes.txt
for i in $(seq 1 8); do
  curl -sS -o "/tmp/probe_body_${i}.txt" \
    -H "Authorization: Bearer ${PROBE_TOKEN}" \
    -w "${i} %{http_code}\n" \
    "${CARGO_BASE}/$(randkey)" >> /tmp/probe_body_codes.txt 2>/dev/null &
done
wait
sort -k1 -n /tmp/probe_body_codes.txt | while read -r idx code; do
  body="$(tr -d '\r\n' < "/tmp/probe_body_${idx}.txt" 2>/dev/null | cut -c1-300)"
  printf '  %s  %s\n' "${code}" "${body:-<empty body>}"
done
echo

echo "=============================================================="
echo "Reminder when reading these numbers:"
echo "  * every request above was a 404 MISS — a real cache HIT costs strictly"
echo "    MORE (it adds the R2 blob fetch + body transfer)."
echo "  * phase-2 'reused conn' is the figure to compare against the ~270 ms"
echo "    per-hit overhead inferred from the 631 s / 827-hit pilot run."
echo "=============================================================="
