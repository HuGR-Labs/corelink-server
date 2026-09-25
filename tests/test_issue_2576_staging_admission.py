"""SQLite adversarial contract for #2576 migration and adapter statement shape."""

import hashlib
import re
import sqlite3
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
RUN_SQL = (ROOT / "migrations/d1/0147_staging_load_test_run_ownership.sql").read_text(encoding="utf-8")
NONCE_SQL = (ROOT / "migrations/d1/0148_staging_load_test_admission_nonce.sql").read_text(encoding="utf-8")
SOURCE = (ROOT / "crates/corelink-container/src/storage/staging_load_test_admission.rs").read_text(encoding="utf-8")


def sql_constant(name: str) -> str:
    match = re.search(rf'const {name}: &str = "([^\"]+)";', SOURCE, re.DOTALL)
    if match is None:
        raise AssertionError(f"missing {name}")
    return re.sub(r"\\\n\s*", "", match.group(1))


def run(run_id: str, sha: str, admitted: int = 1_000) -> tuple[object, ...]:
    return (run_id, "cas", "staging", sha, "open", admitted)


def nonce(digest: str, run_id: str, sha: str, issued: int = 900, expires: int = 1_800, admitted: int = 1_000) -> tuple[object, ...]:
    return (digest, run_id, "cas", "staging", sha, issued, expires, admitted)


class AdmissionMigrationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.db = sqlite3.connect(":memory:")
        self.db.execute("PRAGMA foreign_keys = ON")
        self.db.executescript(RUN_SQL)
        self.db.executescript(NONCE_SQL)

    def add_run(self, value: tuple[object, ...]) -> None:
        self.db.execute("INSERT INTO staging_load_test_runs VALUES (?, ?, ?, ?, ?, ?)", value)

    def add_nonce(self, value: tuple[object, ...]) -> None:
        self.db.execute("INSERT INTO staging_load_test_admission_nonces VALUES (?, ?, ?, ?, ?, ?, ?, ?)", value)

    def test_prefix_digest_only_and_append_only(self) -> None:
        self.assertEqual("0148_staging_load_test_admission_nonce.sql", ROOT.joinpath("migrations/d1/0148_staging_load_test_admission_nonce.sql").name)
        columns = {row[1] for row in self.db.execute("PRAGMA table_info(staging_load_test_admission_nonces)")}
        self.assertNotIn("nonce", columns)
        self.assertNotIn("credential", columns)
        self.add_run(run("100001", "a" * 40))
        self.add_nonce(nonce("b" * 64, "100001", "a" * 40))
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE staging_load_test_admission_nonces SET issued_at_ms = 1")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("DELETE FROM staging_load_test_admission_nonces")

    def test_each_constraint_uses_an_otherwise_unique_valid_identity(self) -> None:
        variants = (
            nonce("B" * 64, "100010", "a" * 40),
            nonce("c" * 63, "100011", "b" * 40),
            nonce("d" * 64, "01", "c" * 40),
            ("e" * 64, "100013", "cas", "prod", "d" * 40, 900, 1_800, 1_000),
            nonce("f" * 64, "100014", "E" * 40),
            nonce("a" * 64, "100015", "f" * 40, 900, 900),
            nonce("b" * 64, "100016", "a" * 40, 900, 900_901),
            nonce("c" * 64, "100017", "b" * 40, 900, 1_800, 1_800),
        )
        for value in variants:
            # Every variant that can name a valid parent gets its own one;
            # this prevents a missing/duplicate parent from masking the
            # nonce-table constraint under test.
            if value[1] != "01" and value[4] != "E" * 40:
                self.add_run(run(value[1], value[4]))
            with self.subTest(value=value[1]), self.assertRaises(sqlite3.IntegrityError):
                self.add_nonce(value)

    def test_replay_duplicate_and_missing_foreign_key_fail(self) -> None:
        self.add_run(run("100020", "a" * 40))
        self.add_nonce(nonce("a" * 64, "100020", "a" * 40))
        with self.assertRaises(sqlite3.IntegrityError):
            self.add_nonce(nonce("b" * 64, "100020", "a" * 40))
        self.add_run(run("100021", "b" * 40))
        with self.assertRaises(sqlite3.IntegrityError):
            self.add_nonce(nonce("a" * 64, "100021", "b" * 40))
        self.add_run(run("100023", "d" * 40))
        with self.assertRaises(sqlite3.IntegrityError):
            self.add_nonce(nonce("e" * 64, "100023", "e" * 40))
        with self.assertRaises(sqlite3.IntegrityError):
            self.add_nonce(nonce("c" * 64, "100022", "c" * 40))

    def test_actual_adapter_statements_and_parameter_order_execute_atomically(self) -> None:
        run_sql, nonce_sql = sql_constant("SQL_INSERT_RUN"), sql_constant("SQL_INSERT_NONCE")
        self.assertRegex(
            SOURCE,
            r"let run_params = vec!\[\s*json!\(admission\.run_id\),\s*"
            r"json!\(scenario\),\s*json!\(admission\.target_deployment_sha\),\s*json!\(now_ms\),",
        )
        self.assertRegex(
            SOURCE,
            r"let nonce_params = vec!\[\s*json!\(admission\.nonce_digest\),\s*"
            r"json!\(admission\.run_id\),\s*json!\(scenario\),\s*"
            r"json!\(admission\.target_deployment_sha\),\s*"
            r"json!\(admission\.issued_at_ms\),\s*json!\(admission\.expires_at_ms\),\s*json!\(now_ms\),",
        )
        self.assertLess(SOURCE.index("D1BatchStatement::new(SQL_INSERT_RUN"), SOURCE.index("D1BatchStatement::new(SQL_INSERT_NONCE"))
        raw_nonce = "01" * 32
        digest = hashlib.sha256(b"corelink/staging-load-admission-nonce/v1\0" + bytes.fromhex(raw_nonce)).hexdigest()
        self.db.execute("BEGIN")
        self.db.execute(run_sql, ("100030", "cas", "a" * 40, 1_000))
        self.db.execute(nonce_sql, (digest, "100030", "cas", "a" * 40, 900, 1_800, 1_000))
        self.db.commit()
        self.assertEqual(self.db.execute("SELECT nonce_digest FROM staging_load_test_admission_nonces").fetchone(), (digest,))
        self.db.execute("BEGIN")
        self.db.execute(run_sql, ("100031", "cas", "b" * 40, 1_000))
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(nonce_sql, (digest, "100031", "cas", "b" * 40, 900, 1_800, 1_000))
        self.db.rollback()
        self.assertIsNone(self.db.execute("SELECT run_id FROM staging_load_test_runs WHERE run_id = '100031'").fetchone())


if __name__ == "__main__":
    unittest.main()
