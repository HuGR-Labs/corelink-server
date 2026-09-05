#!/usr/bin/env python3
"""Behavioral test for the alert promtool wrapper.

The fake executable is the oracle: source text which merely mentions promtool
must not satisfy this test, because it cannot produce the two required calls.
"""

from __future__ import annotations

import inspect
import json
import os
import stat
import subprocess
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
WRAPPER = ROOT / "scripts" / "validate_alert_promtool.sh"


def fail(message: str) -> None:
    raise SystemExit(f"FALHA: {message}")


def assert_mutation_failed(name: str, result: subprocess.CompletedProcess[str], calls: list[list[str]], reason: str) -> None:
    """Require a decoy to fail without invoking the canonical fake calls."""
    if calls:
        fail(f"{name} mutation invoked fake promtool")
    assert result.returncode != 0, f"{name} mutation unexpectedly passed: {reason}"


def fake_source() -> str:
    return '''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path

log = Path(os.environ["PROMTOOL_LOG"])
calls = []
if log.exists():
    calls = [json.loads(line) for line in log.read_text().splitlines() if line]
args = sys.argv[1:]
calls.append(args)
log.write_text("".join(json.dumps(call) + "\\n" for call in calls))
files = os.environ["PROMTOOL_EXPECTED_FILES"].splitlines()
if len(calls) == 1:
    if args != ["--version"]:
        raise SystemExit(31)
elif len(calls) == 2:
    if args != ["check", "rules"] + files:
        raise SystemExit(32)
else:
    raise SystemExit(33)
'''


def run(command: Path, log: Path, fake_dir: Path, expected_files: list[str]) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["PATH"] = str(fake_dir) + os.pathsep + env.get("PATH", "")
    env["PROMTOOL_LOG"] = str(log)
    env["PROMTOOL_EXPECTED_FILES"] = "\n".join(expected_files)
    return subprocess.run(
        [str(command)], cwd=ROOT, env=env, text=True,
        capture_output=True, timeout=30,
    )


def main() -> int:
    if not WRAPPER.is_file() or not os.access(WRAPPER, os.X_OK):
        fail("validate_alert_promtool.sh is missing or not executable")
    files = sorted(str(path.relative_to(ROOT)) for path in (ROOT / "dashboards" / "alerts").glob("*.yml"))
    if not files:
        fail("the fixture has no alert rule files")
    source = WRAPPER.read_text()
    original = 'promtool --version\npromtool check rules "${files[@]}"'
    if source.count(original) != 1:
        fail("wrapper no longer has its two canonical calls")

    mutations = {
        # `false` makes the decoy's failed validation observable as a non-zero
        # process result; without it, calls==[] with rc0 is a false green.
        "echo": 'echo "promtool --version"\necho "promtool check rules dashboards/alerts/*.yml"\nfalse',
        "printf": 'printf "%s\\n" "promtool --version"\nprintf "%s\\n" "promtool check rules dashboards/alerts/*.yml"\nfalse',
        "string": 'mention="promtool --version"\nprintf "%s\\n" "$mention"\nmention="promtool check rules dashboards/alerts/*.yml"\nprintf "%s\\n" "$mention"\nfalse',
        "heredoc": "cat <<'EOF'\npromtool --version\nEOF\ncat <<'EOF'\npromtool check rules dashboards/alerts/*.yml\nEOF\nfalse",
    }
    expected_failure_reason = {
        "echo": "echo only prints the command text",
        "printf": "printf only prints the command text",
        "string": "a shell string only mentions the command text",
        "heredoc": "a heredoc only contains command text",
    }
    if set(mutations) != set(expected_failure_reason):
        fail("mutation table and expected_failure_reason table diverged")

    with tempfile.TemporaryDirectory(prefix="alert-promtool-test-") as directory:
        temp = Path(directory)
        fake_dir = temp / "bin"
        fake_dir.mkdir()
        fake = fake_dir / "promtool"
        fake.write_text(fake_source())
        fake.chmod(fake.stat().st_mode | stat.S_IXUSR)

        log = temp / "calls.jsonl"
        result = run(WRAPPER, log, fake_dir, files)
        if result.returncode != 0:
            fail("canonical wrapper failed: " + (result.stdout + result.stderr).strip()[-300:])
        calls = [json.loads(line) for line in log.read_text().splitlines() if line]
        if calls != [["--version"], ["check", "rules", *files]]:
            fail(f"fake saw unexpected calls: {calls!r}")

        for name, replacement in mutations.items():
            mutant = temp / f"mutant-{name}.sh"
            mutant.write_text(source.replace(original, replacement))
            mutant.chmod(mutant.stat().st_mode | stat.S_IXUSR)
            mutation_log = temp / f"{name}.jsonl"
            result = run(mutant, mutation_log, fake_dir, files)
            calls = ([json.loads(line) for line in mutation_log.read_text().splitlines() if line]
                     if mutation_log.exists() else [])
            assert_mutation_failed(name, result, calls, expected_failure_reason[name])

        # Directly exercise the load-bearing assertion with a synthetic rc0
        # result. This must raise; the meta-mutation below removes that assert
        # and proves the weakened test fails this same check.
        try:
            assert_mutation_failed(
                "synthetic-rc0", subprocess.CompletedProcess([], 0), [],
                "zero exit is not an observed validation failure",
            )
        except AssertionError:
            pass
        else:
            fail("rc0 mutation was not rejected by the assertion")

        # Meta-mutation: remove the required rc assertion from the checker and
        # run that weakened function against rc0. The source-level guard check
        # below must detect the weakened checker rather than accept a green.
        checker_source = inspect.getsource(assert_mutation_failed)
        guard = '    assert result.returncode != 0, f"{name} mutation unexpectedly passed: {reason}"\n'
        if guard not in checker_source:
            fail("rc!=0 assertion disappeared before meta-mutation")
        weakened = checker_source.replace(guard, "    pass\n", 1)
        if weakened == checker_source:
            fail("meta-mutation guard target disappeared")
        namespace: dict[str, object] = {}
        exec(weakened, globals(), namespace)
        try:
            namespace["assert_mutation_failed"](
                "meta-rc0", subprocess.CompletedProcess([], 0), [],
                "zero exit is not an observed validation failure",
            )
        except AssertionError:
            fail("meta-mutation did not remove the rc!=0 assertion")
        try:
            assert guard in weakened, "removed rc!=0 assertion was not detected"
        except AssertionError:
            pass
        else:
            fail("meta-mutation removing rc!=0 assertion was accepted")

    print("ok: canonical promtool wrapper invokes exactly --version and check rules; decoys fail with rc!=0; meta-mutation red")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
