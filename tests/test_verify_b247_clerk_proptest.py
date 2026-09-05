import importlib.util
import sys
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "verify_b247_clerk_proptest", ROOT / "scripts/verify_b247_clerk_proptest.py"
)
assert SPEC and SPEC.loader
verifier = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = verifier
SPEC.loader.exec_module(verifier)


class B247ProptestContract(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.source = verifier.read_source()

    def test_current_contract_and_mutation_teeth(self) -> None:
        verifier.mutation_checks(self.source)

    def test_pem_parse_is_only_in_cached_key_constructor(self) -> None:
        self.assertEqual(self.source.count("EncodingKey::from_rsa_pem"), 2)
        self.assertIn("encode(&header, claims, shared_property_encoding_key())", self.source)
        self.assertIn("sign_2048_smoke", self.source)


if __name__ == "__main__":
    unittest.main()
