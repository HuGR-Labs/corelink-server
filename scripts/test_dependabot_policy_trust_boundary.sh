#!/usr/bin/env bash
# B-133 focused mutation suite. It is bounded, static, and never invokes Cargo,
# Node, Actions, or code from the untrusted fixture.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
TMP="$(mktemp -d "${TMPDIR:-/tmp}/b133.XXXXXX")"
trap 'rm -rf "$TMP"' EXIT
for f in \
  .github/workflows/dependabot-policy.yml \
  .github/workflows/dependabot-policy-trust-boundary.yml \
  scripts/check_dependabot_policy_trusted_tree.py \
  scripts/test_dependabot_policy_trust_boundary.sh \
  scripts/test_ci_use_host_toolchain.sh \
  scripts/prepare_b133_python.sh \
  scripts/ci-use-host-toolchain.sh \
  rust-toolchain.toml; do
  mkdir -p "$TMP/$(dirname "$f")"
  cp "$ROOT/$f" "$TMP/$f"
done
"$TMP/scripts/prepare_b133_python.sh" "$TMP/.b133-python" >/dev/null
if env -u PYTHONPATH python3 -S -c 'import yaml' >/dev/null 2>&1; then
  echo "FAIL [no-global] python -S unexpectedly imported global PyYAML"
  exit 1
fi
echo "PASS [no-global]"
check() { (cd "$TMP" && B133_TRUSTED_ROOT="$ROOT" "$TMP/.b133-python/python" -S scripts/check_dependabot_policy_trusted_tree.py "$@"); }
check --untrusted-tree "$TMP" >/dev/null

mutate() {
  local id="$1" file="$2" needle="$3" replacement="$4"
  cp "$ROOT/$file" "$TMP/$file"
  python3 - "$TMP/$file" "$needle" "$replacement" <<'PY'
import pathlib, sys
p = pathlib.Path(sys.argv[1])
s = p.read_text()
needle, replacement = sys.argv[2:]
if needle not in s:
    raise SystemExit("mutation needle absent")
p.write_text(s.replace(needle, replacement, 1))
PY
  if check --untrusted-tree "$TMP" >"$TMP/$id.out" 2>&1; then
    echo "FAIL [$id] checker accepted mutation"
    exit 1
  fi
  echo "PASS [$id]"
  cp "$ROOT/$file" "$TMP/$file"
}

mutate event-type .github/workflows/dependabot-policy-trust-boundary.yml \
  "types: [opened, synchronize, reopened]" "types: [opened, synchronize]"
mutate path-filter .github/workflows/dependabot-policy-trust-boundary.yml \
  "      - \"rust-toolchain.toml\"" "      - \"README.md\""
mutate permission .github/workflows/dependabot-policy-trust-boundary.yml \
  "  contents: read" "  contents: write"
mutate runner .github/workflows/dependabot-policy-trust-boundary.yml \
  "    runs-on: ubuntu-24.04" "    runs-on: corelink"
mutate timeout .github/workflows/dependabot-policy-trust-boundary.yml \
  "    timeout-minutes: 10" "    timeout-minutes: 0"
mutate pr-path .github/workflows/dependabot-policy.yml \
  "          path: _pr-data" "          path: ."
mutate base-ref .github/workflows/dependabot-policy.yml \
  '          ref: ${{ github.event.pull_request.base.sha }}' '          ref: ${{ github.event.pull_request.head.sha }}'
mutate history-depth .github/workflows/dependabot-policy.yml \
  "          fetch-depth: 2" "          fetch-depth: 1"
mutate history-head .github/workflows/dependabot-policy.yml \
  '          git -C "$UNTRUSTED_TREE" rev-parse --verify HEAD^2' '          echo history-head-omitted'
mutate diff-mask .github/workflows/dependabot-policy.yml \
  'git -C "$UNTRUSTED_TREE" diff --name-only HEAD^1 HEAD)' 'git -C "$UNTRUSTED_TREE" diff --name-only HEAD^1 HEAD || true)'
mutate base-expression-escape .github/workflows/dependabot-policy-trust-boundary.yml \
  '          ref: ${{ github.event.pull_request.base.sha }}' '          ref: \${{ github.event.pull_request.base.sha }}'
mutate interpreter-expression .github/workflows/dependabot-policy.yml \
  '      B133_PYTHON: ${{ github.workspace }}/.b133-python' '      B133_PYTHON: \${{ github.workspace }}/.b133-python'
mutate pr-expression-malformed .github/workflows/dependabot-policy-trust-boundary.yml \
  '          ref: refs/pull/${{ github.event.pull_request.number }}/merge' '          ref: refs/pull/${{ github.event.pull_request.number }/merge'
mutate workdir .github/workflows/dependabot-policy.yml \
  "        working-directory: _base" "        working-directory: _pr-data"
mutate identity .github/workflows/dependabot-policy.yml \
  "github.event.pull_request.user.login == 'dependabot[bot]'" "github.event.pull_request.user.login != 'dependabot[bot]'"
mutate local-action .github/workflows/dependabot-policy.yml \
  "uses: dependabot/fetch-metadata@25dd0e34f4fe68f24cc83900b1fe3fe149efef98" "uses: ./evil-action"
mutate control-byte scripts/check_dependabot_policy_trusted_tree.py \
  "Fail closed" "Fail open"
mutate interpreter-byte scripts/prepare_b133_python.sh \
  "private directory" "public directory"

rm "$TMP/scripts/ci-use-host-toolchain.sh"
if check --untrusted-tree "$TMP" >/dev/null 2>&1; then
  echo "FAIL [missing-control] checker accepted missing control"
  exit 1
fi
echo "PASS [missing-control]"
ln -s "$ROOT/scripts/ci-use-host-toolchain.sh" "$TMP/scripts/ci-use-host-toolchain.sh"
if check --untrusted-tree "$TMP" >/dev/null 2>&1; then
  echo "FAIL [symlink-control] checker accepted symlink control"
  exit 1
fi
echo "PASS [symlink-control]"
mv "$TMP/.github" "$TMP/.github-real"
ln -s "$TMP/.github-real" "$TMP/.github"
if check --untrusted-tree "$TMP" >/dev/null 2>&1; then
  echo "FAIL [symlink-parent] checker accepted symlink parent"
  exit 1
fi
echo "PASS [symlink-parent]"
echo "B133: focused mutation suite passed"
