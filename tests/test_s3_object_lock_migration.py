"""SQLite coverage for the B-046 Compliance metadata rebuild.

This is intentionally a local SQLite fixture: hosted CI runs it as the D1
compatible acceptance witness. It proves preservation and constraints; it does
not claim a provider capability or production migration execution.
"""

from __future__ import annotations

import sqlite3
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
LEGACY = ROOT / "migrations/d1/0102_cas_retention.sql"
MIGRATION = ROOT / "migrations/d1/0144_cas_retention_compliance_metadata.sql"


class ComplianceRetentionMigrationTests(unittest.TestCase):
    def legacy_db(self) -> sqlite3.Connection:
        db = sqlite3.connect(":memory:")
        db.executescript(LEGACY.read_text(encoding="utf-8"))
        db.execute(
            "INSERT INTO cas_retention VALUES (?, ?, ?, ?, ?, ?)",
            ("tenant-a", "sam", "sam/a/one", None, "governance", 100),
        )
        return db

    def test_forward_preserves_governance_and_adds_tenant_scoped_compliance(self) -> None:
        db = self.legacy_db()
        db.executescript(MIGRATION.read_text(encoding="utf-8"))
        self.assertEqual(
            [("tenant-a", "sam", "sam/a/one", None, "governance", 100)],
            db.execute(
                "SELECT tenant_id, region, object_key, retain_until_ms, mode, pseudonymized_at_ms "
                "FROM cas_retention"
            ).fetchall(),
        )
        db.execute(
            "INSERT INTO cas_retention (tenant_id, region, object_key, retain_until_ms, mode, "
            "pseudonymized_at_ms, provider_account_ref, archive_target_ref, object_version, evidence_reference) "
            "VALUES (?, ?, ?, ?, 'compliance', ?, ?, ?, ?, ?)",
            ("tenant-b", "eu-west-1", "audit/tenant-b/one", 900, 200, "acct-ref", "target-ref", "v1", "evidence-1"),
        )
        self.assertEqual(
            [("audit/tenant-b/one", "v1")],
            db.execute(
                "SELECT object_key, object_version FROM cas_retention "
                "WHERE tenant_id = ? AND mode = 'compliance'",
                ("tenant-b",),
            ).fetchall(),
        )
        self.assertEqual(
            [("sam/a/one",)],
            db.execute(
                "SELECT object_key FROM cas_retention WHERE tenant_id = ? AND mode = 'governance'",
                ("tenant-a",),
            ).fetchall(),
        )

    def test_constraints_fence_governance_and_compliance_rows(self) -> None:
        db = self.legacy_db()
        db.executescript(MIGRATION.read_text(encoding="utf-8"))
        with self.assertRaises(sqlite3.IntegrityError):
            db.execute(
                "INSERT INTO cas_retention VALUES ('tenant-c', 'sam', 'audit/c/missing', 1, 'compliance', 1, NULL, NULL, NULL, NULL)"
            )
        with self.assertRaises(sqlite3.IntegrityError):
            db.execute(
                "INSERT INTO cas_retention VALUES ('tenant-c', 'sam', 'sam/c/governance', NULL, 'governance', 1, 'acct', NULL, NULL, NULL)"
            )

    def test_governance_writer_replay_is_idempotent_and_metadata_is_unchanged(self) -> None:
        db = self.legacy_db()
        db.executescript(MIGRATION.read_text(encoding="utf-8"))
        db.execute(
            "INSERT OR IGNORE INTO cas_retention "
            "(tenant_id, region, object_key, retain_until_ms, mode, pseudonymized_at_ms) "
            "VALUES ('tenant-a', 'sam', 'sam/a/one', NULL, 'governance', 999)"
        )
        self.assertEqual(
            (100, None, None),
            db.execute(
                "SELECT pseudonymized_at_ms, provider_account_ref, object_version FROM cas_retention "
                "WHERE tenant_id = 'tenant-a' AND region = 'sam' AND object_key = 'sam/a/one'"
            ).fetchone(),
        )


if __name__ == "__main__":
    unittest.main()
