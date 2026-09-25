"""SQLite adversarial coverage for #2546's additive D1 nonce ledger."""

from __future__ import annotations

import sqlite3
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = (
    ROOT / "migrations/d1/0147_staging_load_test_run_ownership.sql",
    ROOT / "migrations/d1/0148_staging_load_test_admission_nonce.sql",
)
RUN = ("123456", "signup", "staging", "a" * 40, "open", 1)
NONCE = ("b" * 64, "123456", "signup", 1, 10_000, 2)
ADMISSION_SOURCE = (
    ROOT / "crates/corelink-container/src/storage/staging_load_test_admission.rs"
)


class Issue2546AdmissionNonceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.db = sqlite3.connect(":memory:")
        self.db.execute("PRAGMA foreign_keys = ON")
        for migration in MIGRATIONS:
            self.db.executescript(migration.read_text(encoding="utf-8"))

    def add_run(self, values: tuple[object, ...] = RUN) -> None:
        self.db.execute("INSERT INTO staging_load_test_runs VALUES (?, ?, ?, ?, ?, ?)", values)

    def add_nonce(self, values: tuple[object, ...] = NONCE) -> None:
        self.db.execute(
            "INSERT INTO staging_load_test_admission_nonces VALUES (?, ?, ?, ?, ?, ?)", values
        )

    def test_nonce_digest_and_run_identity_are_unique_and_append_only(self) -> None:
        self.add_run()
        self.add_nonce()
        with self.assertRaises(sqlite3.IntegrityError):
            self.add_nonce(("c" * 64, "123456", "signup", 1, 10_000, 2))
        self.add_run(("123457", "signup", "staging", "c" * 40, "open", 1))
        with self.assertRaises(sqlite3.IntegrityError):
            self.add_nonce(("b" * 64, "123457", "signup", 1, 10_000, 2))
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE staging_load_test_admission_nonces SET consumed_at_ms = 3")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("DELETE FROM staging_load_test_admission_nonces")

    def test_checks_reject_malformed_digest_and_time_bounds(self) -> None:
        self.add_run()
        for values in (
            ("B" * 64, "123456", "signup", 1, 10_000, 2),
            ("b" * 64, "123456", "signup", 10_000, 1, 10_000),
            ("b" * 64, "123456", "signup", 1, 10_000, 10_000),
        ):
            with self.subTest(values=values), self.assertRaises(sqlite3.IntegrityError):
                self.add_nonce(values)

    def test_batch_failure_rolls_back_run_and_nonce_together(self) -> None:
        self.add_run()
        self.add_nonce()
        self.db.commit()

        # Mirrors the two ordered statements sent through D1 batch. The second
        # insert collides on nonce digest, so D1's all-or-nothing batch leaves
        # no newly admitted run from statement one.
        self.db.execute("BEGIN")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(
                "INSERT INTO staging_load_test_runs VALUES (?, ?, ?, ?, ?, ?)",
                ("123457", "signup", "staging", "c" * 40, "open", 2),
            )
            self.db.execute(
                "INSERT INTO staging_load_test_admission_nonces VALUES (?, ?, ?, ?, ?, ?)",
                ("b" * 64, "123457", "signup", 2, 10_000, 3),
            )
        self.db.rollback()
        self.assertIsNone(
            self.db.execute(
                "SELECT 1 FROM staging_load_test_runs WHERE run_id = '123457'"
            ).fetchone()
        )

    def test_nonce_requires_existing_run_and_rejects_replay(self) -> None:
        with self.assertRaises(sqlite3.IntegrityError):
            self.add_nonce()
        self.add_run()
        self.add_nonce()
        with self.assertRaises(sqlite3.IntegrityError):
            self.add_nonce()

    def test_migration_0148_is_additive_after_0147_and_tracks_exact_run_identity(self) -> None:
        self.assertEqual(MIGRATIONS[0].name[:4], "0147")
        self.assertEqual(MIGRATIONS[1].name[:4], "0148")
        self.assertLess(MIGRATIONS[0].name, MIGRATIONS[1].name)

        migration = MIGRATIONS[1].read_text(encoding="utf-8")
        self.assertIn("PRIMARY KEY (nonce_digest)", migration)
        self.assertIn("UNIQUE (run_id, scenario)", migration)
        self.assertIn(
            "FOREIGN KEY (run_id, scenario)\n        REFERENCES staging_load_test_runs(run_id, scenario)",
            migration,
        )
        self.assertIn("trg_staging_load_test_admission_nonce_no_update", migration)
        self.assertIn("trg_staging_load_test_admission_nonce_no_delete", migration)

    def test_adapter_uses_one_ordered_d1_transaction_for_run_and_nonce(self) -> None:
        source = ADMISSION_SOURCE.read_text(encoding="utf-8")
        start = source.index("pub async fn consume_verified_admission")
        end = source.index("\nfn unix_time_ms", start)
        method = source[start:end]

        self.assertEqual(method.count("D1BatchStatement::new("), 2)
        self.assertEqual(method.count("self.d1.batch(batch).await"), 1)
        self.assertLess(method.index("SQL_INSERT_RUN"), method.index("SQL_INSERT_NONCE"))
        self.assertIn("let batch = vec![", method)


if __name__ == "__main__":
    unittest.main()
