"""Stdlib-only structural and mutation checks for the B-126 T3 split."""

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[1]
sys.path.insert(0, str(ROOT / "scripts"))

import verify_b126_t3_refactor as gate  # noqa: E402


class B126T3RefactorTests(unittest.TestCase):
    def test_live_parents_are_bounded_and_wired(self) -> None:
        self.assertEqual(gate.verify(ROOT), [])

    def test_missing_wiring_mutation_is_red(self) -> None:
        for spec in gate.T3_SPECS:
            text = (ROOT / spec.parent).read_text(encoding="utf-8")
            with self.subTest(relative=spec.parent):
                if spec.language == "python":
                    text = text.replace(
                        "from validate_docs_reality_core import *",
                        "from removed_split_module import *",
                        1,
                    )
                elif spec.language == "ts":
                    text = text.replace(spec.module, "removed_split_module", 1)
                else:
                    text = text.replace(f"mod {spec.module};", "mod removed_split_module;", 1)
                self.assertTrue(gate.verify(ROOT, {spec.parent: text}))

    def test_marker_only_and_missing_fragment_are_red(self) -> None:
        for spec in gate.T3_SPECS:
            parent = (ROOT / spec.parent).read_text(encoding="utf-8")
            with self.subTest(relative=spec.parent):
                if spec.language == "python":
                    fake = parent.replace(
                        "from validate_docs_reality_core import *",
                        "# from validate_docs_reality_core import *",
                        1,
                    )
                elif spec.language == "ts":
                    fake = parent.replace(
                        'import { freshState, makeAuditDetail, type MockState } from "./e2e-mock-fixture-state";',
                        '// import { freshState, makeAuditDetail, type MockState } from "./e2e-mock-fixture-state";',
                        1,
                    )
                else:
                    fake = parent.replace(f"mod {spec.module};", f"// mod {spec.module};", 1)
                self.assertTrue(gate.verify(ROOT, {spec.parent: fake}))
                self.assertTrue(gate.verify(ROOT, {spec.fragment: None}))

    def test_use_super_wiring_is_unique_real_and_not_bait(self) -> None:
        for spec in gate.T3_SPECS:
            if not spec.language.startswith("rust") or spec.parent.endswith("adapters.rs"):
                continue
            fragment = (ROOT / spec.fragment).read_text(encoding="utf-8")
            with self.subTest(relative=spec.fragment):
                mutations = {
                    "removed": fragment.replace("use super::*;\n", "", 1),
                    "duplicated": fragment.replace(
                        "use super::*;", "use super::*;\nuse super::*;", 1
                    ),
                    "comment": fragment.replace("use super::*;", "// use super::*;", 1),
                    "string": fragment.replace(
                        "use super::*;",
                        'const _B126_WIRING_BAIT: &str = "use super::*;";',
                        1,
                    ),
                }
                for kind, mutated in mutations.items():
                    with self.subTest(kind=kind):
                        self.assertTrue(gate.verify(ROOT, {spec.fragment: mutated}))

    def test_monolith_growth_and_population_mutations_are_red(self) -> None:
        spec = gate.T3_SPECS[0]
        text = (ROOT / spec.parent).read_text(encoding="utf-8")
        oversized = text + "\n" * (gate.LIMIT + 1 - len(text.splitlines()))
        self.assertTrue(gate.verify(ROOT, {spec.parent: oversized}))
        fragment = (ROOT / spec.fragment).read_text(encoding="utf-8")
        mutated = fragment.replace("function makeAuditDetail", "function removed_symbol", 1)
        self.assertTrue(gate.verify(ROOT, {spec.fragment: mutated}))


if __name__ == "__main__":
    unittest.main()
