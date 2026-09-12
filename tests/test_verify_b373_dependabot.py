#!/usr/bin/env python3
"""Focused tests for the B-373 candidate/post-merge source boundary."""

from __future__ import annotations

import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_b373", ROOT / "scripts/verify_b373_dependabot.py"
)
VERIFY = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(VERIFY)


class B373SourceBoundaryTests(unittest.TestCase):
    def test_post_merge_rejects_stdin_and_arbitrary_fixture_before_read(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json") as fixture:
            fixture.write("[]\n")
            fixture.flush()
            for path in (Path("/dev/stdin"), Path(fixture.name)):
                with self.subTest(path=path), patch.object(VERIFY, "read_alerts") as read:
                    result = VERIFY.main(
                        [
                            "--post-merge",
                            "--merged-sha",
                            "704218c5050e99218fa250fc1ff087a2aebd9994",
                            "--alerts-file",
                            str(path),
                        ]
                    )
                    self.assertEqual(result, 2)
                    read.assert_not_called()

    def test_done_fixture_also_requires_authenticated_live_zero(self):
        arguments = ["--alerts-file", str(ROOT / VERIFY.SNAPSHOT)]
        with patch.object(VERIFY, "backlog_b373_done", return_value=True), patch.object(
            VERIFY, "verify_delivered_main_for_candidate"
        ) as delivered, patch.object(
            VERIFY, "read_alerts", return_value=[]
        ) as live:
            self.assertEqual(VERIFY.main(arguments), 0)
            delivered.assert_called_once_with(ROOT)
            live.assert_called_once_with(VERIFY.REPO, None)

    def test_open_state_preserves_historical_candidate_contract(self):
        arguments = ["--alerts-file", str(ROOT / VERIFY.SNAPSHOT)]
        with patch.object(VERIFY, "backlog_b373_done", return_value=False), patch.object(
            VERIFY, "verify_delivered_main_for_candidate"
        ) as delivered, patch.object(VERIFY, "read_alerts") as live:
            self.assertEqual(VERIFY.main(arguments), 0)
            delivered.assert_not_called()
            live.assert_not_called()

    def test_explicit_post_merge_still_rejects_descendant_candidate_tree(self):
        with patch.object(VERIFY, "read_alerts", return_value=[]):
            self.assertEqual(VERIFY.main([
                "--post-merge", "--merged-sha", VERIFY.DELIVERED_MERGE_SHA,
            ]), 2)

    def test_done_candidate_accepts_only_exact_delivered_main_and_ancestry(self):
        main_sha = "1" * 40
        ok = subprocess.CompletedProcess([], 0, "", "")
        with patch.object(VERIFY.subprocess, "run", side_effect=[ok, ok, ok]) as run, patch.object(
            VERIFY, "git", side_effect=["false", main_sha, f"{main_sha}\trefs/heads/main"]
        ) as git:
            VERIFY.verify_delivered_main_for_candidate(ROOT)
            self.assertEqual(run.call_count, 3)
            self.assertEqual(run.call_args_list[0].args[0],
                             ["git", "fetch", "--no-tags", "origin", "main"])
            self.assertEqual(run.call_args_list[1].args[0],
                             ["git", "merge-base", "--is-ancestor",
                              VERIFY.DELIVERED_MERGE_SHA, main_sha])
            self.assertEqual(run.call_args_list[2].args[0],
                             ["git", "merge-base", "--is-ancestor",
                              VERIFY.DELIVERED_MERGE_SHA, "HEAD"])
            self.assertEqual(git.call_count, 3)

    def test_done_candidate_rejects_stale_or_wrong_main_before_live_api(self):
        arguments = ["--alerts-file", str(ROOT / VERIFY.SNAPSHOT)]
        failed = subprocess.CompletedProcess([], 1, "", "network unavailable")
        with patch.object(VERIFY, "backlog_b373_done", return_value=True), patch.object(
            VERIFY.subprocess, "run", return_value=failed
        ) as run, patch.object(
            VERIFY, "read_alerts"
        ) as live:
            self.assertEqual(VERIFY.main(arguments), 2)
            run.assert_called_once()
            live.assert_not_called()
        main_sha = "1" * 40
        ok = subprocess.CompletedProcess([], 0, "", "")
        with patch.object(VERIFY, "backlog_b373_done", return_value=True), patch.object(
            VERIFY.subprocess, "run", return_value=ok
        ) as run, patch.object(
            VERIFY, "git", side_effect=["false", main_sha, f"{'0' * 40}\trefs/heads/main"]
        ), patch.object(VERIFY, "read_alerts") as live:
            self.assertEqual(VERIFY.main(arguments), 2)
            run.assert_called_once()
            live.assert_not_called()

    def test_done_candidate_rejects_non_descendant_before_live_api(self):
        arguments = ["--alerts-file", str(ROOT / VERIFY.SNAPSHOT)]
        main_sha = "1" * 40
        ok = subprocess.CompletedProcess([], 0, "", "")
        unrelated = subprocess.CompletedProcess([], 1, "", "")
        for results in ([ok, unrelated], [ok, ok, unrelated]):
            with self.subTest(results=results), patch.object(
                VERIFY, "backlog_b373_done", return_value=True
            ), patch.object(
                VERIFY.subprocess, "run", side_effect=results
            ), patch.object(
                VERIFY, "git", side_effect=["false", main_sha, f"{main_sha}\trefs/heads/main"]
            ), patch.object(VERIFY, "read_alerts") as live:
                self.assertEqual(VERIFY.main(arguments), 2)
                live.assert_not_called()

    def test_real_shallow_trusted_checkout_recovers_delivered_ancestry(self):
        def run(*args: str) -> str:
            result = subprocess.run(args, check=True, capture_output=True, text=True)
            return result.stdout.strip()

        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            remote = base / "remote.git"
            source = base / "source"
            shallow = base / "shallow"
            run("git", "init", "--bare", str(remote))
            run("git", "init", str(source))
            run("git", "-C", str(source), "config", "user.name", "B373 Test")
            run("git", "-C", str(source), "config", "user.email", "b373@example.test")
            for message in ("root", "delivered", "later"):
                run("git", "-C", str(source), "-c", "commit.gpgsign=false",
                    "commit", "--allow-empty", "-m", message)
                if message == "delivered":
                    delivered = run("git", "-C", str(source), "rev-parse", "HEAD")
            run("git", "-C", str(source), "branch", "-M", "main")
            run("git", "-C", str(source), "remote", "add", "origin", str(remote))
            run("git", "-C", str(source), "push", "origin", "main")
            run("git", "--git-dir", str(remote), "symbolic-ref", "HEAD", "refs/heads/main")
            run("git", "clone", "--depth=1", remote.as_uri(), str(shallow))
            self.assertEqual(run("git", "-C", str(shallow), "rev-parse",
                                 "--is-shallow-repository"), "true")
            with patch.object(VERIFY, "DELIVERED_MERGE_SHA", delivered):
                VERIFY.verify_delivered_main_for_candidate(shallow)
            self.assertEqual(run("git", "-C", str(shallow), "rev-parse",
                                 "--is-shallow-repository"), "false")

    def test_shallow_history_fetch_failure_is_not_zero(self):
        ok = subprocess.CompletedProcess([], 0, "", "")
        failed = subprocess.CompletedProcess([], 1, "", "fetch denied")
        with patch.object(VERIFY.subprocess, "run", side_effect=[ok, failed]) as fetch, patch.object(
            VERIFY, "git", return_value="true"
        ):
            with self.assertRaisesRegex(VERIFY.CensusError, "could not complete main history"):
                VERIFY.verify_delivered_main_for_candidate(ROOT)
            self.assertEqual(fetch.call_count, 2)

    def test_done_fixture_rejects_live_open_alerts(self):
        arguments = ["--alerts-file", str(ROOT / VERIFY.SNAPSHOT)]
        open_alert = {
            "state": "open", "number": 39,
            "dependency": {"package": {"name": "vitest"}},
            "security_advisory": {
                "ghsa_id": "GHSA-82fw-gwwq-j7x9", "severity": "medium",
                "vulnerabilities": [{}],
            },
        }
        with patch.object(VERIFY, "backlog_b373_done", return_value=True), patch.object(
            VERIFY, "verify_delivered_main_for_candidate"
        ), patch.object(
            VERIFY, "read_alerts", return_value=[open_alert]
        ):
            self.assertEqual(VERIFY.main(arguments), 2)

    def test_done_fixture_rejects_unavailable_live_api(self):
        arguments = ["--alerts-file", str(ROOT / VERIFY.SNAPSHOT)]
        with patch.object(VERIFY, "backlog_b373_done", return_value=True), patch.object(
            VERIFY, "verify_delivered_main_for_candidate"
        ), patch.object(
            VERIFY, "read_alerts", side_effect=VERIFY.CensusError("authenticated API unavailable")
        ):
            self.assertEqual(VERIFY.main(arguments), 2)


if __name__ == "__main__":
    unittest.main()
