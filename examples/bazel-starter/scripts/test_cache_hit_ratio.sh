#!/usr/bin/env bash
# Self-test for cache_hit_ratio.py, asserted against real Bazel execution logs.
#
# Run: bash examples/bazel-starter/scripts/test_cache_hit_ratio.sh
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RATIO="${HERE}/cache_hit_ratio.py"
DATA="${HERE}/testdata"
failures=0

check() {
    local name="$1" want_rc="$2"; shift 2
    local out rc
    out="$("$@" 2>&1)"; rc=$?
    if [[ "${rc}" -ne "${want_rc}" ]]; then
        echo "FAIL: ${name} — exit ${rc}, wanted ${want_rc}"
        echo "      ${out}"
        failures=$(( failures + 1 ))
        return
    fi
    echo "ok: ${name}"
}

contains() {
    local name="$1" needle="$2"; shift 2
    local out
    out="$("$@" 2>&1)"
    if [[ "${out}" != *"${needle}"* ]]; then
        echo "FAIL: ${name} — output lacks '${needle}'"
        echo "      ${out}"
        failures=$(( failures + 1 ))
        return
    fi
    echo "ok: ${name}"
}

# A remote cache hit is a hit.
check   "remote hit passes"        0 python3 "${RATIO}" "${DATA}/remote-hit.json"
contains "remote hit reads 100%"  "1/1 = 100.0%" python3 "${RATIO}" "${DATA}/remote-hit.json"

# A disk-cache hit is NOT a remote hit — this is the assertion that stops a
# green CI run from being served by the runner's own local cache.
check   "disk hit fails by default"  1 python3 "${RATIO}" "${DATA}/disk-hit.json"
check   "disk hit passes when asked" 0 python3 "${RATIO}" "${DATA}/disk-hit.json" --allow-disk-cache

# A remote-cacheable spawn that executed locally is a miss, not an absent entry.
check    "local exec fails"       1 python3 "${RATIO}" "${DATA}/local-exec.json"
contains "local exec reads 0%"   "0/1 = 0.0%" python3 "${RATIO}" "${DATA}/local-exec.json"

# The threshold is real in both directions.
check "80% floor rejects 0%"  1 python3 "${RATIO}" "${DATA}/local-exec.json" --min 80
check "0% floor accepts 0%"   0 python3 "${RATIO}" "${DATA}/local-exec.json" --min 0

# A missing log is a failure with a message, never a silent zero.
check    "missing log fails"      1 python3 "${RATIO}" "${DATA}/does-not-exist.json"
contains "missing log explains"  "no execution log" python3 "${RATIO}" "${DATA}/does-not-exist.json"

# The regression itself: read line by line, every fixture is unparseable. The
# parser must not be fooled into calling that "no cache entries".
for f in remote-hit disk-hit local-exec; do
    if head -1 "${DATA}/${f}.json" | python3 -c 'import json,sys; json.loads(sys.stdin.read())' 2>/dev/null; then
        echo "FAIL: ${f}.json parses line-by-line — fixture no longer pins the B-017 shape"
        failures=$(( failures + 1 ))
    else
        echo "ok: ${f}.json is pretty-printed, not JSONL (the B-017 shape)"
    fi
done

if [[ "${failures}" -ne 0 ]]; then
    echo "${failures} check(s) failed."
    exit 1
fi
echo "All checks passed."
