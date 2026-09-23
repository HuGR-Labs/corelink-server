"""SQLite coverage for the CAS Compliance metadata rebuild in migration 0144."""

from __future__ import annotations

import sqlite3
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BASE = ROOT / "migrations/d1/0102_cas_retention.sql"
MIGRATION = ROOT / "migrations/d1/0144_cas_retention_compliance_metadata.sql"
GOVERNANCE_CONSUMER = ROOT / "crates/corelink-container/src/routes/dsr/adapter_r2_cas_legalhold.rs"


class S3ObjectLockMigrationTests(unittest.TestCase):
    def test_governance_consumer_filters_out_compliance_rows(self) -> None:
        source = GOVERNANCE_CONSUMER.read_text(encoding="utf-8")
        self.assertIn(
            '"SELECT object_key FROM cas_retention WHERE tenant_id = ?1 AND mode = \'governance\'"',
            source,
        )

    def test_rebuild_preserves_governance_rows_and_adds_tenant_bound_compliance_rows(self) -> None:
        db = sqlite3.connect(":memory:")
        db.executescript(BASE.read_text(encoding="utf-8"))
        legacy_rows = [
            ("tenant-a", "sam", "sam/a/one", None, "governance", 100),
            ("tenant-b", "iad", "iad/b/two", 500, "governance", 200),
        ]
        db.executemany(
            "INSERT INTO cas_retention "
            "(tenant_id, region, object_key, retain_until_ms, mode, pseudonymized_at_ms) "
            "VALUES (?, ?, ?, ?, ?, ?)",
            legacy_rows,
        )
        db.commit()

        db.executescript(MIGRATION.read_text(encoding="utf-8"))

        self.assertEqual(
            legacy_rows,
            db.execute(
                "SELECT tenant_id, region, object_key, retain_until_ms, mode, pseudonymized_at_ms "
                "FROM cas_retention ORDER BY tenant_id"
            ).fetchall(),
        )
        db.execute(
            "INSERT INTO cas_retention "
            "(tenant_id, region, object_key, retain_until_ms, mode, pseudonymized_at_ms, "
            "provider_account_ref, archive_target_ref, object_version, evidence_reference) "
            "VALUES ('tenant-a', 'sam', 'audit/tenant-a/three', 900, 'compliance', 300, "
            "'acct-ref', 'bucket-ref', 'v3', 'evidence-3')"
        )

        self.assertEqual(
            [("sam/a/one",)],
            db.execute(
                "SELECT object_key FROM cas_retention "
                "WHERE tenant_id = ? AND mode = 'governance'",
                ("tenant-a",),
            ).fetchall(),
        )
        self.assertEqual(
            [("audit/tenant-a/three",)],
            db.execute(
                "SELECT object_key FROM cas_retention "
                "WHERE tenant_id = ? AND mode = 'compliance'",
                ("tenant-a",),
            ).fetchall(),
        )
        with self.assertRaises(sqlite3.IntegrityError):
            db.execute(
                "INSERT INTO cas_retention "
                "(tenant_id, region, object_key, retain_until_ms, mode, pseudonymized_at_ms) "
                "VALUES ('tenant-a', 'sam', 'audit/tenant-a/missing-metadata', 900, "
                "'compliance', 300)"
            )
        with self.assertRaises(sqlite3.IntegrityError):
            db.execute(
                "INSERT INTO cas_retention "
                "(tenant_id, region, object_key, retain_until_ms, mode, pseudonymized_at_ms) "
                "VALUES ('tenant-a', 'sam', 'sam/a/bad-mode', NULL, 'unknown', 400)"
            )

        indexes = {
            row[1] for row in db.execute("PRAGMA index_list(cas_retention)").fetchall()
        }
        self.assertTrue(
            {"idx_cas_retention_tenant_expiry", "idx_cas_retention_tenant_mode"}.issubset(
                indexes
            )
        )
        primary_key = [
            row[1]
            for row in sorted(
                (row for row in db.execute("PRAGMA table_info(cas_retention)")),
                key=lambda row: row[5],
            )
            if row[5]
        ]
        self.assertEqual(["tenant_id", "region", "object_key"], primary_key)

        # The surviving Governance writer is replay-safe under its original key.
        db.execute(
            "INSERT OR IGNORE INTO cas_retention "
            "(tenant_id, region, object_key, retain_until_ms, mode, pseudonymized_at_ms) "
            "VALUES ('tenant-a', 'sam', 'sam/a/one', NULL, 'governance', 999)"
        )
        self.assertEqual(
            100,
            db.execute(
                "SELECT pseudonymized_at_ms FROM cas_retention "
                "WHERE tenant_id='tenant-a' AND region='sam' AND object_key='sam/a/one'"
            ).fetchone()[0],
        )


if __name__ == "__main__":
    unittest.main()
