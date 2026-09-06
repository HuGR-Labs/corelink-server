from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_b158_bazel_remote_cache.py"
spec = importlib.util.spec_from_file_location("b158_verifier", SCRIPT)
assert spec and spec.loader
verifier = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = verifier
spec.loader.exec_module(verifier)


class B158VerifierTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.document = (ROOT / verifier.DOC).read_text(encoding="utf-8")
        cls.routes = "\n".join(
            (ROOT / relative).read_text(encoding="utf-8")
            for relative in verifier.ROUTES
        )

    def test_published_prefix_constructs_registered_cas_and_ac_paths(self) -> None:
        self.assertEqual(verifier.violations(self.document, self.routes), [])

    def test_document_prefix_mutation_reopens_mapping(self) -> None:
        mutated = self.document.replace(
            "build --remote_cache=https://corelink-api.humangr.com/bazel/cache",
            "build --remote_cache=https://corelink-api.humangr.com/bazel/v2",
            1,
        )
        self.assertNotEqual(mutated, self.document)
        self.assertTrue(verifier.violations(mutated, self.routes))

    def test_route_registration_mutation_reopens_mapping(self) -> None:
        mutated = self.routes.replace(
            f'"{verifier.CAS_ROUTE}"', f'"{verifier.WRONG_CAS_ROUTE}"', 1
        ).replace(
            f'"{verifier.AC_ROUTE}"', f'"{verifier.WRONG_AC_ROUTE}"', 1
        )
        self.assertNotEqual(mutated, self.routes)
        self.assertTrue(verifier.violations(self.document, mutated))

    def test_verifier_self_test_exercises_both_mutation_directions(self) -> None:
        verifier.mutation_self_test(self.document, self.routes)


if __name__ == "__main__":
    unittest.main()
