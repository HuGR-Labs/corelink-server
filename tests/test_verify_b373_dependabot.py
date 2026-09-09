#!/usr/bin/env python3
"""Focused tests for the B-373 candidate/post-merge source boundary."""

from __future__ import annotations

import importlib.util
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

    def test_candidate_mode_still_accepts_exact_snapshot_fixture(self):
        self.assertEqual(
            VERIFY.main(
                [
                    "--alerts-file",
                    str(ROOT / VERIFY.SNAPSHOT),
                ]
            ),
            0,
        )


if __name__ == "__main__":
    unittest.main()
