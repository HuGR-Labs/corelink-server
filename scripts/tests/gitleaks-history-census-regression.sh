#!/usr/bin/env bash
# Mutation contract for gitleaks-shape-history-census.sh.
#
# The census is a security guard, not a best-effort report. This harness builds
# a small synthetic history and uses a fake scanner only to exercise graph and
# report fail-closed branches without rescanning this repository on every PR.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/corelink-gitleaks-census-regression.XXXXXX")"
trap 'rm -rf -- "$tmp"' EXIT

python3 - "$root/scripts/tests/gitleaks-shape-history-census.sh" "$root/.gitleaks.toml" "$tmp" <<'PY'
import io
import os
import pathlib
import shutil
import subprocess
import sys

census_src = pathlib.Path(sys.argv[1])
config_src = pathlib.Path(sys.argv[2])
root = pathlib.Path(sys.argv[3])
known = "reports/audits/2026-08-25-comprehensive-audit-and-verification.md"


def git(repo, *args):
    return subprocess.run(["git", *args], cwd=repo, check=True, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def install(repo, mutate=None):
    dst = repo / "scripts/tests/gitleaks-shape-history-census.sh"
    text = census_src.read_text(encoding="utf-8")
    if mutate:
        text = mutate(text)
    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text(text, encoding="utf-8")
    dst.chmod(0o755)
    shutil.copy2(config_src, repo / ".gitleaks.toml")


def make_repo(repo, count=101, mutate=None):
    repo.mkdir()
    git(repo, "init", "-q")
    install(repo, mutate)
    script = census_src.read_bytes()
    config = config_src.read_bytes()
    known_data = ('vars = { ..., AUDIT_CHAIN_SIGNING_SEED_HEX = "' + "a" * 64 + '" }\n').encode()
    stream = io.BytesIO()

    def line(value):
        stream.write(value.encode() + b"\n")

    def blob(path, data, mode="100644"):
        line(f"M {mode} inline {path}")
        line(f"data {len(data)}")
        stream.write(data + b"\n")

    for mark in range(1, count + 1):
        line("commit refs/heads/master")
        line(f"mark :{mark}")
        line("author Census Fixture <census@example.invalid> 0 +0000")
        line("committer Census Fixture <census@example.invalid> 0 +0000")
        message = "known historical positive" if mark == 1 else f"history {mark}"
        payload = message.encode()
        line(f"data {len(payload)}")
        stream.write(payload + b"\n")
        if mark > 1:
            line(f"from :{mark - 1}")
        if mark == 1:
            blob(".gitleaks.toml", config)
            blob("scripts/tests/gitleaks-shape-history-census.sh", script, "100755")
            blob(known, known_data)
        else:
            blob("history-marker.txt", f"commit {mark}\n".encode())
    subprocess.run(["git", "fast-import"], cwd=repo, input=stream.getvalue(), check=True,
                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    git(repo, "checkout", "-q", "-f", "master")
    return repo


fakebin = root / "fake-bin"
fakebin.mkdir()
(fakebin / "gitleaks").write_text(
    "#!/usr/bin/env python3\n"
    "import json, os, sys\n"
    "report = sys.argv[sys.argv.index('--report-path') + 1]\n"
    "mode = os.environ.get('FAKE_GITLEAKS_MODE', 'known')\n"
    "if mode == 'missing': sys.exit(1)\n"
    "if mode == 'malformed': open(report, 'w').write('{'); sys.exit(1)\n"
    "if mode == 'empty': json.dump([], open(report, 'w')); sys.exit(0)\n"
    "rule = 'other-rule' if mode == 'other-rule' else 'corelink-secret-shaped-hex'\n"
    "json.dump([{'RuleID': rule, 'File': '" + known + "', 'Commit': 'fixture'}], open(report, 'w'))\n"
    "sys.exit(2 if mode == 'unexpected' else 1)\n",
    encoding="utf-8",
)
(fakebin / "gitleaks").chmod(0o755)


def run(repo, mode="known", script="scripts/tests/gitleaks-shape-history-census.sh", target="HEAD"):
    env = os.environ.copy()
    env["PATH"] = str(fakebin) + os.pathsep + env["PATH"]
    env["FAKE_GITLEAKS_MODE"] = mode
    return subprocess.run([str(repo / script), target], cwd=repo, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)


def expect_failure(name, result, marker):
    output = result.stdout + result.stderr
    if result.returncode == 0 or marker.lower() not in output.lower():
        raise SystemExit(f"{name} unexpectedly passed: rc={result.returncode} {output}")
    print(f"PASS [{name}]")


full = make_repo(root / "full")
if run(full).returncode != 0:
    raise SystemExit("baseline census did not pass")
print("PASS [complete history baseline]")

shallow = root / "shallow"
git(shallow.parent, "clone", "-q", "--depth=2", f"file://{full}", str(shallow))
install(shallow)
expect_failure("shallow graph", run(shallow), "shallow")

replacement = make_repo(root / "replacement")
git(replacement, "replace", "HEAD", "HEAD^")
expect_failure("replace refs", run(replacement), "replace")

for mode, marker in (("missing", "missing or empty"), ("malformed", "unreadable"),
                     ("empty", "known historical positive"),
                     ("other-rule", "outside corelink-secret-shaped-hex"),
                     ("unexpected", "unexpectedly")):
    expect_failure(f"scanner {mode}", run(full, mode), marker)

# Remove the graph guard and population floor: a deliberately shallow clone
# would then reach the fake scanner, so the mutation contract must catch it.
def remove_guards(text):
    lines = text.splitlines(keepends=True)
    text = "".join(
        line for line in lines
        if "rev-parse --is-shallow-repository" not in line
        and 'fail "repository is shallow"' not in line
        and 'shallow boundary file' not in line
    )
    return text.replace('[[ "$commit_count" -ge 100 ]]', '[[ "$commit_count" -ge 0 ]]')


small = root / "population-mutation"
make_repo(small, count=2)
mutated = census_src.read_text(encoding="utf-8")
mutated = remove_guards(mutated)
if mutated == census_src.read_text(encoding="utf-8"):
    raise SystemExit("population/graph guard mutation was not applied")
expect_failure("population floor", run(small), "minimum 100")
print("PASS [population and graph guard mutation is observable]")

print("gitleaks history census mutations: PASS")
PY
