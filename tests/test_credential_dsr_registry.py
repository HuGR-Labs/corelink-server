"""New tenant-keyed credential tables must participate in canonical erasure."""

from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[1]
ADAPTER = ROOT / "crates/corelink-container/src/routes/dsr/adapter_d1.rs"
REGISTRY = ROOT / "crates/corelink-container/src/routes/dsr/adapter_d1_registry.rs"
MIGRATIONS = ROOT / "migrations/d1"
TABLES_BY_MIGRATION = {
    "0127_devenv_credential_obligation.sql": ("devenv_credential_obligation",),
    "0128_runner_credential_obligation.sql": ("runner_credential_obligation",),
    "0129_credential_lifecycle_generation.sql": (
        "tenant_credential_revocation_floor", "credential_generation_revocation"),
    "0130_credential_generation_event_receipts.sql": (
        "credential_generation_event_receipts",),
}


def rust_table_array(path: Path, name: str) -> list[str]:
    source = path.read_text(encoding="utf-8")
    match = re.search(rf"\b{name}:\s*&\[&str\]\s*=\s*&\[(.*?)\];", source, re.S)
    if match is None:
        raise AssertionError(f"missing DSR array {name} in {path}")
    return re.findall(r'^\s*"([a-z_]+)"\s*,', match.group(1), re.M)


class CredentialDsrRegistry(unittest.TestCase):
    def test_every_new_tenant_table_is_erased_before_pat(self):
        erase = rust_table_array(ADAPTER, "TENANT_ID_TABLES")
        census = rust_table_array(REGISTRY, "ALL_TENANT_KEYED_TABLES")
        for migration, tables in TABLES_BY_MIGRATION.items():
            if not (MIGRATIONS / migration).is_file():
                continue
            for table in tables:
                with self.subTest(migration=migration, table=table):
                    self.assertIn(table, erase)
                    self.assertIn(table, census)
                    self.assertLess(erase.index(table), erase.index("pat"))


if __name__ == "__main__":
    unittest.main()
