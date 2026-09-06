"""Mutation controls for the B-063 archive Worker boundary."""

from __future__ import annotations

import importlib.util
import sys
from pathlib import Path


ROOT = Path(__file__).parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_b063_archive_pipeline", ROOT / "scripts/verify_b063_archive_pipeline.py"
)
assert SPEC and SPEC.loader
verifier = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = verifier
SPEC.loader.exec_module(verifier)


def test_b063_archive_pipeline_guard_and_mutations() -> None:
    assert verifier.verify(ROOT) == 4
