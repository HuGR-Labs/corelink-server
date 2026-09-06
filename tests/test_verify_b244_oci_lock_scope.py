import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_b244_oci_lock_scope", ROOT / "scripts/verify_b244_oci_lock_scope.py"
)
assert SPEC and SPEC.loader
verifier = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = verifier
SPEC.loader.exec_module(verifier)


class B244BoundaryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.source = verifier.read_source()

    def test_current_source_and_mutation_teeth(self) -> None:
        verifier.mutation_checks(self.source)

    def test_renamed_focal_and_restoration(self) -> None:
        renamed = self.source.replace(verifier.TEST_NAME, verifier.TEST_NAME + "_renamed", 1)
        with self.assertRaises(verifier.VerificationError):
            verifier.validate_source(renamed)
        verifier.validate_source(self.source)

    def test_duplicate_focal_and_restoration(self) -> None:
        declaration = "#[tokio::test]\n" f"async fn {verifier.TEST_NAME}()"
        duplicate = self.source.replace(
            declaration,
            declaration + "\n#[tokio::test]\nasync fn " + verifier.TEST_NAME + "()",
            1,
        )
        with self.assertRaises(verifier.VerificationError):
            verifier.validate_source(duplicate)
        verifier.validate_source(self.source)


if __name__ == "__main__":
    unittest.main()
