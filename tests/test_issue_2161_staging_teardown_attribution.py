"""SQLite adversarial coverage for the frozen #2161 attribution ledger."""

from __future__ import annotations

import sqlite3
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATION = ROOT / "migrations/d1/0147_staging_load_test_run_ownership.sql"
RUN = ("123456", "cas", "staging", "a" * 40, "open", 1)
RESOURCE = ("123456", "cas", "cas_reference", "b" * 64, "ref-1", "disposable", "registered", 1)
RESOURCE_CLASSES = (
    "cas_reference", "webhook_inbox", "webhook_effect", "dsr_artifact", "dsr_obligation",
    "audit_evidence", "billing_audit", "signup_artifact", "byok_artifact",
)


class Issue2161AttributionTests(unittest.TestCase):
    def setUp(self) -> None:
        self.db = sqlite3.connect(":memory:")
        self.db.executescript(MIGRATION.read_text(encoding="utf-8"))
        self.db.execute("PRAGMA foreign_keys = ON")

    def add_run(self, values: tuple[object, ...] = RUN) -> None:
        self.db.execute("INSERT INTO staging_load_test_runs VALUES (?, ?, ?, ?, ?, ?)", values)

    def complete_scans(self, counts: dict[str, int] | None = None) -> None:
        for resource_class in RESOURCE_CLASSES:
            self.db.execute(
                "INSERT INTO staging_load_test_resource_scans VALUES (?, ?, ?, ?, ?, ?)",
                (*RUN[:2], resource_class, "complete", (counts or {}).get(resource_class, 0), 1),
            )

    def test_rejects_noncanonical_or_nonstaging_run_identity(self) -> None:
        for index, value in ((0, "01"), (0, "x"), (1, "*"), (2, "prod"), (3, "A" * 40)):
            values = list(RUN)
            values[index] = value
            with self.subTest(value=value), self.assertRaises(sqlite3.IntegrityError):
                self.add_run(tuple(values))

    def test_rejects_cross_run_and_sealed_registration(self) -> None:
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("INSERT INTO staging_load_test_resources VALUES (?, ?, ?, ?, ?, ?, ?, ?)", RESOURCE)
        self.add_run()
        self.db.execute(
            "INSERT INTO staging_load_test_runs VALUES (?, ?, ?, ?, ?, ?)",
            ("123457", "cas", "staging", "c" * 40, "open", 1),
        )
        self.db.execute("INSERT INTO staging_load_test_resources VALUES (?, ?, ?, ?, ?, ?, ?, ?)", RESOURCE)
        cross_run = ("123457", *RESOURCE[1:])
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("INSERT INTO staging_load_test_resources VALUES (?, ?, ?, ?, ?, ?, ?, ?)", cross_run)
        self.complete_scans({"cas_reference": 1})
        self.db.execute("UPDATE staging_load_test_runs SET state = 'sealed' WHERE run_id = '123456'")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("INSERT INTO staging_load_test_resources VALUES (?, ?, ?, ?, ?, ?, ?, ?)", RESOURCE)

    def test_retained_or_shared_reference_cannot_be_deleted_or_reassigned(self) -> None:
        self.add_run()
        for resource_class in ("dsr_obligation", "audit_evidence", "billing_audit"):
            forbidden = (*RESOURCE[:2], resource_class, "c" * 63 + str(len(resource_class) % 10), f"forbidden-{resource_class}", "disposable", "registered", 1)
            with self.subTest(resource_class=resource_class), self.assertRaises(sqlite3.IntegrityError):
                self.db.execute("INSERT INTO staging_load_test_resources VALUES (?, ?, ?, ?, ?, ?, ?, ?)", forbidden)
        retained = (*RESOURCE[:4], "ref-retained", "retained", "registered", 1)
        self.db.execute("INSERT INTO staging_load_test_resources VALUES (?, ?, ?, ?, ?, ?, ?, ?)", retained)
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE staging_load_test_resources SET state = 'deleted'")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE staging_load_test_resources SET run_id = '999999'")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("DELETE FROM staging_load_test_resources")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE staging_load_test_resources SET state = 'preserved'")

    def test_disposable_delete_and_run_states_are_forward_only_and_idempotent(self) -> None:
        self.add_run()
        self.db.execute("INSERT INTO staging_load_test_resources VALUES (?, ?, ?, ?, ?, ?, ?, ?)", RESOURCE)
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE staging_load_test_resources SET state = 'deleted'")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE staging_load_test_resources SET state = 'delete_started'")
        self.complete_scans({"cas_reference": 1})
        self.db.execute("UPDATE staging_load_test_runs SET state = 'sealed'")
        self.db.execute("UPDATE staging_load_test_runs SET state = 'teardown_started'")
        self.db.execute("UPDATE staging_load_test_resources SET state = 'delete_started'")
        self.db.execute("UPDATE staging_load_test_resources SET state = 'deleted'")
        self.db.execute("UPDATE staging_load_test_resources SET state = 'deleted'")
        self.db.execute("UPDATE staging_load_test_runs SET state = 'reconciled'")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE staging_load_test_runs SET state = 'open'")

    def test_scan_records_are_exactly_bound_and_can_report_incomplete_inventory(self) -> None:
        self.add_run()
        self.db.execute("INSERT INTO staging_load_test_resource_scans VALUES (?, ?, ?, ?, ?, ?)", (*RUN[:2], "cas_reference", "incomplete", 0, 1))
        self.assertEqual(self.db.execute("SELECT state FROM staging_load_test_resource_scans").fetchone(), ("incomplete",))
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE staging_load_test_runs SET state = 'sealed'")
        self.db.execute("PRAGMA foreign_keys = OFF")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("INSERT INTO staging_load_test_resource_scans VALUES (?, ?, ?, ?, ?, ?)", ("999999", "cas", "cas_reference", "complete", 0, 1))
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("INSERT INTO staging_load_test_resource_scans VALUES (?, ?, ?, ?, ?, ?)", (*RUN[:2], "unknown", "complete", 0, 1))

    def test_sealed_run_identity_and_scans_are_immutable(self) -> None:
        self.add_run()
        self.complete_scans()
        self.db.execute("UPDATE staging_load_test_runs SET state = 'sealed'")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE staging_load_test_runs SET target_deployment_sha = ?", ("d" * 40,))
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE staging_load_test_resource_scans SET observed_count = 1")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("INSERT INTO staging_load_test_resource_scans VALUES (?, ?, ?, ?, ?, ?)", (*RUN[:2], "cas_reference", "complete", 0, 2))


if __name__ == "__main__":
    unittest.main()
