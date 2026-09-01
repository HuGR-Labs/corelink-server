#!/usr/bin/env bash
# Regression proof for `corelink-secret-shaped-hex`.
#
# This test uses gitleaks itself, not a copied regex: it proves the workflow's
# config rejects 64-hex material even under an opaque identifier, allows public
# digest context and fixture paths, and has a load-bearing custom rule.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
config="${repo_root}/.gitleaks.toml"
tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/corelink-gitleaks-shape.XXXXXX")"
trap 'test -n "${tmpdir:-}" && test -d "$tmpdir" && rm -rf -- "$tmpdir"' EXIT

command -v gitleaks >/dev/null || { echo "gitleaks is required" >&2; exit 2; }
test -f "$config" || { echo "missing .gitleaks.toml" >&2; exit 2; }

# Deterministic public probe; it is not a credential and is never printed.
probe="6b73d8c0f5a19e2b4d6087c3a9f1b5e7c2d4a6f8091b3d5e7f9a0c2e4b6d8f01"

run_scan() {
  local source="$1" config_path="$2" expected="$3" label="$4" out status
  out="${tmpdir}/${label}.out"
  set +e
  gitleaks detect --no-git --source "$source" --config "$config_path" \
    --redact --no-banner --exit-code 1 >"$out" 2>&1
  status=$?
  set -e
  if [[ "$status" != "$expected" ]]; then
    echo "${label}: expected gitleaks exit ${expected}, got ${status}" >&2
    sed -n '1,80p' "$out" >&2
    exit 1
  fi
}

positive_wrangler_toml="${tmpdir}/positive-wrangler-toml"
positive_wrangler_json="${tmpdir}/positive-wrangler-json"
positive_wrangler_jsonc="${tmpdir}/positive-wrangler-jsonc"
positive_dotenv_bare="${tmpdir}/positive-dotenv-bare"
positive_dotenv="${tmpdir}/positive-dotenv"
positive_secret_config="${tmpdir}/positive-secret-config"
positive_credential_config="${tmpdir}/positive-credential-config"
positive_audit_report="${tmpdir}/reports/audits"
negative="${tmpdir}/negative"
fixture="${tmpdir}/tests/fixture"
mkdir -p "$positive_wrangler_toml" "$positive_wrangler_json" "$positive_wrangler_jsonc" \
  "$positive_dotenv_bare" "$positive_dotenv" \
  "$positive_secret_config" "$positive_credential_config" "$positive_audit_report" \
  "$negative" "$fixture"
printf 'opaque_material = "%s"\nAUDIT_CHAIN_SIGNING_SEED_HEX = "%s"\n' "$probe" "$probe" >"${positive_wrangler_toml}/wrangler.toml"
printf 'opaque_material = "%s"\n' "$probe" >"${positive_wrangler_json}/wrangler.json"
printf 'opaque_material = "%s"\n' "$probe" >"${positive_wrangler_jsonc}/wrangler.jsonc"
printf 'opaque_material = "%s"\n' "$probe" >"${positive_dotenv_bare}/.env"
printf 'opaque_material = "%s"\n' "$probe" >"${positive_dotenv}/.env.production"
printf 'opaque_material = "%s"\n' "$probe" >"${positive_secret_config}/deploy-secrets.conf"
printf 'opaque_material = "%s"\n' "$probe" >"${positive_credential_config}/deploy-credentials.toml"
printf 'opaque_material = "%s"\n' "$probe" >"${positive_audit_report}/shape-incident.md"
printf 'sha256 = "%s"\nchecksum = "%s"\ndigest = "%s"\n' "$probe" "$probe" "$probe" >"${negative}/wrangler.toml"
printf 'opaque_material = "%s"\n' "$probe" >"${fixture}/wrangler.toml"

# The actual configured scanner must reject an opaque assignment in every path
# class declared by the rule, pass explicit public digests, and honor the
# narrowly-scoped versioned-fixture policy.
run_scan "$positive_wrangler_toml" "$config" 1 "positive-wrangler-toml"
run_scan "$positive_wrangler_json" "$config" 1 "positive-wrangler-json"
run_scan "$positive_wrangler_jsonc" "$config" 1 "positive-wrangler-jsonc"
run_scan "$positive_dotenv_bare" "$config" 1 "positive-dotenv-bare"
run_scan "$positive_dotenv" "$config" 1 "positive-dotenv"
run_scan "$positive_secret_config" "$config" 1 "positive-secret-config"
run_scan "$positive_credential_config" "$config" 1 "positive-credential-config"
run_scan "$positive_audit_report" "$config" 1 "positive-audit-report"
run_scan "$negative" "$config" 0 "public-digests"
run_scan "${tmpdir}/tests" "$config" 0 "fixture"

# Census the shape rule and its deliberate exception so a future refactor cannot
# silently reintroduce `keywords` (which makes gitleaks skip opaque values) or
# remove the public-digest boundary that the negative cell exercises.
python3 - "$config" <<'PY'
import pathlib, re, sys, tomllib
path = pathlib.Path(sys.argv[1])
text = path.read_text()
match = re.search(r'(?ms)^\[\[rules\]\]\nid = "corelink-secret-shaped-hex"\n(.*?)(?=^\[\[rules\]\]|^\[allowlist\])', text)
if not match:
    raise SystemExit("corelink-secret-shaped-hex rule is missing")
rule = match.group(0)
if 'keywords =' in rule:
    raise SystemExit("shape rule must not use variable-name keywords")
if 'secretGroup = 2' not in rule:
    raise SystemExit("shape rule must preserve secretGroup = 2")
if 'path =' not in rule or 'wrangler' not in rule or 'reports/audits' not in rule:
    raise SystemExit("shape rule must stay scoped to deployable configuration and audit reports")
if 'Public 64-hex digests/checksums' not in rule:
    raise SystemExit("shape rule lacks its explicit public-digest exception")
data = tomllib.loads(text)
shape = next((item for item in data['rules'] if item['id'] == 'corelink-secret-shaped-hex'), None)
if shape is None:
    raise SystemExit("TOML census could not find the shape rule")
allowlists = shape.get('allowlists', [])
if len(allowlists) != 2:
    raise SystemExit(f"expected exactly two shape-rule allowlists, found {len(allowlists)}")
descriptions = {item.get('description') for item in allowlists}
expected = {
    'Public 64-hex digests/checksums in explicit digest syntax, not credential assignments.',
    'Placeholder hex shapes (all-zeros/ones/a/f runs, deadbeef/cafebabe/0123456789abcdef repeats) — documentation filler, not key material.',
}
if descriptions != expected:
    raise SystemExit("shape-rule allowlist census changed; add a matching regression cell")
if not any('tests?/' in item for item in data['allowlist']['paths']):
    raise SystemExit("global versioned-fixture allowlist is missing")
PY

# Teeth mutation: remove this custom block only. The opaque positive must turn
# green because it intentionally has neither a secret-like name nor a default
# gitleaks signature; restoring the real config turns it red above.
mutated="${tmpdir}/without-shape-rule.toml"
python3 - "$config" "$mutated" <<'PY'
import pathlib, re, sys
source = pathlib.Path(sys.argv[1]).read_text()
start = source.index('# ── Raw-hex secret material: a SHAPE rule')
end = source.index('# ── Upstream rules re-declared', start)
pathlib.Path(sys.argv[2]).write_text(source[:start] + source[end:])
PY
run_scan "$positive_wrangler_toml" "$mutated" 0 "tooth-mutation"

echo "gitleaks shape regression: PASS (eight path-class positives red; digest + fixture green; mutation green)"
