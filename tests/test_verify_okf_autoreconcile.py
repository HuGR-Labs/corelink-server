from __future__ import annotations

import importlib.util
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_okf_autoreconcile", ROOT / "scripts/verify_okf_autoreconcile.py"
)
assert SPEC and SPEC.loader
module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(module)


class OkfAutoreconcileContractTests(unittest.TestCase):
    @staticmethod
    def _run_block(text: str, step_name: str) -> str:
        """Extract one YAML run block as the fresh shell Actions uses per step."""
        marker = f"      - name: {step_name}\n"
        start = text.index(marker) + len(marker)
        next_step = text.find("\n      - name:", start)
        section = text[start:] if next_step < 0 else text[start:next_step]
        run_marker = "        run: |\n"
        run_start = section.index(run_marker) + len(run_marker)
        lines = []
        for line in section[run_start:].splitlines():
            if line and not line.startswith("          "):
                break
            lines.append(line[10:] if line else "")
        return "\n".join(lines) + "\n"

    def test_current_workflow_and_mutations(self) -> None:
        text = module.read_workflow()
        module.validate(text)
        module.self_test(text)

    def test_pull_request_mutation_is_red(self) -> None:
        text = module.read_workflow()
        with self.assertRaises(module.ContractError):
            module.validate(text.replace("workflow_dispatch:", "pull_request:", 1))

    def test_state_helper_survives_fresh_step_shells_and_missing_function_is_red(self) -> None:
        text = module.read_workflow()
        blocks = [
            self._run_block(text, "Snapshot git state before Codex (fail-closed baseline)"),
            self._run_block(text, "Verify Codex did not mutate git state (fail-closed)"),
        ]
        helper = "scripts/okf-state-sha.sh"
        for block in blocks:
            self.assertIn(helper, block)
            self.assertNotRegex(block, r"(?m)^\s*state_sha\s*\(\)")

        # Execute both extracted blocks in independent bash processes, exactly
        # as Actions does for separate `run:` steps.  Fake git supplies only
        # the read-only state queries; the state files are local fixtures.
        with tempfile.TemporaryDirectory() as temp:
            env = os.environ.copy()
            env["RUNNER_TEMP"] = temp
            fake_bin = Path(temp) / "bin"
            fake_bin.mkdir()
            fake_git = fake_bin / "git"
            fake_git.write_text(
                """#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  "rev-parse HEAD") echo deadbeefdeadbeefdeadbeefdeadbeefdeadbeef ;;
  "symbolic-ref --quiet --short HEAD") echo main ;;
  "for-each-ref --format=%(refname) %(objectname)") echo 'refs/heads/main deadbeef' ;;
  "remote -v") echo 'origin https://example.invalid/repo (fetch)' ;;
  "worktree list --porcelain") echo 'worktree /workspace' ;;
  *) echo "unexpected fake git invocation: $*" >&2; exit 99 ;;
esac
""",
                encoding="utf-8",
            )
            fake_git.chmod(0o755)
            env["PATH"] = f"{fake_bin}:{env['PATH']}"
            env["GITHUB_OUTPUT"] = str(Path(temp) / "output")
            env["GITHUB_ENV"] = str(Path(temp) / "env")
            Path(temp, "okf-agent-guard").mkdir()
            Path(temp, "okf-agent-guard/mutation.log").write_text("", encoding="utf-8")

            snapshot = blocks[0]
            post = blocks[1].replace(
                "${{ steps.snapshot.outputs.state_sha }}",
                '$(scripts/okf-state-sha.sh "$RUNNER_TEMP/okf_state_after")',
            )
            for block in (snapshot, post):
                result = subprocess.run(
                    ["bash", "-euo", "pipefail", "-c", block],
                    cwd=ROOT,
                    env=env,
                    text=True,
                    capture_output=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)

            # A regression to the former step-local function must fail closed,
            # and must be observable as command-not-found (127), not accepted.
            mutant = post.replace(helper, "state_sha")
            result = subprocess.run(
                ["bash", "-euo", "pipefail", "-c", mutant],
                cwd=ROOT,
                env=env,
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 127, result.stderr)

    def test_state_helper_macos_fallback_is_hash_only_and_filename_independent(self) -> None:
        """Exercise shasum fallback without allowing sha256sum into PATH."""
        fixed_hash = "0123456789abcdef" * 4
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            fake_bin = root / "bin"
            fake_bin.mkdir()
            fake_shasum = fake_bin / "shasum"
            fake_shasum.write_text(
                """#!/bin/sh
test "$1" = "-a" && test "$2" = "256" || exit 2
printf '%s  %s\\n' '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef' "$3"
""",
                encoding="utf-8",
            )
            fake_shasum.chmod(0o755)
            fake_awk = fake_bin / "awk"
            fake_awk.write_text(
                """#!/bin/sh
IFS= read -r line
printf '%s\\n' "${line%% *}"
""",
                encoding="utf-8",
            )
            fake_awk.chmod(0o755)
            before = root / "okf_state_before"
            after = root / "okf_state_after"
            before.write_text("same state\n", encoding="utf-8")
            after.write_text("same state\n", encoding="utf-8")
            env = {"PATH": str(fake_bin)}
            outputs = []
            for state_file in (before, after):
                result = subprocess.run(
                    ["/bin/bash", str(ROOT / "scripts/okf-state-sha.sh"), str(state_file)],
                    cwd=ROOT,
                    env=env,
                    text=True,
                    capture_output=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                outputs.append(result.stdout)
            self.assertEqual(outputs, [f"{fixed_hash}\n", f"{fixed_hash}\n"])
            self.assertNotIn(str(before), outputs[0])
            self.assertNotIn(str(after), outputs[1])


if __name__ == "__main__":
    unittest.main()
