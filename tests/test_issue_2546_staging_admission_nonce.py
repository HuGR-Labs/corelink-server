"""Credentialless SQLite contract tests for migration 0148 and batch atomicity."""

from __future__ import annotations

import sqlite3
import hashlib
import re
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations/d1"
RUN_MIGRATION = MIGRATIONS / "0147_staging_load_test_run_ownership.sql"
NONCE_MIGRATION = MIGRATIONS / "0148_staging_load_test_admission_nonce.sql"
RUN = ("123456", "cas", "staging", "a" * 40, "open", 1000)
NONCE = ("b" * 64, "123456", "cas", "staging", "a" * 40, 900, 1800, 1000)
ADAPTER = ROOT / "crates/corelink-container/src/storage/staging_load_test_admission.rs"


def rust_sql_constant(name: str) -> str:
    source = ADAPTER.read_text(encoding="utf-8")
    match = re.search(rf'const {name}: &str = "([^\"]+)";', source, re.DOTALL)
    if match is None:
        raise AssertionError(f"adapter SQL constant {name} is missing")
    return re.sub(r"\\\n\s*", "", match.group(1))


class AdmissionNonceMigrationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.db = sqlite3.connect(":memory:")
        self.db.execute("PRAGMA foreign_keys = ON")
        self.db.executescript(RUN_MIGRATION.read_text(encoding="utf-8"))
        self.db.executescript(NONCE_MIGRATION.read_text(encoding="utf-8"))

    def add_run(self, values: tuple[object, ...] = RUN) -> None:
        self.db.execute("INSERT INTO staging_load_test_runs VALUES (?, ?, ?, ?, ?, ?)", values)

    def add_nonce(self, values: tuple[object, ...] = NONCE) -> None:
        self.db.execute(
            "INSERT INTO staging_load_test_admission_nonces VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            values,
        )

    def test_migration_prefix_and_nonce_table_contract(self) -> None:
        self.assertEqual(NONCE_MIGRATION.name, "0148_staging_load_test_admission_nonce.sql")
        columns = self.db.execute(
            "PRAGMA table_info(staging_load_test_admission_nonces)"
        ).fetchall()
        names = {row[1] for row in columns}
        self.assertEqual(
            names,
            {"nonce_digest", "run_id", "scenario", "target_environment", "target_deployment_sha",
             "issued_at_ms", "expires_at_ms", "admitted_at_ms"},
        )
        self.assertNotIn("nonce", names)
        self.assertNotIn("credential", names)

    def test_valid_consumption_and_replay_or_duplicate_is_rejected(self) -> None:
        self.add_run()
        self.add_nonce()
        with self.assertRaises(sqlite3.IntegrityError):
            self.add_nonce()
        second_nonce = ("c" * 64, *NONCE[1:])
        with self.assertRaises(sqlite3.IntegrityError):
            self.add_nonce(second_nonce)
        other_run = ("123457", "cas", "staging", "d" * 40, "open", 1000)
        self.add_run(other_run)
        replay_nonce = (NONCE[0], "123457", "cas", "staging", "d" * 40, 900, 1800, 1000)
        with self.assertRaises(sqlite3.IntegrityError):
            self.add_nonce(replay_nonce)

    def test_invalid_digest_identity_and_lifetime_are_rejected(self) -> None:
        self.add_run()
        self.add_nonce()
        variants = (
            ("B" * 64, *NONCE[1:]),
            ("c" * 63, *NONCE[1:]),
            ("d" * 64, "01", *NONCE[2:]),
            ("e" * 64, "123456", "cas", "prod", *NONCE[4:]),
            ("f" * 64, "123456", "cas", "staging", "A" * 40, *NONCE[5:]),
            ("g" * 64, "123456", "cas", "staging", "a" * 40, 1000, 1000, 1000),
            ("h" * 64, "123456", "cas", "staging", "a" * 40, 1000, 999, 1000),
            ("i" * 64, "123456", "cas", "staging", "a" * 40, 1000, 901001, 1000),
            ("j" * 64, "123456", "cas", "staging", "a" * 40, 1000, 2000, 2000),
        )
        for values in variants:
            with self.subTest(values=values), self.assertRaises(sqlite3.IntegrityError):
                self.add_nonce(values)

    def test_nonce_record_is_append_only(self) -> None:
        self.add_run()
        self.add_nonce()
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(
                "UPDATE staging_load_test_admission_nonces SET target_deployment_sha = ?",
                ("c" * 40,),
            )
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("DELETE FROM staging_load_test_admission_nonces")

    def test_second_statement_failure_rolls_back_run_and_nonce(self) -> None:
        self.add_run()
        self.add_nonce()
        self.db.commit()
        self.db.execute("BEGIN")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(
                "INSERT INTO staging_load_test_runs VALUES (?, ?, ?, ?, ?, ?)",
                ("123457", "cas", "staging", "c" * 40, "open", 1000),
            )
            self.add_nonce((NONCE[0], "123457", "cas", "staging", "c" * 40, 900, 1800, 1000))
        self.db.rollback()
        self.assertEqual(
            self.db.execute("SELECT COUNT(*) FROM staging_load_test_admission_nonces").fetchone(),
            (1,),
        )
        self.assertIsNone(
            self.db.execute(
                "SELECT run_id FROM staging_load_test_runs WHERE run_id = '123457'"
            ).fetchone()
        )

    def test_adapter_sql_and_parameter_order_match_atomic_sqlite_contract(self) -> None:
        source = ADAPTER.read_text(encoding="utf-8")
        run_params_contract = re.compile(
            r"let run_params = vec!\[\s*json!\(admission\.run_id\),\s*"
            r"json!\(scenario\),\s*json!\(admission\.target_deployment_sha\),\s*json!\(now_ms\),\s*\]",
            re.DOTALL,
        )
        nonce_params_contract = re.compile(
            r"let nonce_params = vec!\[\s*json!\(nonce_digest\),\s*"
            r"json!\(admission\.run_id\),\s*json!\(scenario\),\s*"
            r"json!\(admission\.target_deployment_sha\),\s*"
            r"json!\(admission\.issued_at_ms\),\s*json!\(admission\.expires_at_ms\),\s*json!\(now_ms\),\s*\]",
            re.DOTALL,
        )
        self.assertRegex(source, run_params_contract)
        self.assertRegex(source, nonce_params_contract)
        self.assertLess(source.index("D1BatchStatement::new(SQL_INSERT_RUN"),
                        source.index("D1BatchStatement::new(SQL_INSERT_NONCE"))

        run_sql = rust_sql_constant("SQL_INSERT_RUN")
        nonce_sql = rust_sql_constant("SQL_INSERT_NONCE")
        run_id, scenario, target_sha, now_ms = "123456", "cas", "a" * 40, 1000
        raw_nonce = "01" * 32
        nonce_digest = hashlib.sha256(
            b"corelink/staging-load-admission-nonce/v1\0" + bytes.fromhex(raw_nonce)
        ).hexdigest()
        self.assertNotIn(raw_nonce, run_sql + nonce_sql)
        self.db.execute("BEGIN")
        self.db.execute(run_sql, (run_id, scenario, target_sha, now_ms))
        self.db.execute(
            nonce_sql,
            (nonce_digest, run_id, scenario, target_sha, 900, 1800, now_ms),
        )
        self.db.commit()
        self.assertEqual(
            self.db.execute(
                "SELECT run_id, scenario, target_environment, target_deployment_sha, admitted_at_ms "
                "FROM staging_load_test_runs WHERE run_id = ?",
                (run_id,),
            ).fetchone(),
            (run_id, scenario, "staging", target_sha, now_ms),
        )
        self.assertEqual(
            self.db.execute(
                "SELECT nonce_digest, run_id, scenario, target_environment, target_deployment_sha, "
                "issued_at_ms, expires_at_ms, admitted_at_ms FROM staging_load_test_admission_nonces"
            ).fetchone(),
            (nonce_digest, run_id, scenario, "staging", target_sha, 900, 1800, now_ms),
        )

    def test_missing_run_cannot_consume_nonce(self) -> None:
        with self.assertRaises(sqlite3.IntegrityError):
            self.add_nonce()


if __name__ == "__main__":
    unittest.main()
