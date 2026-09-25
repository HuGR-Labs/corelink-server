from __future__ import annotations

import sqlite3
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
M147 = ROOT / "migrations/d1/0147_staging_load_test_run_ownership.sql"
M149 = ROOT / "migrations/d1/0149_staging_load_test_r2_intents.sql"
RUN = ("123", "cas", "staging", "a" * 40, "open", 100)
INTENT = ("b" * 64, "123", "cas", "a" * 40, "cas_reference", "c" * 64, "opaque", "retained", "prepared", 101, None)
RESOURCE = ("123", "cas", "cas_reference", "c" * 64, "opaque", "retained", "registered", 102)


def database() -> sqlite3.Connection:
    db = sqlite3.connect(":memory:")
    db.execute("PRAGMA foreign_keys=ON")
    db.executescript(M147.read_text())
    db.executescript(M149.read_text())
    db.execute("INSERT INTO staging_load_test_runs VALUES (?, ?, ?, ?, ?, ?)", RUN)
    return db


class R2IntentContractTests(unittest.TestCase):
    def test_r2_intent_requires_registration_before_commit(self) -> None:
        db = database()
        db.execute("INSERT INTO staging_load_test_r2_intents VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", INTENT)
        with self.assertRaises(sqlite3.IntegrityError):
            db.execute("UPDATE staging_load_test_r2_intents SET state='committed', committed_at_ms=102")
        db.execute("INSERT INTO staging_load_test_resources VALUES (?, ?, ?, ?, ?, ?, ?, ?)", RESOURCE)
        db.execute("UPDATE staging_load_test_r2_intents SET state='committed', committed_at_ms=102")
        self.assertEqual(db.execute("SELECT state FROM staging_load_test_r2_intents").fetchone(), ("committed",))

    def test_r2_intent_rejects_wrong_run_rewrite_reverse_and_delete(self) -> None:
        db = database()
        db.execute("INSERT INTO staging_load_test_r2_intents VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", INTENT)
        for sql in (
            "UPDATE staging_load_test_r2_intents SET run_id='999'",
            "UPDATE staging_load_test_r2_intents SET state='committed', committed_at_ms=102",
            "DELETE FROM staging_load_test_r2_intents",
        ):
            with self.subTest(sql=sql), self.assertRaises(sqlite3.IntegrityError):
                db.execute(sql)

    def test_r2_prepare_requires_exact_open_staging_identity(self) -> None:
        for index, changed in enumerate(("wrong", "d" * 40)):
            db = database()
            values = list(INTENT)
            values[0] = f"{index + 1:064x}"
            if index == 0:
                values[2] = changed
            else:
                values[3] = changed
            with self.subTest(index=index), self.assertRaises(sqlite3.IntegrityError):
                db.execute("INSERT INTO staging_load_test_r2_intents VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", values)

    def test_run_cannot_seal_around_unresolved_r2_intent(self) -> None:
        db = database()
        db.execute("INSERT INTO staging_load_test_r2_intents VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)", INTENT)
        classes = (
            "cas_reference",
            "webhook_inbox",
            "webhook_effect",
            "dsr_artifact",
            "dsr_obligation",
            "audit_evidence",
            "billing_audit",
            "signup_artifact",
            "byok_artifact",
        )
        db.executemany(
            "INSERT INTO staging_load_test_resource_scans VALUES ('123', 'cas', ?, 'complete', 0, 102)",
            ((resource_class,) for resource_class in classes),
        )
        with self.assertRaises(sqlite3.IntegrityError):
            db.execute("UPDATE staging_load_test_runs SET state='sealed' WHERE run_id='123' AND scenario='cas'")
        db.execute("UPDATE staging_load_test_runs SET state='failed' WHERE run_id='123' AND scenario='cas'")
        self.assertEqual(db.execute("SELECT state FROM staging_load_test_runs").fetchone(), ("failed",))


if __name__ == "__main__":
    unittest.main()
