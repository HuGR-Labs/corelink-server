"""Negative mutation coverage for the frozen #2366 repository contract."""

from __future__ import annotations

import importlib.util
import shutil
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/verify_issue_2366_dsr_redrive_prereq.py"
spec = importlib.util.spec_from_file_location("issue_2366", SCRIPT)
assert spec and spec.loader
contract = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = contract
spec.loader.exec_module(contract)


class Issue2366ContractTests(unittest.TestCase):
    def fixture(self) -> tempfile.TemporaryDirectory[str]:
        temp = tempfile.TemporaryDirectory()
        root = Path(temp.name)
        for path in (
            contract.MIGRATION, contract.SCHEMA_FENCE, contract.SECRET_FENCE,
            contract.SIGNUP_DEPLOY, contract.STAGING_BOOTSTRAP,
            contract.STAGING_VERIFIER, contract.STAGING_TOPOLOGY,
            contract.SECRETS_INVENTORY,
        ):
            destination = root / path
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / path, destination)
        return temp

    def mutate(self, path: Path, old: str, new: str = "") -> list[str]:
        temp = self.fixture()
        with temp:
            target = Path(temp.name) / path
            text = target.read_text(encoding="utf-8")
            self.assertIn(old, text)
            target.write_text(text.replace(old, new, 1), encoding="utf-8")
            return contract.verify(Path(temp.name))

    def mutate_table(self, table: str, old: str, new: str = "") -> list[str]:
        temp = self.fixture()
        with temp:
            target = Path(temp.name) / contract.MIGRATION
            text = target.read_text(encoding="utf-8")
            start = text.index(f"CREATE TABLE IF NOT EXISTS {table}")
            end = text.index("\n);", start) + 3
            body = text[start:end]
            self.assertIn(old, body)
            target.write_text(text[:start] + body.replace(old, new, 1) + text[end:], encoding="utf-8")
            return contract.verify(Path(temp.name))

    def test_repository_satisfies_frozen_contract(self) -> None:
        self.assertEqual(contract.verify(ROOT), [])

    def test_each_required_schema_column_is_fail_closed(self) -> None:
        for column in contract.ENVELOPE_COLUMNS:
            with self.subTest(column=column):
                self.assertIn(
                    f"migration:envelope-column:{column}",
                    self.mutate_table("dsr_dlq_redrive_envelopes", f"  {column} "),
                )
        for column in contract.AUDIT_COLUMNS:
            with self.subTest(column=column):
                self.assertIn(
                    f"migration:audit-column:{column}",
                    self.mutate_table("dsr_dlq_redrive_audit", f"  {column} "),
                )

    def test_state_transition_and_key_mutations_are_rejected(self) -> None:
        mutations = (
            ("migration:envelope-states", "'ambiguous'", "'removed'"),
            ("migration:audit-transitions", "transition IN ('claimed', 'submitted', 'ambiguous')", "transition IN ('claimed', 'submitted')"),
            ("migration:opaque-event-primary-key", "event_id TEXT PRIMARY KEY NOT NULL", "event_id TEXT NOT NULL"),
            ("migration:audit-primary-key", "PRIMARY KEY (event_id, transition)", "PRIMARY KEY (event_id)"),
        )
        for expected, old, new in mutations:
            with self.subTest(expected=expected):
                self.assertIn(expected, self.mutate(contract.MIGRATION, old, new))

    def test_each_required_secret_name_is_fail_closed(self) -> None:
        cases = (
            ("production-secret-fence", contract.SECRET_FENCE, "  DSR_DLQ_REDRIVE_AUTH_KEY\n"),
            ("staging:secret-inventory", contract.STAGING_BOOTSTRAP, "STAGING_DSR_DLQ_REDRIVE_AUTH_KEY"),
            ("staging-verifier:worker-secret", contract.STAGING_VERIFIER, '"DSR_DLQ_REDRIVE_AUTH_KEY"'),
            ("staging-verifier:environment-secret", contract.STAGING_VERIFIER, '"STAGING_DSR_DLQ_REDRIVE_AUTH_KEY"'),
            ("secrets-inventory:production", contract.SECRETS_INVENTORY, "| 297 | DSR DLQ redrive authority key"),
            ("secrets-inventory:staging", contract.SECRETS_INVENTORY, "| 298 | Staging DSR DLQ redrive authority key"),
        )
        for expected, path, old in cases:
            with self.subTest(expected=expected):
                self.assertIn(expected, self.mutate(path, old))

    def test_each_deploy_fence_must_precede_wrangler_deploy(self) -> None:
        self.assertIn(
            "signup-deploy:schema-before-deploy",
            self.mutate(contract.SIGNUP_DEPLOY, "- name: Verify DSR redrive schema before deploy", "- name: Disabled DSR schema check"),
        )
        self.assertIn(
            "staging:schema-before-deploy",
            self.mutate(contract.STAGING_BOOTSTRAP, "bash scripts/verify-signup-worker-dsr-redrive-schema.sh", "# schema check removed"),
        )


if __name__ == "__main__":
    unittest.main()
