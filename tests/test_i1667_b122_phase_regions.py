"""Adversarial tests for the repository-owned B-122 contract verifier."""

from __future__ import annotations

import importlib.util
import shutil
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).parents[1]
SCRIPT = ROOT / "scripts/verify_i1667_b122_phase_regions.py"
spec = importlib.util.spec_from_file_location("i1667_b122_phase_regions", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


class B122PhaseRegionContractTests(unittest.TestCase):
    def test_canonical_sources_pass(self) -> None:
        self.assertEqual(verifier.verify(ROOT), [])

    def test_missing_blocking_bridge_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            candidate = Path(directory)
            source = ROOT / "crates/corelink-container/src/adapter_cache.rs"
            target = candidate / "crates/corelink-container/src/adapter_cache.rs"
            target.parent.mkdir(parents=True)
            target.write_text(
                source.read_text(encoding="utf-8").replace(
                    "let ledger = crate::origin_timing::current_ledger();",
                    "let ledger = None;",
                    1,
                ),
                encoding="utf-8",
            )
            for relative in (
                "crates/corelink-container/src/origin_timing.rs",
                "crates/corelink-container/src/origin_timing/tests_recording.rs",
            ):
                destination = candidate / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy(ROOT / relative, destination)
            self.assertTrue(verifier.verify(candidate))

    def test_missing_depth_or_adversarial_test_fails_closed(self) -> None:
        for relative, needle in (
            ("crates/corelink-container/src/origin_timing.rs", "depth: usize"),
            (
                "crates/corelink-container/src/origin_timing/tests_recording.rs",
                "nested_reentry_of_the_same_phase_records_once",
            ),
        ):
            with self.subTest(relative=relative), tempfile.TemporaryDirectory() as directory:
                candidate = Path(directory)
                for path in (
                    "crates/corelink-container/src/adapter_cache.rs",
                    "crates/corelink-container/src/origin_timing.rs",
                    "crates/corelink-container/src/origin_timing/tests_recording.rs",
                ):
                    destination = candidate / path
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy(ROOT / path, destination)
                path = candidate / relative
                path.write_text(path.read_text(encoding="utf-8").replace(needle, "removed", 1), encoding="utf-8")
                self.assertTrue(verifier.verify(candidate))


if __name__ == "__main__":
    unittest.main()
