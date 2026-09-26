from __future__ import annotations

import sqlite3
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
M147 = ROOT / "migrations/d1/0147_staging_load_test_run_ownership.sql"
M149 = ROOT / "migrations/d1/0149_staging_load_test_r2_intents.sql"
M151 = ROOT / "migrations/d1/0151_staging_load_test_teardown_receipts.sql"
CLASSES = (
    "cas_reference", "webhook_inbox", "webhook_effect", "dsr_artifact",
    "dsr_obligation", "audit_evidence", "billing_audit", "signup_artifact", "byok_artifact",
)
RETAINED = {"cas_reference", "dsr_obligation", "audit_evidence", "billing_audit"}


def database() -> sqlite3.Connection:
    db = sqlite3.connect(":memory:")
    db.execute("PRAGMA foreign_keys=ON")
    db.executescript(M147.read_text())
    db.executescript(M149.read_text())
    db.executescript(M151.read_text())
    db.execute(
        "INSERT INTO staging_load_test_runs VALUES (?, ?, ?, ?, ?, ?)",
        ("123", "dsr", "staging", "a" * 40, "open", 100),
    )
    return db


def resource_values(resource_class: str) -> tuple[object, ...]:
    retained = resource_class in RETAINED
    return (
        "123", "dsr", resource_class, f"{CLASSES.index(resource_class) + 1:064x}",
        f"handle-{resource_class}", "retained" if retained else "disposable", "registered", 101,
    )


def prepare_teardown(db: sqlite3.Connection) -> None:
    for resource_class in CLASSES:
        db.execute("INSERT INTO staging_load_test_resources VALUES (?, ?, ?, ?, ?, ?, ?, ?)", resource_values(resource_class))
        db.execute(
            "INSERT INTO staging_load_test_resource_scans VALUES (?, ?, ?, 'complete', 1, 102)",
            ("123", "dsr", resource_class),
        )
    db.execute("UPDATE staging_load_test_runs SET state='sealed'")
    db.execute("UPDATE staging_load_test_runs SET state='teardown_started'")
    for resource_class in CLASSES:
        if resource_class in RETAINED:
            db.execute("UPDATE staging_load_test_resources SET state='preserved' WHERE resource_class=?", (resource_class,))
        else:
            db.execute("UPDATE staging_load_test_resources SET state='delete_started' WHERE resource_class=?", (resource_class,))
            db.execute("UPDATE staging_load_test_resources SET state='deleted' WHERE resource_class=?", (resource_class,))


def add_complete_counts(
    db: sqlite3.Connection, override: tuple[str, str, int] | None = None
) -> None:
    for resource_class in CLASSES:
        retained = resource_class in RETAINED
        values: list[object] = [
            "123", "dsr", resource_class, 1, 0 if retained else 1, 0 if retained else 1,
            1 if retained else 0, 0, 0, 0, "preserved" if retained else "absent",
        ]
        if override is not None and override[0] == resource_class:
            values[("inventory_count", "attempted_count", "deleted_count", "preserved_count", "remaining_count", "quarantined_count", "cross_run_deletion_count", "readback_state").index(override[1]) + 3] = override[2]
        db.execute(
            "INSERT INTO staging_load_test_teardown_receipt_counts VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            values,
        )


def receipt(db: sqlite3.Connection, state: str = "reconciled") -> None:
    db.execute(
        "INSERT INTO staging_load_test_teardown_receipts VALUES (?, ?, ?, ?, ?, ?, ?)",
        ("123", "dsr", "a" * 40, "corelink.staging-load-test-teardown-receipt.v2", state, 200, "b" * 64),
    )


class TeardownContractTests(unittest.TestCase):
    def test_reconciled_receipt_requires_all_nine_exact_readback_counts_and_is_immutable(self) -> None:
        db = database()
        prepare_teardown(db)
        with self.assertRaises(sqlite3.IntegrityError):
            receipt(db)
        add_complete_counts(db)
        receipt(db)
        self.assertEqual(db.execute("SELECT terminal_state FROM staging_load_test_teardown_receipts").fetchone(), ("reconciled",))
        for sql in (
            "UPDATE staging_load_test_teardown_receipts SET terminal_state='failed'",
            "DELETE FROM staging_load_test_teardown_receipts",
            "UPDATE staging_load_test_teardown_receipt_counts SET deleted_count=0",
        ):
            with self.subTest(sql=sql), self.assertRaises(sqlite3.IntegrityError):
                db.execute(sql)

    def test_reconciled_receipt_rejects_retained_delete_partial_disposable_and_cross_run_counts(self) -> None:
        for resource_class, column, value in (
            ("cas_reference", "deleted_count", 1),
            ("webhook_effect", "attempted_count", 2),
            ("webhook_inbox", "remaining_count", 1),
            ("signup_artifact", "cross_run_deletion_count", 1),
        ):
            db = database()
            prepare_teardown(db)
            with self.subTest(resource_class=resource_class, column=column):
                if column in {"attempted_count", "cross_run_deletion_count"}:
                    with self.assertRaises(sqlite3.IntegrityError):
                        add_complete_counts(db, (resource_class, column, value))
                else:
                    add_complete_counts(db, (resource_class, column, value))
                    with self.assertRaises(sqlite3.IntegrityError):
                        receipt(db)

    def test_locators_are_exact_open_disposable_pairs_and_never_retained(self) -> None:
        db = database()
        db.execute("INSERT INTO staging_load_test_resources VALUES (?, ?, ?, ?, ?, ?, ?, ?)", resource_values("webhook_inbox"))
        ref = f"{CLASSES.index('webhook_inbox') + 1:064x}"
        db.execute(
            "INSERT INTO staging_load_test_teardown_locators VALUES (?, ?, ?, ?, ?, ?, ?)",
            ("123", "dsr", "webhook_inbox", ref, "webhook_inbox_v1", "{}", 101),
        )
        with self.assertRaises(sqlite3.IntegrityError):
            db.execute(
                "INSERT INTO staging_load_test_teardown_locators VALUES (?, ?, ?, ?, ?, ?, ?)",
                ("123", "dsr", "webhook_inbox", ref, "signup_pilot_v1", "{}", 101),
            )
        with self.assertRaises(sqlite3.IntegrityError):
            db.execute(
                "INSERT INTO staging_load_test_teardown_locators VALUES (?, ?, ?, ?, ?, ?, ?)",
                ("123", "dsr", "cas_reference", "f" * 64, "webhook_inbox_v1", "{}", 101),
            )

    def test_synthetic_marker_and_prepared_reconciliation_are_exact_and_append_only(self) -> None:
        db = database()
        db.execute("INSERT INTO staging_load_test_synthetic_tenants VALUES (?, ?, ?, ?, ?)", ("123", "dsr", "tenant-1", "generation_zero_empty", 101))
        with self.assertRaises(sqlite3.IntegrityError):
            db.execute("UPDATE staging_load_test_synthetic_tenants SET baseline_marker='anything'")
        intent = ("c" * 64, "123", "dsr", "a" * 40, "dsr_artifact", "d" * 64, "r2-key", "disposable", "prepared", 101, None)
        db.execute("INSERT INTO staging_load_test_r2_intents VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", intent)
        db.execute(
            "INSERT INTO staging_load_test_prepared_intent_reconciliations VALUES (?, ?, ?, ?, ?, ?)",
            ("c" * 64, "123", "dsr", "dsr_artifact", "deleted", 102),
        )
        with self.assertRaises(sqlite3.IntegrityError):
            db.execute("UPDATE staging_load_test_prepared_intent_reconciliations SET readback_outcome='absent'")


if __name__ == "__main__":
    unittest.main()
