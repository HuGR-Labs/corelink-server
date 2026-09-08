#!/usr/bin/env bash
# Executable regression for the custom raw-hex gitleaks rule.
#
# This invokes the pinned scanner supplied by the workflow.  It deliberately
# exercises every path class in the rule, legacy and opaque names, boundary
# values, and a rule-removal mutation.  A missing scanner/config or malformed
# mutation is an error; it must never degrade to a green skip.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
config="${repo_root}/.gitleaks.toml"
tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/corelink-gitleaks-shape.XXXXXX")"
trap 'rm -rf -- "$tmpdir"' EXIT

command -v gitleaks >/dev/null || { echo "gitleaks is required" >&2; exit 2; }
[[ -f "$config" ]] || { echo "missing .gitleaks.toml" >&2; exit 2; }

probe="6b73d8c0f5a19e2b4d6087c3a9f1b5e7c2d4a6f8091b3d5e7f9a0c2e4b6d8f01"

run_scan() {
  local source="$1" config_path="$2" expected="$3" label="$4" output status
  output="${tmpdir}/${label}.out"
  set +e
  gitleaks detect --no-git --source "$source" --config "$config_path" \
    --redact --no-banner --exit-code 1 >"$output" 2>&1
  status=$?
  set -e
  if [[ "$status" != "$expected" ]]; then
    echo "${label}: expected scanner exit ${expected}, got ${status}" >&2
    sed -n '1,80p' "$output" >&2
    exit 1
  fi
}

positive_root="${tmpdir}/positive"
mkdir -p "$positive_root/.github/workflows" "$positive_root/reports/audits" \
  "$positive_root/tests" "$positive_root/public"
printf 'opaque_material = "%s"\n' "$probe" >"${positive_root}/wrangler.toml"
printf '{"opaque material":"%s"}\n' "$probe" >"${positive_root}/wrangler.json"
printf '{"opaque material":"%s"}\n' "$probe" >"${positive_root}/wrangler.jsonc"
printf 'opaque_material="%s"\n' "$probe" >"${positive_root}/.env.production"
printf 'opaque_material = "%s"\n' "$probe" >"${positive_root}/deploy-secrets.conf"
printf 'opaque_material = "%s"\n' "$probe" >"${positive_root}/deploy-credentials.toml"
printf 'opaque_material: %s\n' "$probe" >"${positive_root}/deploy-secrets.yaml"
printf 'name: fixture\non:\n  workflow_dispatch:\njobs:\n  scan:\n    runs-on: ubuntu-latest\n    env:\n      opaque_material: %s\n' \
  "$probe" >"${positive_root}/.github/workflows/secrets-fixture.yml"
printf 'opaque_material = "%s"\n' "$probe" >"${positive_root}/reports/audits/incident.md"
printf 'CORELINK_INTERNAL_AUTH_KEY = "%s"\nPAT_SIGNING_KEY_NEXT = "%s"\n' \
  "$probe" "$probe" >"${positive_root}/legacy.toml"

negative_root="${tmpdir}/negative"
mkdir -p "$negative_root/reports" "$negative_root/tests"
printf 'sha256: %s\nchecksum: %s\ndigest: %s\n' "$probe" "$probe" "$probe" \
  >"${negative_root}/reports/public-digests.md"
printf 'opaque_material = "%s"\n' "$probe" >"${negative_root}/tests/fixture.toml"
short32="${probe:0:32}"
short63="${probe:0:63}"
long65="${probe}a"
hyphenated="${probe:0:32}-${probe:32}"
printf 'short32 = "%s"\nshort63 = "%s"\nlong65 = "%s"\nprefixed = "0x%s"\nhyphenated = "%s"\n' \
  "$short32" "$short63" "$long65" "$probe" "$hyphenated" \
  >"${negative_root}/wrangler.toml"

run_scan "$positive_root" "$config" 1 "all-deployable-path-classes"
run_scan "$negative_root/reports" "$config" 0 "public-audit-outside-rule"
run_scan "$negative_root/tests" "$config" 0 "versioned-fixture-allowlist"
# Boundary values are not the continuous bare 64-hex assignment shape.
run_scan "$negative_root" "$config" 0 "shape-boundaries"

# The custom rule is the only detector expected to catch this opaque value.
# Removing it must make the positive fixture green, proving the regression has
# a real tooth and is not merely exercising a default rule.
mutated="${tmpdir}/without-shape-rule.toml"
python3 - "$config" "$mutated" <<'PY'
import pathlib
import sys

source = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
start = source.index("# Raw-hex secret material is identified")
end = source.index("# ── Upstream rules re-declared", start)
pathlib.Path(sys.argv[2]).write_text(source[:start] + source[end:], encoding="utf-8")
PY
grep -q 'corelink-secret-shaped-hex' "$config"
! grep -q 'corelink-secret-shaped-hex' "$mutated"
run_scan "$positive_root/wrangler.toml" "$mutated" 0 "removed-rule-mutation"

# An invalid replacement must fail closed during scanner configuration parsing.
broken="${tmpdir}/broken.toml"
python3 - "$config" "$broken" <<'PY'
import pathlib
import re
import sys

source = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
start = source.index('id = "corelink-secret-shaped-hex"')
prefix, block = source[:start], source[start:]
mutated, count = re.subn(r"(?m)^regex = .*$", "regex = '''([a-'''", block, count=1)
if count != 1 or mutated == block:
    raise SystemExit("invalid-regex mutation was not applied")
pathlib.Path(sys.argv[2]).write_text(prefix + mutated, encoding="utf-8")
PY
run_scan "$positive_root" "$broken" 2 "invalid-regex-mutation"

echo "gitleaks shape regression: PASS (path classes, opaque/legacy names, boundaries, allowlist, fail-closed mutations)"
