"""Adversarial tests for the BASE-owned backlog PR gate."""

from __future__ import annotations

import re
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
import datetime as dt
from pathlib import Path
from unittest.mock import patch

from scripts import backlog_verify


ROOT = Path(__file__).resolve().parents[1]
VERIFIER = ROOT / "scripts" / "backlog_verify.py"


class BacklogVerifyTrustBoundaryTests(unittest.TestCase):
    def test_workflow_uses_base_control_and_credentialless_data_checkouts(self) -> None:
        workflow = (ROOT / ".github" / "workflows" / "backlog-verify.yml").read_text(encoding="utf-8")
        self.assertIn("pull_request_target:", workflow)
        self.assertNotIn("\n  pull_request:\n", workflow)
        self.assertGreaterEqual(workflow.count("persist-credentials: false"), 2)
        self.assertIn("github.event.pull_request.head.sha || github.sha", workflow)
        self.assertIn("github.event.pull_request.base.sha || github.sha", workflow)
        self.assertNotIn("github.event.pull_request.head.ref", workflow)
        self.assertNotIn("github.event.pull_request.base.ref", workflow)
        self.assertEqual(workflow.count("GH_TOKEN:"), 1)
        self.assertIn("vulnerability-alerts: read", workflow)
        self.assertIn("--trusted-semantic --auth-only", workflow)
        self.assertIn("--trusted-semantic --exclude-auth-checks", workflow)
        self.assertIn("path: _candidate", workflow)
        self.assertIn("path: _base", workflow)
        self.assertIn("working-directory: _base", workflow)
        self.assertIn("--candidate-file", workflow)
        self.assertIn("--trusted-file", workflow)
        self.assertIn("--trusted-semantic", workflow)
        self.assertIn("if: github.event_name == 'push' || github.event_name == 'schedule'", workflow)

    @staticmethod
    def item(item_id: str, *, status: str = "open", verify: str = '"true"', owner: str = "tl") -> backlog_verify.Item:
        return backlog_verify.Item(
            raw={
                "id": item_id,
                "repo": "corelink-server",
                "owner": owner,
                "status": status,
                "verify": verify,
                "verify-means": "proof",
                "last-verified": "2026-08-23",
            },
            line=1,
            id=item_id,
        )

    def test_new_id_must_be_open_but_open_implementation_id_is_allowed(self) -> None:
        base = [self.item("B-001")]
        self.assertEqual(
            backlog_verify.validate_candidate_transitions(
                [self.item("B-001"), self.item("B-002")], base, dt.date(2026, 8, 23)
            ),
            [],
        )
        errors = backlog_verify.validate_candidate_transitions(
            [self.item("B-001"), self.item("B-002", status="done")],
            base,
            dt.date(2026, 8, 23),
        )
        self.assertTrue(any("new item B-002 must start status: open" in error for error in errors))

    def test_block_scalar_verify_and_field_status_manipulation_are_rejected(self) -> None:
        base = [self.item("B-001", status="open", verify="python3 scripts/verify.py\n")]
        candidate = [self.item("B-001", status="done", verify="python3 scripts/verify.py\n# mutation\n")]
        errors = backlog_verify.validate_candidate_transitions(
            candidate, base, dt.date(2026, 8, 23)
        )
        self.assertTrue(any("immutable field 'verify' changed" in error for error in errors))
        self.assertFalse(any("status transition" in error for error in errors))
        errors = backlog_verify.validate_candidate_transitions(
            [self.item("B-001", status="open", owner="owner")],
            base,
            dt.date(2026, 8, 23),
        )
        self.assertTrue(any("owner may change" in error for error in errors))

    def test_verify_means_can_change_only_with_status_transition(self) -> None:
        base = self.item("B-373", status="open")
        changed = self.item("B-373", status="done")
        changed.raw["verify-means"] = "done — authenticated post-merge live zero"
        self.assertEqual(
            backlog_verify.validate_candidate_transitions([changed], [base], dt.date(2026, 9, 12)),
            [],
        )
        unchanged_status = self.item("B-373", status="open")
        unchanged_status.raw["verify-means"] = changed.raw["verify-means"]
        errors = backlog_verify.validate_candidate_transitions(
            [unchanged_status], [base], dt.date(2026, 9, 12)
        )
        self.assertTrue(any("verify-means may change only with a status transition" in error for error in errors))
        changed.raw["verify"] = "true"
        errors = backlog_verify.validate_candidate_transitions([changed], [base], dt.date(2026, 9, 12))
        self.assertTrue(any("immutable field 'verify' changed" in error for error in errors))

    def test_deleting_highest_base_id_is_rejected(self) -> None:
        errors = backlog_verify.validate_candidate_transitions(
            [self.item("B-001")],
            [self.item("B-001"), self.item("B-002")],
            dt.date(2026, 8, 23),
        )
        self.assertTrue(any("deleted BASE item(s): B-002" in error for error in errors))

    def test_mutable_workflow_ref_is_rejected_as_data(self) -> None:
        workflow = self.candidate / ".github" / "workflows" / "backlog-verify.yml"
        text = workflow.read_text(encoding="utf-8").replace(
            "github.event.pull_request.head.sha || github.sha",
            "github.event.pull_request.head.ref",
        )
        workflow.write_text(text, encoding="utf-8")
        with self.assertRaisesRegex(RuntimeError, "candidate workflow policy"):
            backlog_verify.validate_candidate_workflow(self.candidate)

    def test_b314_trusted_step_shape_and_placement_are_fail_closed(self) -> None:
        workflow = self.candidate / ".github" / "workflows" / "backlog-verify.yml"
        baseline = workflow.read_text(encoding="utf-8")
        backlog_verify.validate_candidate_workflow(self.candidate)
        b314_marker = "      - name: Prove BASE B-314 owner-gate mutation teeth"
        semantic_marker = "      - name: Execute trusted main semantic checks"
        start = baseline.index(b314_marker)
        end = baseline.index(semantic_marker, start)
        prefix, b314_step, suffix = baseline[:start], baseline[start:end], baseline[end:]
        mutations = {
            "removed": prefix + suffix,
            "wrong-working-directory": prefix + b314_step.replace(
                "working-directory: _base", "working-directory: _candidate", 1
            ) + suffix,
            "weakened-self-test": prefix + b314_step.replace(
                "python3 -S scripts/verify_b314_gdpr_sigstore.py --self-test",
                "python3 -S scripts/verify_b314_gdpr_sigstore.py",
                1,
            ) + suffix,
            "weakened-mutation-test": prefix + b314_step.replace(
                "python3 -m pytest -q tests/test_verify_b314_gdpr_sigstore.py", "true", 1
            ) + suffix,
            "extra-shell-key": prefix + b314_step + "        shell: bash\n" + suffix,
            "reordered": prefix + suffix + b314_step,
        }
        for label, mutated in mutations.items():
            with self.subTest(label=label):
                workflow.write_text(mutated, encoding="utf-8")
                with self.assertRaisesRegex(RuntimeError, "candidate workflow policy"):
                    backlog_verify.validate_candidate_workflow(self.candidate)
        workflow.write_text(baseline, encoding="utf-8")

    def test_workflow_rejects_inherited_and_step_environment_execution(self) -> None:
        workflow = self.candidate / ".github" / "workflows" / "backlog-verify.yml"
        baseline = workflow.read_text(encoding="utf-8")
        backlog_verify.validate_candidate_workflow(self.candidate)
        b314_marker = "      - name: Prove BASE B-314 owner-gate mutation teeth"
        mutations = {
            "root-env-bash-env": baseline.replace(
                "name: backlog-verify\n",
                "name: backlog-verify\nenv:\n  BASH_ENV: ${{ github.workspace }}/_candidate/evil.sh\n", 1,
            ),
            "root-default-shell": baseline.replace(
                "name: backlog-verify\n",
                "name: backlog-verify\ndefaults:\n  run:\n    shell: bash\n", 1,
            ),
            "job-env-bash-env": baseline.replace(
                "    timeout-minutes: 10\n",
                "    timeout-minutes: 10\n    env:\n      BASH_ENV: ${{ github.workspace }}/_candidate/evil.sh\n", 1,
            ),
            "job-defaults": baseline.replace(
                "    timeout-minutes: 10\n",
                "    timeout-minutes: 10\n    defaults:\n      run:\n        working-directory: _candidate\n", 1,
            ),
            "job-container": baseline.replace(
                "    timeout-minutes: 10\n",
                "    timeout-minutes: 10\n    container: attacker-controlled-image\n", 1,
            ),
            "base-gate-env-bash-env": baseline.replace(
                "          TRUSTED_ROOT: ${{ github.workspace }}/_base\n",
                "          TRUSTED_ROOT: ${{ github.workspace }}/_base\n"
                "          BASH_ENV: ${{ github.workspace }}/_candidate/evil.sh\n", 1,
            ),
            "b314-step-env": baseline.replace(
                b314_marker + "\n",
                b314_marker + "\n        env:\n          BASH_ENV: ${{ github.workspace }}/_candidate/evil.sh\n", 1,
            ),
            "extra-pr-trigger": baseline.replace(
                "    paths: [\"**\"]\n  push:\n",
                "    paths: [\"**\"]\n    branches: [main]\n  push:\n", 1,
            ),
        }
        for label, mutated in mutations.items():
            with self.subTest(label=label):
                self.assertNotEqual(mutated, baseline)
                workflow.write_text(mutated, encoding="utf-8")
                with self.assertRaisesRegex(RuntimeError, "candidate workflow policy"):
                    backlog_verify.validate_candidate_workflow(self.candidate)
        workflow.write_text(baseline, encoding="utf-8")

    def test_b046_trusted_step_shape_and_placement_are_fail_closed(self) -> None:
        workflow = self.candidate / ".github" / "workflows" / "backlog-verify.yml"
        baseline = workflow.read_text(encoding="utf-8")
        backlog_verify.validate_candidate_workflow(self.candidate)
        marker = "      - name: Execute B-046 Object-Lock contract and mutation checks"
        b046_start = baseline.index(marker)
        prefix, b046_step = baseline[:b046_start], baseline[b046_start:]
        auth_marker = "      - name: Execute authenticated Dependabot semantic checks"
        auth_start = prefix.index(auth_marker)
        prefix_without_auth = prefix[:auth_start]
        auth_step = prefix[auth_start:]
        mutations = {
            "removed": prefix.rstrip() + "\n",
            "extra-if": prefix + b046_step.replace(
                "        working-directory: _base", "        if: github.event_name == 'workflow_dispatch'\n        working-directory: _base", 1),
            "wrong-working-directory": prefix + b046_step.replace(
                "working-directory: _base", "working-directory: _candidate", 1
            ),
            "reordered": prefix_without_auth + b046_step + auth_step,
        }
        for label, mutated in mutations.items():
            with self.subTest(label=label):
                workflow.write_text(mutated, encoding="utf-8")
                with self.assertRaisesRegex(RuntimeError, "candidate workflow policy"):
                    backlog_verify.validate_candidate_workflow(self.candidate)
        workflow.write_text(baseline, encoding="utf-8")

    def test_alert_token_is_confined_to_exact_trusted_verifiers(self) -> None:
        result = subprocess.CompletedProcess([], 0, "ok", "")
        commands = backlog_verify.AUTH_VERIFY_COMMANDS
        with patch.dict(os.environ, {
            "GH_TOKEN": "test-only-alert-token", "GITHUB_TOKEN": "other-token",
            "ACTIONS_RUNTIME_TOKEN": "runtime-token", "BASH_ENV": "/tmp/evil",
        }), patch.object(backlog_verify.subprocess, "run", return_value=result) as run:
            for item_id, command in commands.items():
                backlog_verify.run_verify(command, mode="trusted", item_id=item_id)
                env = run.call_args.kwargs["env"]
                self.assertEqual(env.get("GH_TOKEN"), "test-only-alert-token")
                self.assertNotIn("GITHUB_TOKEN", env)
                self.assertNotIn("ACTIONS_RUNTIME_TOKEN", env)
                self.assertNotIn("BASH_ENV", env)
            for item_id, command in (
                ("B-001", "true"),
                ("B-028", commands["B-028"] + " && env"),
                ("B-373", "true"),
            ):
                backlog_verify.run_verify(command, mode="trusted", item_id=item_id)
                self.assertNotIn("GH_TOKEN", run.call_args.kwargs["env"])
            count = run.call_count
            backlog_verify.run_verify(commands["B-028"], mode="candidate", item_id="B-028")
            self.assertEqual(run.call_count, count)

    def test_split_semantic_modes_reject_single_id_shortcut(self) -> None:
        for mode in ("--auth-only", "--exclude-auth-checks"):
            with self.subTest(mode=mode):
                result = subprocess.run(
                    [sys.executable, str(VERIFIER), "--trusted-semantic", mode, "--id", "B-028"],
                    cwd=ROOT, capture_output=True, text=True,
                    env={**os.environ, "GITHUB_EVENT_NAME": "push"}, check=False,
                )
                self.assertEqual(result.returncode, 2)
                self.assertIn("full partition", result.stderr)

    def test_workflow_rejects_token_grants_or_candidate_exposure(self) -> None:
        workflow = self.candidate / ".github" / "workflows" / "backlog-verify.yml"
        baseline = workflow.read_text(encoding="utf-8")
        mutations = {
            "workflow-scope-alerts": baseline.replace(
                "permissions:\n  contents: read\n", "permissions:\n  contents: read\n  vulnerability-alerts: read\n", 1),
            "pr-step-token": baseline.replace(
                "          TRUSTED_ROOT: ${{ github.workspace }}/_base\n",
                "          TRUSTED_ROOT: ${{ github.workspace }}/_base\n          GH_TOKEN: ${{ github.token }}\n", 1),
            "trusted-candidate-checkout": baseline.replace(
                "      - name: Execute trusted main semantic checks\n",
                "      - name: Checkout candidate again\n        run: true\n"
                "      - name: Execute trusted main semantic checks\n", 1),
            "extra-token-env": baseline.replace(
                "      - name: Execute trusted main semantic checks\n",
                "      - name: Execute trusted main semantic checks\n        env:\n          GH_TOKEN: ${{ github.token }}\n", 1),
            "wrong-auth-command": baseline.replace(
                "--trusted-semantic --auth-only", "--trusted-semantic", 1),
            "credential-persistence": baseline.replace(
                "          fetch-depth: 0\n          persist-credentials: false",
                "          fetch-depth: 0\n          persist-credentials: true", 1),
        }
        for label, mutated in mutations.items():
            with self.subTest(label=label):
                self.assertNotEqual(mutated, baseline)
                workflow.write_text(mutated, encoding="utf-8")
                with self.assertRaisesRegex(RuntimeError, "candidate workflow policy"):
                    backlog_verify.validate_candidate_workflow(self.candidate)
        workflow.write_text(baseline, encoding="utf-8")

    def test_transitive_control_mutation_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            trusted = Path(directory) / "trusted"
            candidate = Path(directory) / "candidate"
            (trusted / "scripts").mkdir(parents=True)
            (candidate / "scripts").mkdir(parents=True)
            shutil.copy2(ROOT / "scripts" / "backlog_verify.py", trusted / "scripts" / "backlog_verify.py")
            shutil.copy2(ROOT / "scripts" / "backlog_verify.py", candidate / "scripts" / "backlog_verify.py")
            (trusted / "scripts" / "check.py").write_text("from scripts import helper\n", encoding="utf-8")
            (trusted / "scripts" / "helper.py").write_text("VALUE = 1\n", encoding="utf-8")
            for name in ("check.py", "helper.py"):
                shutil.copy2(trusted / "scripts" / name, candidate / "scripts" / name)
            items = [self.item("B-001", verify="python3 scripts/check.py")]
            candidate.joinpath("scripts/helper.py").write_text("VALUE = 2\n", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "helper.py"):
                backlog_verify.check_candidate_controls(candidate, trusted, items)

    def test_dynamic_import_control_mutation_is_rejected(self) -> None:
        trusted_items = backlog_verify.parse((ROOT / "BACKLOG.md").read_text(encoding="utf-8"))
        controls = backlog_verify._candidate_control_paths(ROOT, trusted_items)
        self.assertIn("scripts/b155_backlog_grep_parser.py", controls)
        parser = self.candidate / "scripts" / "b155_backlog_grep_parser.py"
        parser.write_text(parser.read_text(encoding="utf-8") + "\n# candidate mutation\n", encoding="utf-8")

        result = self.run_gate()

        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("b155_backlog_grep_parser.py", result.stdout + result.stderr)

    def test_trusted_semantic_mode_reproduces_stale_b001(self) -> None:
        result = subprocess.run(
            [sys.executable, str(VERIFIER), "--trusted-semantic", "--id", "B-001", "--today", "2026-09-08"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            env={**os.environ, "GITHUB_EVENT_NAME": "push"},
            check=False,
        )
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("STALE", result.stdout + result.stderr)

    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.candidate = Path(self.temp.name) / "candidate"
        self.candidate.mkdir()
        shutil.copy2(ROOT / "BACKLOG.md", self.candidate / "BACKLOG.md")
        trusted_items = backlog_verify.parse((ROOT / "BACKLOG.md").read_text(encoding="utf-8"))
        controls = backlog_verify._candidate_control_paths(ROOT, trusted_items)
        for relative in controls:
            source = ROOT / relative
            target = self.candidate / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)
        workflow = self.candidate / ".github" / "workflows" / "backlog-verify.yml"
        workflow.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / ".github" / "workflows" / "backlog-verify.yml", workflow)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def run_gate(self) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [
                sys.executable,
                str(VERIFIER),
                "--candidate-file",
                str(self.candidate / "BACKLOG.md"),
                "--trusted-file",
                str(ROOT / "BACKLOG.md"),
                "--candidate-root",
                str(self.candidate),
                "--trusted-root",
                str(ROOT),
                "--today",
                "2026-09-08",
                "--id",
                "B-044",
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=False,
        )

    def test_mutated_candidate_verify_string_cannot_execute_or_green(self) -> None:
        text = (self.candidate / "BACKLOG.md").read_text(encoding="utf-8")
        marker = Path(self.temp.name) / "candidate-verify-executed"
        payload = (
            'python3 -c "from pathlib import Path; '
            f"Path({str(marker)!r}).write_text('pwned')"
            '"'
        )
        mutated, count = re.subn(
            r"(### B-044\b.*?^verify:) .*?$",
            rf"\1 {payload}",
            text,
            count=1,
            flags=re.MULTILINE | re.DOTALL,
        )
        self.assertEqual(count, 1)
        (self.candidate / "BACKLOG.md").write_text(mutated, encoding="utf-8")

        result = self.run_gate()

        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertFalse(marker.exists(), "candidate verify payload was executed")
        self.assertIn("immutable field 'verify' changed", result.stdout + result.stderr)

    def test_mutated_candidate_verifier_cannot_green(self) -> None:
        verifier = self.candidate / "scripts" / "verify_b044_orphan_teardown_wp.py"
        self.assertTrue(verifier.is_file())
        verifier.write_text(verifier.read_text(encoding="utf-8") + "\n# candidate mutation\n", encoding="utf-8")

        result = self.run_gate()

        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("mutated trusted backlog control", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
