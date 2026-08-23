#!/usr/bin/env bash
# Prove detect_unreachable.py still answers the cases it was built for.
#
# The detector's whole value is the direction of its errors: a missed dead
# binding costs nothing, a FALSE "this is dead" invites someone to delete live
# code. The prose in the module docstring claims that property; this script is
# what makes the claim re-checkable tomorrow. Each cell builds a synthetic
# wrangler.toml + worker/src and asserts the verdict, except the last two,
# which assert facts about the REAL tree.
#
# Run: bash scripts/test_detect_unreachable.sh
set -uo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
DETECT="$HERE/detect_unreachable.py"
SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"' EXIT

fails=0
pass() { echo "  PASS  $1"; }
fail() { echo "  FAIL  $1"; fails=$((fails + 1)); }

# $1 case name, $2 binding name, $3 expected status, $4 worker/src body
cell() {
  local name="$1" binding="$2" want="$3" body="$4" root out got
  root="$SANDBOX/$(echo "$name" | tr -c 'a-zA-Z0-9' '_')"
  mkdir -p "$root/worker/src"
  cat > "$root/wrangler.toml" <<TOML
name = "probe"

[[durable_objects.bindings]]
name = "$binding"
class_name = "Probe"
TOML
  printf '%s\n' "$body" > "$root/worker/src/index.ts"
  out="$(python3 "$DETECT" --repo-root "$root" --json 2>&1)" || { fail "$name (detector crashed)"; return; }
  got="$(python3 -c '
import json, sys
d = json.load(sys.stdin)
for f in d["findings"]:
    if f["name"] == sys.argv[1] and f["category"] == "durable_object_binding":
        print(f["status"]); break
else:
    print("ABSENT")
' "$binding" <<<"$out")"
  if [[ "$got" != "$want" ]]; then
    fail "$name (got $got, want $want)"; return
  fi
  pass "$name"
}

echo "detect_unreachable.py — regression cells"

# 1-4: the four access shapes that must all read as REACHED.
cell "direct dot access"   PROBE_DO REACHED 'const x = env.PROBE_DO;'
cell "bracket access"      PROBE_DO REACHED 'const x = env["PROBE_DO"];'
cell "destructured"        PROBE_DO REACHED 'const { PROBE_DO } = env;'
# The case a naive grep got wrong on EDGE_DO_METER, and the reason this file exists.
cell "type-cast access"    PROBE_DO REACHED 'const x = (env as unknown as { PROBE_DO?: string }).PROBE_DO;'

# 5: genuinely unreferenced -> UNREACHED.
cell "no reference at all"  PROBE_DO UNREACHED 'export default { fetch() { return new Response("hi"); } };'

# 6: named ONLY in a comment is NOT a reaching reference.
cell "named only in prose"  PROBE_DO UNREACHED '// PROBE_DO is bound but nothing calls it.
export default { fetch() { return new Response("hi"); } };'

echo "detect_unreachable.py — assertions about the real tree"

# 7: the accepted string-literal limitation is only acceptable while its
# blast radius is zero. This is the check that tells us when that changes.
url_and_env="$(grep -rn --include='*.ts' '://' "$REPO/worker/src" 2>/dev/null | grep -cE '\benv\.[A-Z]')"
if [[ "$url_and_env" == "0" ]]; then
  pass "no worker/src line mixes a URL string with an env reference"
else
  fail "$url_and_env line(s) mix a URL string with an env reference — the comment-stripper can now destroy a real reference and emit a false UNREACHED; teach it about string literals"
fi

# 8: EDGE_DO_METER is live in prod and read through a cast. If this ever reads
# UNREACHED, the detector has regressed into the bug it was built to prevent.
real="$(python3 "$DETECT" --repo-root "$REPO" --json 2>/dev/null | python3 -c '
import json, sys
d = json.load(sys.stdin)
print(next((f["status"] for f in d["findings"]
            if f["name"] == "EDGE_DO_METER" and f["category"] == "wrangler_var"), "ABSENT"))
')"
if [[ "$real" == "REACHED" ]]; then
  pass "EDGE_DO_METER reads REACHED against the real tree"
else
  fail "EDGE_DO_METER reads $real against the real tree (want REACHED)"
fi

echo
if [[ "$fails" -eq 0 ]]; then
  echo "test_detect_unreachable: OK"
  exit 0
fi
echo "test_detect_unreachable: $fails failure(s)"
exit 1
