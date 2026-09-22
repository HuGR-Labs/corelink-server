#!/usr/bin/env python3
"""Verify the Terraform drift region contract and its D1 compatibility migration."""

from __future__ import annotations

import sqlite3
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
REGIONS = ("wnam", "enam", "weur", "sam")
LEGACY_REGIONS = ("us-east", "us-west", "eu-west", "ap-southeast", "sa-east", "global")
CONSUMER = ROOT / "crates/corelink-terraform-drift-consumer/src/event.rs"
MIGRATION_0025 = ROOT / "migrations/d1/0025_terraform_drift_findings.sql"
MIGRATION_0131 = ROOT / "migrations/d1/0131_terraform_drift_summary_artifact.sql"
MIGRATION_0139 = ROOT / "migrations/d1/0139_terraform_drift_region_contract.sql"


def insert_finding(connection: sqlite3.Connection, finding_id: bytes, region: str) -> None:
    connection.execute(
        """
        INSERT INTO terraform_drift_findings (
            finding_id, region, detected_at_ms, plan_diff_count, plan_summary,
            severity, status
        ) VALUES (?, ?, 1, 0, '', 'none', 'open')
        """,
        (finding_id, region),
    )


class TerraformDriftRegionContractTests(unittest.TestCase):
    def test_consumer_has_exactly_the_four_workflow_regions(self) -> None:
        source = CONSUMER.read_text(encoding="utf-8")
        self.assertIn(
            'pub const REGIONS: &[&str] = &["wnam", "enam", "weur", "sam"];',
            source,
        )
        for legacy in LEGACY_REGIONS:
            self.assertNotIn(f'"{legacy}"', source)

    def test_migration_keeps_history_but_rejects_legacy_new_writes(self) -> None:
        connection = sqlite3.connect(":memory:")
        connection.executescript(MIGRATION_0025.read_text(encoding="utf-8"))
        connection.executescript(MIGRATION_0131.read_text(encoding="utf-8"))
        insert_finding(connection, b"legacy-row-00001", "us-east")
        connection.executescript(MIGRATION_0139.read_text(encoding="utf-8"))

        for region in REGIONS:
            insert_finding(connection, f"new-{region}".encode().ljust(16, b"-"), region)
        with self.assertRaises(sqlite3.IntegrityError):
            insert_finding(connection, b"legacy-new-row-1", "us-east")

        self.assertEqual(
            connection.execute(
                "SELECT region FROM terraform_drift_findings WHERE finding_id = ?",
                (b"legacy-row-00001",),
            ).fetchone()[0],
            "us-east",
        )
        with self.assertRaises(sqlite3.IntegrityError):
            connection.execute(
                "UPDATE terraform_drift_findings SET region = 'us-west' WHERE finding_id = ?",
                (b"legacy-row-00001",),
            )

        columns = {
            row[1] for row in connection.execute("PRAGMA table_info(terraform_drift_findings)")
        }
        self.assertIn("plan_summary_artifact_url", columns)
        indexes = {
            row[1] for row in connection.execute("PRAGMA index_list(terraform_drift_findings)")
        }
        self.assertTrue(
            {
                "idx_terraform_drift_open",
                "idx_terraform_drift_region_time",
                "idx_terraform_drift_severity",
            }.issubset(indexes)
        )


if __name__ == "__main__":
    unittest.main()
