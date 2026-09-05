#!/usr/bin/env python3
"""Mutation tests for the B-151 closed-world OpenAPI and locale gate."""

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "verify_b151_openapi.py"
SPEC = importlib.util.spec_from_file_location("verify_b151_openapi", SCRIPT)
assert SPEC and SPEC.loader
verifier = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verifier)


def contract() -> dict:
    return {
        "openapi": "3.1.0",
        "info": {"title": "CoreLink", "version": "1.0.0"},
        "paths": {
            "/health": {"get": {"operationId": "getHealth"}},
            "/items": {
                "get": {"operationId": "listItems"},
                "post": {"operationId": "createItem"},
            },
        },
        "components": {"schemas": {"Item": {"type": "object"}}},
    }


class ClosedWorldMutationTests(unittest.TestCase):
    def assert_mutation_rejected(self, published: dict, expected: str) -> None:
        errors = verifier.compare_closed_world(contract(), published)
        self.assertTrue(errors)
        self.assertTrue(any(expected in error for error in errors), errors)

    def test_identical_documents_pass(self) -> None:
        self.assertEqual(verifier.compare_closed_world(contract(), contract()), [])

    def test_path_removal_is_rejected(self) -> None:
        mutant = contract()
        del mutant["paths"]["/items"]
        self.assert_mutation_rejected(mutant, "missing paths: /items")

    def test_path_addition_is_rejected(self) -> None:
        mutant = contract()
        mutant["paths"]["/admin"] = {"get": {"operationId": "admin"}}
        self.assert_mutation_rejected(mutant, "extra paths: /admin")

    def test_method_removal_is_rejected(self) -> None:
        mutant = contract()
        del mutant["paths"]["/items"]["post"]
        self.assert_mutation_rejected(mutant, "missing methods at /items: POST")

    def test_method_addition_is_rejected(self) -> None:
        mutant = contract()
        mutant["paths"]["/health"]["post"] = {"operationId": "healthPost"}
        self.assert_mutation_rejected(mutant, "extra methods at /health: POST")

    def test_operation_identity_mutation_is_rejected(self) -> None:
        mutant = contract()
        mutant["paths"]["/items"]["get"]["operationId"] = "wrong"
        self.assert_mutation_rejected(mutant, "operationId drift at GET /items")

    def test_document_level_mutation_is_rejected(self) -> None:
        mutant = contract()
        mutant["components"]["schemas"]["Item"]["required"] = ["id"]
        self.assert_mutation_rejected(mutant, "differs from canonical beyond")

    def test_unknown_path_item_key_fails_closed(self) -> None:
        mutant = contract()
        mutant["paths"]["/health"]["wat"] = {}
        with self.assertRaises(verifier.InstrumentError):
            verifier.compare_closed_world(contract(), mutant)


class LocaleMutationTests(unittest.TestCase):
    def test_stale_phrase_mutation_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for locale in verifier.EXPECTED_LOCALES:
                path = root / "apps" / "docs" / "i18n" / locale / "docusaurus-plugin-content-docs" / "current" / "explanation" / "rbac" / "index.mdx"
                path.parent.mkdir(parents=True)
                path.write_text("three customer-facing categories\n", encoding="utf-8")
            stale = root / "apps" / "docs" / "i18n" / "de" / "docusaurus-plugin-content-docs" / "current" / "explanation" / "rbac" / "index.mdx"
            stale.write_text("five customer-facing categories\n", encoding="utf-8")
            errors = verifier.check_rbac_locales(root)
            self.assertEqual(len(errors), 1)
            self.assertIn("de", errors[0])

    def test_missing_locale_is_instrument_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            errors = verifier.check_rbac_locales(root)
            self.assertEqual(len(errors), len(verifier.EXPECTED_LOCALES))


if __name__ == "__main__":
    unittest.main()
