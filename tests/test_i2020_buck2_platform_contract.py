#!/usr/bin/env python3
"""Mutation tests for the credentialless issue #2020 Buck2 contract."""
from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_i2020_buck2_platform", ROOT / "scripts/verify_i2020_buck2_platform.py"
)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


class Buck2PlatformContractTest(unittest.TestCase):
    def test_current_source_and_mutations_pass(self) -> None:
        MODULE.mutation_checks(MODULE._sources())
