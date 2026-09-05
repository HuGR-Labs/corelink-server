#!/usr/bin/env bash
# B-133 teeth: prove the policy checker and the archive/Cargo boundary reject
# concrete bypasses. Every successful cell has a stable unique ID; BACKLOG.md
# counts unique IDs rather than an easily-forged number of pleasant messages.
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
SANDBOX="$(mktemp -d)"
trap 'rm -rf "$SANDBOX"' EXIT
# The workflow has already put a direct, preinstalled toolchain binary on PATH
# from the BASE-only step before it changes HOME. Reproduce that ordering here;
# invoking the rustup shim after HOME is intentionally isolated would test a
# different failure (no default rustup home), not the Cargo config boundary.
HOST_CARGO="$(rustup which cargo)"
if [ ! -x "$HOST_CARGO" ]; then
  echo "INSTRUMENTO QUEBRADO: rustup which cargo não devolveu um binário executável" >&2
  exit 2
fi
HOST_CARGO_DENY="$(command -v cargo-deny || true)"
if [ ! -x "$HOST_CARGO_DENY" ]; then
  echo "INSTRUMENTO QUEBRADO: cargo-deny não está disponível para a célula CARGO" >&2
  exit 2
fi
fails=0
passed_cells=""
pass() {
  echo "  PASS  [$1] $2"
  case "$1" in
    B133-CELL-*) passed_cells="${passed_cells}${1}"$'\n' ;;
  esac
}
fail() { echo "  FAIL  [$1] $2"; fails=$((fails + 1)); }

expected_cell_ids() {
  awk 'BEGIN { for (i = 1; i <= 49; i++) printf "B133-CELL-%02d\n", i }'
}

has_exact_cell_census() {
  # PASS is evidence only when every numbered cell appears exactly once.  A
  # deleted tooth must make the suite red; repeating a pleasant tooth cannot
  # inflate the population.  Keep this POSIX-text based for Bash 3 on macOS.
  local observed="$1" expected actual unique_count pass_count
  expected="$(expected_cell_ids)"
  actual="$(printf '%s' "$observed" | sed '/^$/d' | sort -u)"
  unique_count="$(printf '%s\n' "$actual" | sed '/^$/d' | wc -l | tr -d ' ')"
  pass_count="$(printf '%s\n' "$observed" | sed '/^$/d' | wc -l | tr -d ' ')"
  [ "$pass_count" -eq 49 ] && [ "$unique_count" -eq 49 ] && [ "$actual" = "$expected" ]
}

prove_cell_census_is_non_vacuous() {
  local complete missing duplicate
  complete="$(expected_cell_ids)"
  missing="$(printf '%s\n' "$complete" | sed '$d')"
  duplicate="${complete}"$'\nB133-CELL-49'
  if ! has_exact_cell_census "$complete"; then
    fail B133-CENSUS "censo sintético completo deveria ser aceito"
  elif has_exact_cell_census "$missing"; then
    fail B133-CENSUS "remoção de célula foi aceita (censo vácuo)"
  elif has_exact_cell_census "$duplicate"; then
    fail B133-CENSUS "duplicata de célula foi aceita (PASS inflado)"
  fi
}

mkdir -p "$SANDBOX/.github/workflows" "$SANDBOX/scripts"
cp "$HERE/../.github/workflows/dependabot-policy.yml" "$SANDBOX/.github/workflows/"
cp "$HERE/../.github/workflows/dependabot-policy-trust-boundary.yml" "$SANDBOX/.github/workflows/"
cp "$HERE/check_dependabot_policy_trusted_tree.py" "$SANDBOX/scripts/"
cp "$HERE/test_ci_use_host_toolchain.sh" "$SANDBOX/scripts/"
cp "$HERE/ci-use-host-toolchain.sh" "$SANDBOX/scripts/"
cp "$HERE/test_dependabot_policy_trust_boundary.sh" "$SANDBOX/scripts/"
cp "$HERE/../rust-toolchain.toml" "$SANDBOX/"
WF="$SANDBOX/.github/workflows/dependabot-policy.yml"
TEETH_WF="$SANDBOX/.github/workflows/dependabot-policy-trust-boundary.yml"
WORKFLOW_SOURCE="$HERE/../.github/workflows/dependabot-policy.yml"
TEETH_WORKFLOW_SOURCE="$HERE/../.github/workflows/dependabot-policy-trust-boundary.yml"

run_checker() { (cd "$SANDBOX" && python3 scripts/check_dependabot_policy_trusted_tree.py); }
prepare_untrusted_tree() {
  rm -rf "$SANDBOX/pr-data"
  mkdir -p "$SANDBOX/pr-data/.github/workflows" "$SANDBOX/pr-data/scripts"
  cp "$WF" "$SANDBOX/pr-data/.github/workflows/dependabot-policy.yml"
  cp "$TEETH_WF" "$SANDBOX/pr-data/.github/workflows/dependabot-policy-trust-boundary.yml"
  cp "$SANDBOX/scripts/check_dependabot_policy_trusted_tree.py" "$SANDBOX/pr-data/scripts/"
  cp "$HERE/test_ci_use_host_toolchain.sh" "$SANDBOX/pr-data/scripts/"
  cp "$HERE/ci-use-host-toolchain.sh" "$SANDBOX/pr-data/scripts/"
  cp "$HERE/test_dependabot_policy_trust_boundary.sh" "$SANDBOX/pr-data/scripts/"
  cp "$HERE/../rust-toolchain.toml" "$SANDBOX/pr-data/"
}
run_checker_static() {
  (cd "$SANDBOX" && python3 scripts/check_dependabot_policy_trusted_tree.py --untrusted-tree "$SANDBOX/pr-data")
}
expect_static_red() {
  local id="$1" description="$2" expected="$3"
  if run_checker_static >"$SANDBOX/checker-$id.out" 2>&1; then
    fail "$id" "$description (checker aceitou o mutante)"
  elif grep -Fq -- "$expected" "$SANDBOX/checker-$id.out"; then
    pass "$id" "$description"
  else
    fail "$id" "$description (checker não nomeou '$expected')"
    sed 's/^/        /' "$SANDBOX/checker-$id.out"
  fi
}
restore_workflow() {
  cp "$WORKFLOW_SOURCE" "$WF"
  cp "$TEETH_WORKFLOW_SOURCE" "$TEETH_WF"
}
expect_checker_red() {
  local id="$1" description="$2" expected="$3"
  if run_checker >"$SANDBOX/checker-$id.out" 2>&1; then
    fail "$id" "$description (checker aceitou o mutante)"
  elif grep -Fq -- "$expected" "$SANDBOX/checker-$id.out"; then
    pass "$id" "$description"
  else
    fail "$id" "$description (checker não nomeou '$expected')"
    sed 's/^/        /' "$SANDBOX/checker-$id.out"
  fi
  restore_workflow
}

if run_checker >"$SANDBOX/checker-baseline.out" 2>&1; then
  pass B133-CELL-01 "baseline checker is green"
else
  fail B133-CELL-01 "baseline checker is green"
  sed 's/^/        /' "$SANDBOX/checker-baseline.out"
fi

# This focused lane is deliberately small enough to run during cold review.
# It reproduces exactly the three P1 mutants: changing the interpreter via
# `shell:`, poisoning a pinned `uses:` action via executable environment
# selectors, and the TOML parser cases exercised by test_ci_use_host_toolchain.
# The default path below remains the full historical adversarial suite.
if [[ "${B133_REVIEW_P1_ONLY:-0}" == "1" ]]; then
  python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '      - name: Policy-gate audit log\n'
if needle not in s:
    raise SystemExit("review shell mutation anchor missing")
p.write_text(s.replace(needle, '      - name: Policy-gate audit log\n        shell: bash {0}\n', 1))
PY
  expect_checker_red B133-CELL-35 "checker rejects a post-checkout attacker shell" "shell='bash {0}' não está no allowlist"

  python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '          CARGO_HOME: ${{ runner.temp }}/corelink-dependabot-cargo-home\n        with:\n          tool: cargo-deny@0.19.8'
if needle not in s:
    raise SystemExit("review uses environment mutation anchor missing")
replacement = (
    '          CARGO_HOME: ${{ runner.temp }}/corelink-dependabot-cargo-home\n'
    '          CARGO: /tmp/attacker-cargo\n'
    '          BASH_ENV: /tmp/attacker-bash-env\n'
    '          PATH: /tmp/attacker-path\n'
    '          NODE_OPTIONS: --require=/tmp/attacker-node.js\n'
    '        with:\n          tool: cargo-deny@0.19.8'
)
p.write_text(s.replace(needle, replacement, 1))
PY
  expect_checker_red B133-CELL-36 "checker rejects executable environment sinks on uses step" "env extras não permitidos: BASH_ENV, CARGO, NODE_OPTIONS, PATH"

  python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = 'name: dependabot-policy (license + status-check enforcement)\n'
if needle not in s:
    raise SystemExit("workflow default-shell mutation anchor missing")
p.write_text(s.replace(needle, needle + 'defaults:\n  run:\n    shell: bash {0}\n', 1))
PY
  expect_checker_red B133-CELL-37 "checker rejects workflow inherited shell" "workflow: defaults.run.shell='bash {0}' não está no allowlist"

  python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = 'name: dependabot-policy (license + status-check enforcement)\n'
if needle not in s:
    raise SystemExit("workflow environment mutation anchor missing")
p.write_text(s.replace(needle, needle + 'env:\n  NODE_OPTIONS: --require=/tmp/attacker.js\n', 1))
PY
  expect_checker_red B133-CELL-38 "checker rejects workflow inherited NODE_OPTIONS" "env do workflow: env extras não permitidos: NODE_OPTIONS"

  python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '  policy-gate:\n'
if needle not in s:
    raise SystemExit("job default-shell mutation anchor missing")
p.write_text(s.replace(needle, needle + '    defaults:\n      run:\n        shell: bash {0}\n', 1))
PY
  expect_checker_red B133-CELL-39 "checker rejects job inherited shell" "policy-gate: defaults.run.shell='bash {0}' não está no allowlist"

  python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '    env:\n      BASE_REF: ${{ github.event.pull_request.base.ref }}\n'
if needle not in s:
    raise SystemExit("job environment mutation anchor missing")
p.write_text(s.replace(needle, needle + '      NODE_OPTIONS: --require=/tmp/attacker.js\n', 1))
PY
  expect_checker_red B133-CELL-40 "checker rejects job inherited NODE_OPTIONS" "env do policy-gate: env extras não permitidos: NODE_OPTIONS"

  echo
  if [[ "$fails" -eq 0 ]]; then
    echo "dependabot-policy review-P1 teeth: all selected cells passed"
    exit 0
  fi
  echo "dependabot-policy review-P1 teeth: $fails selected cell(s) failed"
  exit 1
fi

# This narrow lane exercises the anti-vacuum accounting without invoking Cargo.
# It is intentionally separate from REVIEW_P1_ONLY: the latter proves six
# workflow mutations, while this proves deletion/duplication cannot forge 49.
if [[ "${B133_CENSUS_ONLY:-0}" == "1" ]]; then
  prove_cell_census_is_non_vacuous
  if [[ "$fails" -eq 0 ]]; then
    echo "B-133 cell census rejects removal and duplication"
    exit 0
  fi
  exit 1
fi

# Direct Cargo is not a .sh invocation. Removing --locked must be a red cell,
# not merely a lint warning, or lockfile resolution becomes an unreviewed sink.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '"$CARGO_DENY_BIN" --all-features --locked check --config "$DENY_CONFIG" sources licenses bans'
if needle not in s:
    raise SystemExit("direct-Cargo mutation anchor missing")
p.write_text(s.replace(needle, needle.replace(" --locked", ""), 1))
PY
expect_checker_red B133-CELL-02 "checker rejects direct Cargo without --locked" "binário confiável direto, com --locked"

# `install-action` defaults to cargo-binstall and can eventually use cargo
# install. A policy gate must not compile a package from PR-controlled manifests.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '          fallback: none\n'
if needle not in s:
    raise SystemExit("fallback mutation anchor missing")
p.write_text(s.replace(needle, '          fallback: cargo-install\n', 1))
PY
expect_checker_red B133-CELL-03 "checker rejects installer fallback" "fallback deve ser 'none'"

# The symlink preflight must happen before tar sees a PR byte. Removing its
# typed-tree query would turn an archive into a filesystem escape primitive.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = 'git ls-tree -r --full-tree HEAD'
if needle not in s:
    raise SystemExit("symlink mutation anchor missing")
p.write_text(s.replace(needle, 'git show --format= HEAD', 1))
PY
expect_checker_red B133-CELL-04 "checker rejects missing archive-symlink preflight" "git ls-tree -r --full-tree HEAD"

# A conditional preflight used to make deleted manifests silently skip Cargo.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '          for manifest in Cargo.toml Cargo.lock deny.toml; do\n'
if needle not in s:
    raise SystemExit("manifest mutation anchor missing")
p.write_text(s.replace(needle, '          for manifest in Cargo.lock; do\n', 1))
PY
expect_checker_red B133-CELL-05 "checker rejects missing regular-manifest census" "for manifest in Cargo.toml Cargo.lock deny.toml; do"

# The dependency graph is PR data, but deny.toml is the decision policy. A
# local config (or a copied PR config) is a policy substitution, not analysis.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = 'cp _base/deny.toml "$DENY_CONFIG"'
if needle not in s:
    raise SystemExit("deny source mutation anchor missing")
p.write_text(s.replace(needle, 'cp "$POLICY_TREE/deny.toml" "$DENY_CONFIG"', 1))
PY
expect_checker_red B133-CELL-06 "checker rejects PR-controlled deny policy" "cp _base/deny.toml \"\$DENY_CONFIG\""

# A local `uses: ./…` is a composite action from the PR tree even though the
# workflow itself comes from the base ref.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = 'uses: taiki-e/install-action@07b4745e0c39a41822af610387492e3e53aa222b'
if needle not in s:
    raise SystemExit("composite mutation anchor missing")
p.write_text(s.replace(needle, 'uses: ./attacker-composite', 1))
PY
expect_checker_red B133-CELL-07 "checker rejects local composite action" "ação local/composite"

# Changing triggers is a control-plane bypass: the policy/checker/test must
# remain observable without guessing whether a path glob still includes them.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = 'types: [opened, synchronize, reopened, labeled, ready_for_review]'
if needle not in s:
    raise SystemExit("trigger mutation anchor missing")
p.write_text(s.replace(needle, 'types: [opened, reopened, labeled, ready_for_review]', 1))
PY
expect_checker_red B133-CELL-08 "checker rejects non-self-triggering workflow" "faltam types synchronize"

# A trusted pathname is insufficient when a script derives its config from
# cwd. This is the exact `_base/`-but-PR-cwd half-fix B-133 was opened for.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '        working-directory: _base\n        env:\n          HOST_TRIPLE:'
if needle not in s:
    raise SystemExit("working-directory mutation anchor missing")
p.write_text(s.replace(needle, '        env:\n          HOST_TRIPLE:', 1))
PY
expect_checker_red B133-CELL-09 "checker rejects BASE script with PR working directory" "host toolchain deve usar working-directory: _base"

# CARGO_HOME and HOME must be fresh for each PR-data consumer. Deleting one
# from the installer returns it to a shared credential/config location.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '          CARGO_HOME: ${{ runner.temp }}/corelink-dependabot-cargo-home\n        with:\n          tool: cargo-deny@0.19.8'
if needle not in s:
    raise SystemExit("installer environment mutation anchor missing")
p.write_text(s.replace(needle, '        with:\n          tool: cargo-deny@0.19.8', 1))
PY
expect_checker_red B133-CELL-10 "checker rejects installer without ephemeral Cargo home" "instalação cargo-deny isolada: CARGO_HOME"

# This is the shell-parser bypass that invalidated the first checker: a regex
# looking only at command separators missed an interpreter after `then`, after
# an assignment prefix, and a direct executable. The exact run-block allowlist
# must red all three forms before any PR-data command can run.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '          AUDIT_HEAD_SHA: ${{ github.event.pull_request.head.sha }}\n        run: |\n'
if needle not in s:
    raise SystemExit("shell-bypass mutation anchor missing")
p.write_text(s.replace(needle, needle + '          if true; then bash scripts/evil.sh; fi\n', 1))
PY
expect_checker_red B133-CELL-16 "checker rejects interpreter hidden after then" "corpo run divergiu do allowlist exato"

python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '          AUDIT_HEAD_SHA: ${{ github.event.pull_request.head.sha }}\n        run: |\n'
if needle not in s:
    raise SystemExit("dot-exec mutation anchor missing")
p.write_text(s.replace(needle, needle + '          if true; then ./evil.sh; fi\n', 1))
PY
expect_checker_red B133-CELL-17 "checker rejects direct executable hidden after then" "corpo run divergiu do allowlist exato"

python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '          AUDIT_HEAD_SHA: ${{ github.event.pull_request.head.sha }}\n        run: |\n'
if needle not in s:
    raise SystemExit("assignment-prefix mutation anchor missing")
p.write_text(s.replace(needle, needle + '          if true; then EVIL=1 bash scripts/evil.sh; fi\n', 1))
PY
expect_checker_red B133-CELL-18 "checker rejects assignment-prefixed interpreter" "corpo run divergiu do allowlist exato"

# The triggering actor may be a maintainer applying a label, so only the PR
# author distinguishes a Dependabot PR from a pass-through sentinel event.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = "    if: github.event.pull_request.user.login == 'dependabot[bot]'\n"
if needle not in s:
    raise SystemExit("Dependabot author mutation anchor missing")
p.write_text(s.replace(needle, "    if: github.actor == 'dependabot[bot]'\n", 1))
PY
expect_checker_red B133-CELL-19 "checker rejects actor-triggered Dependabot bypass" "autor imutável do PR Dependabot"

# Source policy is read from the lockfile before Cargo gets a chance to contact
# an attacker-selected registry or Git source. Removing that static preflight
# must fail independently of the later cargo-deny `sources` check.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '^[[:space:]]*source[[:space:]]*=[[:space:]]*/ {'
if needle not in s:
    raise SystemExit("source-preflight mutation anchor missing")
p.write_text(s.replace(needle, '^source = ', 1))
PY
expect_checker_red B133-CELL-20 "checker rejects missing pre-Cargo source preflight" "preparo de Cargo sem controle obrigatório"

# Exercise the preflight itself against an attacker-supplied lock source. This
# happens before any Cargo invocation; a Git source therefore cannot turn a
# source-policy failure into an outbound fetch or credential-provider lookup.
SOURCE_LOCK="$SANDBOX/source-preflight-Cargo.lock"
printf '%s\n' \
  '[[package]]' \
  'name = "safe-registry"' \
  'version = "1.0.0"' \
  'source = "registry+https://github.com/rust-lang/crates.io-index"' \
  '' \
  '[[package]]' \
  'name = "attacker-git"' \
  'version = "1.0.0"' \
  'source = "git+https://example.invalid/attacker#deadbeef"' > "$SOURCE_LOCK"
invalid_sources="$(awk '
  /^source = / && $0 != "source = \"registry+https://github.com/rust-lang/crates.io-index\"" {
    print FNR ": " $0
  }
' "$SOURCE_LOCK")"
if grep -Fq 'source = "git+https://example.invalid/attacker#deadbeef"' <<<"$invalid_sources"; then
  pass B133-CELL-28 "Cargo.lock source preflight rejects Git source before Cargo/network"
else
  fail B133-CELL-28 "Cargo.lock source preflight rejects Git source before Cargo/network"
  printf '        saw: %s\n' "$invalid_sources"
fi

# Cargo.lock is TOML, so a key may be indented or padded around `=`. This is
# the exact old-parser bypass: /^source = / missed the attacker source below.
SOURCE_WHITESPACE_LOCK="$SANDBOX/source-whitespace-Cargo.lock"
printf '%s\n' \
  '[[package]]' \
  'name = "attacker-indented-git"' \
  'version = "1.0.0"' \
  '  source = "git+https://example.invalid/whitespace#deadbeef"' > "$SOURCE_WHITESPACE_LOCK"
whitespace_invalid_sources="$(awk '
  /^[[:space:]]*source[[:space:]]*=[[:space:]]*/ {
    source_value = $0
    sub(/^[[:space:]]*source[[:space:]]*=[[:space:]]*/, "", source_value)
    if (source_value != "\"registry+https://github.com/rust-lang/crates.io-index\"") {
      print FNR ": " $0
    }
  }
' "$SOURCE_WHITESPACE_LOCK")"
if grep -Fq 'source = "git+https://example.invalid/whitespace#deadbeef"' <<<"$whitespace_invalid_sources"; then
  pass B133-CELL-29 "Cargo.lock source preflight parses indented TOML source"
else
  fail B133-CELL-29 "Cargo.lock source preflight parses indented TOML source"
  printf '        saw: %s\n' "$whitespace_invalid_sources"
fi

# `cargo deny` asks Cargo to resolve an alias from config/environment. The gate
# must execute the pinned cargo-deny file installed under its private HOME.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '"$CARGO_DENY_BIN" --all-features --locked check --config "$DENY_CONFIG" sources licenses bans'
if needle not in s:
    raise SystemExit("direct-cargo-deny mutation anchor missing")
p.write_text(s.replace(needle, 'cargo deny --all-features --locked check --config "$DENY_CONFIG" sources licenses bans', 1))
PY
expect_checker_red B133-CELL-21 "checker rejects Cargo alias invocation" "binário confiável direto"

# Both workspace wrapper spellings override an inherited/ancestor Cargo
# configuration. Deleting either leaves a code-execution sink open.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '      CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER: ""\n'
if needle not in s:
    raise SystemExit("workspace-wrapper environment mutation anchor missing")
p.write_text(s.replace(needle, '', 1))
PY
expect_checker_red B133-CELL-22 "checker rejects inherited Cargo workspace wrapper" "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER"

# A private Cargo credential-provider setting must not survive from the runner;
# it could make Cargo execute an arbitrary provider while inspecting PR data.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '      CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS: ""\n'
if needle not in s:
    raise SystemExit("credential-provider mutation anchor missing")
p.write_text(s.replace(needle, '', 1))
PY
expect_checker_red B133-CELL-23 "checker rejects inherited Cargo credential provider" "CARGO_REGISTRY_GLOBAL_CREDENTIAL_PROVIDERS"

# GIT_CONFIG_COUNT makes GIT_CONFIG_KEY_n/VALUE_n live even with global and
# system config disabled. A zero count is the authoritative fail-closed sink.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '      GIT_CONFIG_COUNT: "0"\n'
if needle not in s:
    raise SystemExit("git-config-count mutation anchor missing")
p.write_text(s.replace(needle, '', 1))
PY
expect_checker_red B133-CELL-24 "checker rejects injected Git config population" "GIT_CONFIG_COUNT"

# cargo-deny reaches cargo-metadata, whose dependency honors $CARGO. The job
# must pin that executable selector to the trusted host-toolchain command.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '      CARGO: cargo\n'
if needle not in s:
    raise SystemExit("CARGO environment mutation anchor missing")
p.write_text(s.replace(needle, '      CARGO: /tmp/attacker-cargo\n', 1))
PY
expect_checker_red B133-CELL-30 "checker rejects inherited CARGO executable" "env do policy-gate: CARGO"

# The job environment is a closed-world sink census. A new Cargo selector
# cannot appear merely because it was not in an older required-key list.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '      CARGO: cargo\n'
if needle not in s:
    raise SystemExit("extra-CARGO environment mutation anchor missing")
p.write_text(s.replace(needle, needle + '      CARGO_STEALTH: /tmp/attacker-cargo\n', 1))
PY
expect_checker_red B133-CELL-31 "checker rejects extra Cargo executable sink" "env extras não permitidos: CARGO_STEALTH"

# The PR-side teeth workflow is the pre-merge proof for the checker itself.
# Removing its self path would make a control-plane edit disappear until after
# merge, so the BASE checker must reject that population change by name.
python3 - "$TEETH_WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '      - ".github/workflows/dependabot-policy-trust-boundary.yml"\n'
if needle not in s:
    raise SystemExit("teeth self-trigger mutation anchor missing")
p.write_text(s.replace(needle, '', 1))
PY
expect_checker_red B133-CELL-33 "checker rejects removed premerge teeth self-trigger" "premerge B-133: paths do pull_request_target divergentes"

# Removing the regular PR trigger entirely is distinct from narrowing its input
# census: without it, the proof is delayed until after merge. Keep this a
# separate named tooth so the trigger's existence cannot be inferred from a
# pleasant-looking paths-only result.
python3 - "$TEETH_WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '  pull_request_target:\n'
if needle not in s:
    raise SystemExit("premerge pull_request_target mutation anchor missing")
p.write_text(s.replace(needle, '  pull_request_target_removed:\n', 1))
PY
expect_checker_red B133-CELL-34 "checker rejects removed BASE-controlled premerge trigger" "premerge B-133: gatilho pull_request_target BASE-controlado ausente"

# A shared runner must never run a regular pull_request version of these
# controls. The workflow itself is BASE-controlled only under
# pull_request_target; changing that event is a red trust-boundary regression.
python3 - "$TEETH_WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '  pull_request_target:\n'
if needle not in s:
    raise SystemExit("regular pull_request mutation anchor missing")
p.write_text(s.replace(needle, '  pull_request:\n', 1))
PY
expect_checker_red B133-CELL-41 "checker rejects PR-controlled executable workflow event" "teeth não podem executar pull_request em runner compartilhado"

# The static inspection receives the PR checkout strictly as bytes. A run step
# resolving from `_pr-data` would turn the shared runner label back into an
# untrusted executor even though the event remains pull_request_target.
python3 - "$TEETH_WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '        working-directory: _base\n        run: bash scripts/test_ci_use_host_toolchain.sh\n'
if needle not in s:
    raise SystemExit("untrusted working-directory mutation anchor missing")
p.write_text(s.replace(needle, needle.replace('_base', '_pr-data'), 1))
PY
expect_checker_red B133-CELL-42 "checker rejects BASE tooth resolving from PR data" "deve rodar de _base"

# This mutation is not passed to a shell or YAML parser in the checker. The
# BASE checker compares fixed control paths as regular-file bytes and stops
# before the BASE runtime teeth can execute.
prepare_untrusted_tree
printf '\n# attacker control mutation\n' >> "$SANDBOX/pr-data/scripts/ci-use-host-toolchain.sh"
if run_checker_static >"$SANDBOX/checker-43.out" 2>&1; then
  fail B133-CELL-43 "static checker rejects changed B-133 control bytes (checker aceitou o mutante)"
elif grep -Fq -- "PR alterou controle B-133 'scripts/ci-use-host-toolchain.sh'; execução recusada" "$SANDBOX/checker-43.out"; then
  pass B133-CELL-43 "static checker rejects changed B-133 control bytes before execution"
else
  fail B133-CELL-43 "static checker rejects changed B-133 control bytes (checker não nomeou mutante)"
  sed 's/^/        /' "$SANDBOX/checker-43.out"
fi

# `lstat` on only the leaf is insufficient: Path.read_bytes follows a parent
# symlink first. Exercise every parent in the fixed control census, both a
# relative escape and absolute targets, so the BASE checker never descends
# into an attacker-selected directory.
prepare_untrusted_tree
rm -rf "$SANDBOX/pr-data/scripts"
ln -s "$SANDBOX/scripts" "$SANDBOX/pr-data/scripts"
expect_static_red B133-CELL-44 "static checker rejects absolute scripts parent symlink" "pai de controle B-133 'scripts' é symlink; leitura recusada"

prepare_untrusted_tree
rm -rf "$SANDBOX/pr-data/.github"
ln -s "$SANDBOX/.github" "$SANDBOX/pr-data/.github"
expect_static_red B133-CELL-45 "static checker rejects absolute .github parent symlink" "pai de controle B-133 '.github' é symlink; leitura recusada"

prepare_untrusted_tree
rm -rf "$SANDBOX/pr-data/.github/workflows"
ln -s "$SANDBOX/.github/workflows" "$SANDBOX/pr-data/.github/workflows"
expect_static_red B133-CELL-46 "static checker rejects absolute workflows parent symlink" "pai de controle B-133 '.github/workflows' é symlink; leitura recusada"

prepare_untrusted_tree
rm -rf "$SANDBOX/pr-data/scripts" "$SANDBOX/pr-data/.github"
ln -s ../scripts "$SANDBOX/pr-data/scripts"
ln -s ../.github "$SANDBOX/pr-data/.github"
expect_static_red B133-CELL-47 "static checker rejects combined relative parent escapes" "pai de controle B-133 '.github' é symlink; leitura recusada"

prepare_untrusted_tree
rm -rf "$SANDBOX/pr-data/scripts"
ln -s /tmp "$SANDBOX/pr-data/scripts"
expect_static_red B133-CELL-48 "static checker rejects absolute parent escape" "pai de controle B-133 'scripts' é symlink; leitura recusada"

prepare_untrusted_tree
rm "$SANDBOX/pr-data/scripts/ci-use-host-toolchain.sh"
ln -s "$SANDBOX/scripts/ci-use-host-toolchain.sh" "$SANDBOX/pr-data/scripts/ci-use-host-toolchain.sh"
expect_static_red B133-CELL-49 "static checker rejects control leaf symlink" "PR tornou controle B-133 não-regular 'scripts/ci-use-host-toolchain.sh'"

restore_workflow

# `shell:` changes how a byte-identical `run:` is interpreted. It must be in
# the closed-world census itself, not inferred from a hash of the run body.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '      - name: Policy-gate audit log\n'
if needle not in s:
    raise SystemExit("shell mutation anchor missing")
p.write_text(s.replace(needle, '      - name: Policy-gate audit log\n        shell: bash {0}\n', 1))
PY
expect_checker_red B133-CELL-35 "checker rejects a post-checkout attacker shell" "shell='bash {0}' não está no allowlist"

# A `uses:` action is also an executable sink. Its environment is not exempt:
# PATH, NODE_OPTIONS, BASH_ENV and CARGO can all change what runs before or
# inside the pinned action, so all four must be rejected as one exact-map
# mutation rather than merely being absent from a few `run:` checks.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '          CARGO_HOME: ${{ runner.temp }}/corelink-dependabot-cargo-home\n        with:\n          tool: cargo-deny@0.19.8'
if needle not in s:
    raise SystemExit("uses environment mutation anchor missing")
replacement = (
    '          CARGO_HOME: ${{ runner.temp }}/corelink-dependabot-cargo-home\n'
    '          CARGO: /tmp/attacker-cargo\n'
    '          BASH_ENV: /tmp/attacker-bash-env\n'
    '          PATH: /tmp/attacker-path\n'
    '          NODE_OPTIONS: --require=/tmp/attacker-node.js\n'
    '        with:\n          tool: cargo-deny@0.19.8'
)
p.write_text(s.replace(needle, replacement, 1))
PY
expect_checker_red B133-CELL-36 "checker rejects executable environment sinks on uses step" "env extras não permitidos: BASH_ENV, CARGO, NODE_OPTIONS, PATH"

# Ancestor configuration runs before the exact per-step contract. Keep these
# in the default suite as well as the focused review lane above.
python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = 'name: dependabot-policy (license + status-check enforcement)\n'
if needle not in s:
    raise SystemExit("workflow default-shell mutation anchor missing")
p.write_text(s.replace(needle, needle + 'defaults:\n  run:\n    shell: bash {0}\n', 1))
PY
expect_checker_red B133-CELL-37 "checker rejects workflow inherited shell" "workflow: defaults.run.shell='bash {0}' não está no allowlist"

python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = 'name: dependabot-policy (license + status-check enforcement)\n'
if needle not in s:
    raise SystemExit("workflow environment mutation anchor missing")
p.write_text(s.replace(needle, needle + 'env:\n  NODE_OPTIONS: --require=/tmp/attacker.js\n', 1))
PY
expect_checker_red B133-CELL-38 "checker rejects workflow inherited NODE_OPTIONS" "env do workflow: env extras não permitidos: NODE_OPTIONS"

python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '  policy-gate:\n'
if needle not in s:
    raise SystemExit("job default-shell mutation anchor missing")
p.write_text(s.replace(needle, needle + '    defaults:\n      run:\n        shell: bash {0}\n', 1))
PY
expect_checker_red B133-CELL-39 "checker rejects job inherited shell" "policy-gate: defaults.run.shell='bash {0}' não está no allowlist"

python3 - "$WF" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
needle = '    env:\n      BASE_REF: ${{ github.event.pull_request.base.ref }}\n'
if needle not in s:
    raise SystemExit("job environment mutation anchor missing")
p.write_text(s.replace(needle, needle + '      NODE_OPTIONS: --require=/tmp/attacker.js\n', 1))
PY
expect_checker_red B133-CELL-40 "checker rejects job inherited NODE_OPTIONS" "env do policy-gate: env extras não permitidos: NODE_OPTIONS"

# Use a tiny git fixture to prove the runtime preflight sees a symlink in Git's
# typed tree before extraction. The shell deliberately mirrors the workflow's
# `git ls-tree` guard rather than trusting tar's platform-specific behavior.
FIXTURE="$SANDBOX/archive-fixture"
mkdir -p "$FIXTURE/src"
printf '%s\n' '[package]' 'name = "b133-archive"' 'version = "0.1.0"' 'edition = "2021"' > "$FIXTURE/Cargo.toml"
printf '%s\n' 'version = 4' > "$FIXTURE/Cargo.lock"
printf '%s\n' '[licenses]' 'allow = ["MIT"]' > "$FIXTURE/deny.toml"
printf '%s\n' 'fn main() {}' > "$FIXTURE/src/main.rs"
(cd "$FIXTURE" && git init -q . && git config user.email b133@test && git config user.name b133 && git add -A && git commit -qm clean)
ln -s /outside "$FIXTURE/archive-escape"
(cd "$FIXTURE" && git add archive-escape && git commit -qm symlink)
unsafe_entries="$(cd "$FIXTURE" && git ls-tree -r --full-tree HEAD | awk '$1 == "120000" || $1 == "160000" { print $1 " " $4 }')"
if grep -qx '120000 archive-escape' <<<"$unsafe_entries"; then
  pass B133-CELL-11 "typed-tree archive preflight rejects malicious symlink before extraction"
else
  fail B133-CELL-11 "typed-tree archive preflight rejects malicious symlink before extraction"
  printf '        saw: %s\n' "$unsafe_entries"
fi

# Reset the fixture to a regular tree, then prove a missing manifest fails the
# same post-extraction regular-file contract instead of becoming a skipped gate.
(cd "$FIXTURE" && git rm -q archive-escape deny.toml && git commit -qm missing-deny)
MISSING_TREE="$SANDBOX/missing-tree"
mkdir -p "$MISSING_TREE"
(cd "$FIXTURE" && git archive --format=tar HEAD) | tar -x -C "$MISSING_TREE"
missing_ok=1
for manifest in Cargo.toml Cargo.lock deny.toml; do
  if [ ! -f "$MISSING_TREE/$manifest" ] || [ -L "$MISSING_TREE/$manifest" ]; then
    missing_ok=0
  fi
done
if [ "$missing_ok" -eq 0 ]; then
  pass B133-CELL-12 "regular-manifest contract fails closed when deny.toml is removed"
else
  fail B133-CELL-12 "regular-manifest contract fails closed when deny.toml is removed"
fi

# Build a PR-controlled Cargo config pointing to an executable wrapper. Full
# metadata reaches it; this is a measured exploit, not a parser hypothesis.
PROBE="$SANDBOX/probe"
mkdir -p "$PROBE/.cargo" "$PROBE/src"
printf '%s\n' '[package]' 'name = "b133-wrapper-probe"' 'version = "0.1.0"' 'edition = "2021"' > "$PROBE/Cargo.toml"
printf '%s\n' '# Cargo lock fixture.' 'version = 4' > "$PROBE/Cargo.lock"
printf '%s\n' '[licenses]' 'allow = ["MIT"]' > "$PROBE/deny.toml"
printf '%s\n' 'fn main() {}' > "$PROBE/src/main.rs"
WRAPPER="$SANDBOX/wrapper.sh"
MARKER="$SANDBOX/wrapper-ran"
printf '%s\n' '#!/bin/sh' "printf 'INVOKED\\n' >> '$MARKER'" 'exec "$@"' > "$WRAPPER"
chmod +x "$WRAPPER"
printf '[build]\nrustc-wrapper = "%s"\n' "$WRAPPER" > "$PROBE/.cargo/config.toml"
(cd "$PROBE" && cargo metadata --format-version 1 >"$SANDBOX/metadata.out" 2>"$SANDBOX/metadata.err")
if [ -s "$MARKER" ]; then
  pass B133-CELL-13 "malicious PR rustc-wrapper is reproducibly reached by full metadata"
else
  fail B133-CELL-13 "malicious PR rustc-wrapper is reproducibly reached by full metadata"
  sed 's/^/        /' "$SANDBOX/metadata.err"
fi

# `rustc-workspace-wrapper` is a second, independent Cargo configuration
# source. A dynamic marker proves it executes for a workspace package, rather
# than merely proving that the checker recognizes its spelling.
WORKSPACE_WRAPPER="$SANDBOX/workspace-wrapper.sh"
WORKSPACE_MARKER="$SANDBOX/workspace-wrapper-ran"
printf '%s\n' '#!/bin/sh' "printf 'INVOKED\\n' >> '$WORKSPACE_MARKER'" 'exec "$@"' > "$WORKSPACE_WRAPPER"
chmod +x "$WORKSPACE_WRAPPER"
printf '[build]\nrustc-workspace-wrapper = "%s"\n' "$WORKSPACE_WRAPPER" > "$PROBE/.cargo/config.toml"
(cd "$PROBE" && CARGO_TARGET_DIR="$SANDBOX/workspace-wrapper-target" "$HOST_CARGO" check >"$SANDBOX/workspace-wrapper.out" 2>"$SANDBOX/workspace-wrapper.err")
workspace_wrapper_rc=$?
if [ "$workspace_wrapper_rc" -eq 0 ] && [ -s "$WORKSPACE_MARKER" ]; then
  pass B133-CELL-25 "malicious workspace wrapper is reproducibly reached by Cargo"
else
  fail B133-CELL-25 "malicious workspace wrapper is reproducibly reached by Cargo (rc=$workspace_wrapper_rc)"
  sed 's/^/        /' "$SANDBOX/workspace-wrapper.err"
fi

# Reproduce the data boundary: archive the PR, remove its Cargo config/toolchain
# selectors, and invoke metadata with the same environment sinks neutralized by
# the policy job. The policy tree itself has no attacker wrapper.
(cd "$PROBE" && git init -q . && git config user.email b133@test && git config user.name b133 && git add -A && git commit -qm fixture)
SAFE="$SANDBOX/safe"
SAFE_HOME="$SANDBOX/cargo-home"
mkdir -p "$SAFE" "$SAFE_HOME"
(cd "$PROBE" && git archive --format=tar HEAD) | tar -x -C "$SAFE"
rm -rf "$SAFE/.cargo" "$MARKER"
rm -f "$SAFE/rust-toolchain" "$SAFE/rust-toolchain.toml"
(cd "$SAFE" && PATH="${HOST_CARGO%/*}:$PATH" HOME="$SANDBOX/policy-home" CARGO_HOME="$SAFE_HOME" CARGO_BUILD_RUSTC_WRAPPER='' CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER='' CARGO_BUILD_RUSTFLAGS='' RUSTC_WRAPPER='' RUSTC_WORKSPACE_WRAPPER='' RUSTC=rustc RUSTFLAGS='' "$HOST_CARGO" metadata --format-version 1 --locked >"$SANDBOX/safe-metadata.out" 2>"$SANDBOX/safe-metadata.err")
safe_rc=$?
if [ "$safe_rc" -eq 0 ] && [ ! -e "$MARKER" ] && [ ! -e "$SAFE/.cargo" ]; then
  pass B133-CELL-14 "sanitized archive blocks PR-local wrapper execution"
else
  fail B133-CELL-14 "sanitized archive blocks PR-local wrapper execution (rc=$safe_rc)"
  sed 's/^/        /' "$SANDBOX/safe-metadata.err"
fi

# Cargo walks ancestors for .cargo/config.toml. The workflow's empty wrapper
# environment must win over a malicious ancestor even after the local config
# was removed; this protects the runner.temp hierarchy, not just the archive.
mkdir -p "$SANDBOX/.cargo"
printf '[build]\nrustc-wrapper = "%s"\nrustc-workspace-wrapper = "%s"\n' "$WRAPPER" "$WORKSPACE_WRAPPER" > "$SANDBOX/.cargo/config.toml"
rm -f "$MARKER" "$WORKSPACE_MARKER"
(cd "$SAFE" && PATH="${HOST_CARGO%/*}:$PATH" HOME="$SANDBOX/policy-home" CARGO_HOME="$SAFE_HOME" CARGO_BUILD_RUSTC_WRAPPER='' CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER='' CARGO_BUILD_RUSTFLAGS='' RUSTC_WRAPPER='' RUSTC_WORKSPACE_WRAPPER='' RUSTC=rustc RUSTFLAGS='' "$HOST_CARGO" metadata --format-version 1 --locked >"$SANDBOX/ancestor-metadata.out" 2>"$SANDBOX/ancestor-metadata.err")
ancestor_rc=$?
if [ "$ancestor_rc" -eq 0 ] && [ ! -e "$MARKER" ] && [ ! -e "$WORKSPACE_MARKER" ]; then
  pass B133-CELL-15 "empty Cargo wrapper sinks defeat malicious ancestor config"
else
  fail B133-CELL-15 "empty Cargo wrapper sinks defeat malicious ancestor config (rc=$ancestor_rc)"
  sed 's/^/        /' "$SANDBOX/ancestor-metadata.err"
fi

# An ancestor config can alias `cargo deny` to Cargo's `run`, which executes a
# PR-controlled build script. The positive marker demonstrates the alias; the
# direct cargo-deny binary used by the workflow must leave it untouched even
# when the same ancestor config and CARGO_ALIAS_DENY are present.
ALIAS_PROBE="$SANDBOX/alias-probe"
ALIAS_MARKER="$SANDBOX/alias-ran"
mkdir -p "$ALIAS_PROBE/src" "$SANDBOX/alias-cargo-home"
printf '%s\n' '[package]' 'name = "b133-alias-probe"' 'version = "0.1.0"' 'edition = "2021"' > "$ALIAS_PROBE/Cargo.toml"
printf '%s\n' 'fn main() {}' > "$ALIAS_PROBE/src/main.rs"
printf '%s\n' "fn main() { std::fs::write(\"$ALIAS_MARKER\", \"INVOKED\\n\").unwrap(); }" > "$ALIAS_PROBE/build.rs"
printf '[alias]\ndeny = "run --manifest-path %s/Cargo.toml"\n' "$ALIAS_PROBE" > "$SANDBOX/.cargo/config.toml"
(cd "$ALIAS_PROBE" && PATH="${HOST_CARGO%/*}:$PATH" HOME="$SANDBOX/policy-home" CARGO_HOME="$SANDBOX/alias-cargo-home" CARGO_BUILD_RUSTC_WRAPPER='' CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER='' CARGO_BUILD_RUSTFLAGS='' RUSTC_WRAPPER='' RUSTC_WORKSPACE_WRAPPER='' "$HOST_CARGO" deny >"$SANDBOX/alias.out" 2>"$SANDBOX/alias.err")
alias_rc=$?
if [ "$alias_rc" -eq 0 ] && [ -s "$ALIAS_MARKER" ]; then
  pass B133-CELL-26 "ancestor Cargo alias is reproducibly reached by cargo deny"
else
  fail B133-CELL-26 "ancestor Cargo alias is reproducibly reached by cargo deny (rc=$alias_rc)"
  sed 's/^/        /' "$SANDBOX/alias.err"
fi
rm -f "$ALIAS_MARKER"
DIRECT_DENY="$SANDBOX/trusted-cargo-deny"
printf '%s\n' '#!/bin/sh' 'exit 0' > "$DIRECT_DENY"
chmod +x "$DIRECT_DENY"
(cd "$ALIAS_PROBE" && CARGO_ALIAS_DENY="run --manifest-path $ALIAS_PROBE/Cargo.toml" "$DIRECT_DENY" --version >"$SANDBOX/direct-deny.out" 2>"$SANDBOX/direct-deny.err")
direct_deny_rc=$?
if [ "$direct_deny_rc" -eq 0 ] && [ ! -e "$ALIAS_MARKER" ]; then
  pass B133-CELL-27 "direct cargo-deny binary ignores ancestor and environment aliases"
else
  fail B133-CELL-27 "direct cargo-deny binary ignores ancestor and environment aliases (rc=$direct_deny_rc)"
  sed 's/^/        /' "$SANDBOX/direct-deny.err"
fi

# cargo-deny uses cargo-metadata, whose real implementation resolves `$CARGO`
# before falling back to PATH. This marker is the concrete inherited-executable
# exploit; `CARGO=cargo` plus the trusted host-toolchain PATH is the policy job's
# safe form and must neither execute nor retain the marker.
CARGO_ENV_PROBE="$SANDBOX/cargo-env-probe"
CARGO_ENV_HOME="$SANDBOX/cargo-env-home"
CARGO_ENV_DENY="$SANDBOX/cargo-env-deny.toml"
CARGO_ENV_MARKER="$SANDBOX/cargo-env-ran"
CARGO_ENV_WRAPPER="$SANDBOX/cargo-env-wrapper.sh"
mkdir -p "$CARGO_ENV_PROBE/src" "$CARGO_ENV_HOME"
printf '%s\n' '[package]' 'name = "b133-cargo-env-probe"' 'version = "1.0.0"' 'edition = "2021"' > "$CARGO_ENV_PROBE/Cargo.toml"
printf '%s\n' 'fn main() {}' > "$CARGO_ENV_PROBE/src/main.rs"
printf '%s\n' \
  '[sources]' \
  'unknown-registry = "deny"' \
  'unknown-git = "deny"' \
  'allow-registry = ["https://github.com/rust-lang/crates.io-index"]' \
  'allow-git = []' > "$CARGO_ENV_DENY"
printf '%s\n' '#!/bin/sh' "printf 'INVOKED\\n' >> '$CARGO_ENV_MARKER'" "exec '$HOST_CARGO' \"\$@\"" > "$CARGO_ENV_WRAPPER"
chmod +x "$CARGO_ENV_WRAPPER"
# Let Cargo write a structurally valid lockfile before the exploit call. A
# hand-written `version = 4` is not a lock for a root package, and would make
# --locked fail before cargo-deny reaches the CARGO selector we are proving.
(cd "$CARGO_ENV_PROBE" && PATH="${HOST_CARGO%/*}:$PATH" HOME="$SANDBOX/cargo-env-policy-home" CARGO_HOME="$CARGO_ENV_HOME" CARGO=cargo "$HOST_CARGO" generate-lockfile >"$SANDBOX/cargo-env-lock.out" 2>"$SANDBOX/cargo-env-lock.err")
cargo_env_rc=$?
if [ "$cargo_env_rc" -eq 0 ]; then
  (cd "$CARGO_ENV_PROBE" && PATH="${HOST_CARGO%/*}:$PATH" HOME="$SANDBOX/cargo-env-policy-home" CARGO_HOME="$CARGO_ENV_HOME" CARGO="$CARGO_ENV_WRAPPER" "$HOST_CARGO_DENY" --locked check --config "$CARGO_ENV_DENY" sources >"$SANDBOX/cargo-env.out" 2>"$SANDBOX/cargo-env.err")
  cargo_env_rc=$?
fi
if [ "$cargo_env_rc" -eq 0 ] && [ -s "$CARGO_ENV_MARKER" ]; then
  rm -f "$CARGO_ENV_MARKER"
  (cd "$CARGO_ENV_PROBE" && PATH="${HOST_CARGO%/*}:$PATH" HOME="$SANDBOX/cargo-env-policy-home" CARGO_HOME="$CARGO_ENV_HOME" CARGO=cargo "$HOST_CARGO_DENY" --locked check --config "$CARGO_ENV_DENY" sources >"$SANDBOX/cargo-env-safe.out" 2>"$SANDBOX/cargo-env-safe.err")
  cargo_env_safe_rc=$?
  if [ "$cargo_env_safe_rc" -eq 0 ] && [ ! -e "$CARGO_ENV_MARKER" ]; then
    pass B133-CELL-32 "CARGO marker reaches cargo-deny but trusted cargo selector blocks it"
  else
    fail B133-CELL-32 "CARGO marker reaches cargo-deny but trusted cargo selector blocks it (safe rc=$cargo_env_safe_rc)"
    sed 's/^/        /' "$SANDBOX/cargo-env-safe.err"
  fi
else
  fail B133-CELL-32 "CARGO marker reaches cargo-deny (rc=$cargo_env_rc)"
  sed 's/^/        /' "$SANDBOX/cargo-env.err"
fi

echo
prove_cell_census_is_non_vacuous
if ! has_exact_cell_census "$passed_cells"; then
  fail B133-CENSUS "esperava exatamente B133-CELL-01..49 uma vez cada; PASS e IDs únicos divergiram"
fi
if [ "$fails" -eq 0 ]; then
  echo "dependabot-policy trust-boundary teeth: all cells passed"
  exit 0
fi
echo "dependabot-policy trust-boundary teeth: $fails cell(s) failed"
exit 1
