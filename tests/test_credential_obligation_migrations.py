"""Fail-closed acceptance for runner and DevEnv D1 obligation tables."""

from pathlib import Path
import sqlite3
import unittest


ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations/d1"
TABLES = (
    ("devenv_credential_obligation", "", ()),
    ("runner_credential_obligation", ", job_id, repo", ("job", "repo")),
)


class ObligationMigrations(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(":memory:")
        self.db.execute("PRAGMA foreign_keys = ON")
        self.db.execute(
            "CREATE TABLE pat (pat_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, "
            "token_id TEXT, revoked_at_ms INTEGER)"
        )
        self.db.execute("INSERT INTO pat VALUES ('pat','tenant','token',NULL)")
        for name in ("0127_devenv_credential_obligation.sql", "0128_runner_credential_obligation.sql"):
            path = MIGRATIONS / name
            self.assertTrue(path.is_file(), f"missing migration: {name}")
            self.db.executescript(path.read_text(encoding="utf-8"))

    def tearDown(self):
        self.db.close()

    def insert(self, table, extra_columns, extra_values, operation, state, tenant, pat, token):
        columns = "operation_id,tenant_id,state,deadline_ms,pat_id,token_id" + extra_columns
        values = (operation, tenant, state, 1, pat, token, *extra_values)
        marks = ",".join("?" for _ in values)
        self.db.execute(f"INSERT INTO {table} ({columns}) VALUES ({marks})", values)

    def test_only_opaque_identity_columns_and_no_persistent_pat_fk(self):
        for table, _, _ in TABLES:
            with self.subTest(table=table):
                columns = {row[1] for row in self.db.execute(f"PRAGMA table_info({table})")}
                self.assertIn("pat_id", columns)
                self.assertIn("token_id", columns)
                self.assertNotIn("pat_plaintext", columns)
                self.assertNotIn("token_secret", columns)
                self.assertEqual(list(self.db.execute(f"PRAGMA foreign_key_list({table})")), [])

    def test_state_and_identity_checks(self):
        for table, extra_columns, extra_values in TABLES:
            for state, pat, token in (
                ("unknown", None, None),
                ("prepared", "pat", "token"),
                ("issued", None, None),
            ):
                with self.subTest(table=table, state=state):
                    with self.assertRaises(sqlite3.IntegrityError):
                        self.insert(table, extra_columns, extra_values, state, state,
                                    "tenant", pat, token)
            self.insert(table, extra_columns, extra_values, "prepared", "prepared",
                        "tenant", None, None)
            self.insert(table, extra_columns, extra_values, "issued", "issued",
                        "tenant", "pat", "token")

    def test_live_tenant_token_binding_on_insert_and_update(self):
        for table, extra_columns, extra_values in TABLES:
            for label, tenant, pat, token in (
                ("missing", "tenant", "missing", "token"),
                ("cross-tenant", "other", "pat", "token"),
                ("wrong-token", "tenant", "pat", "wrong"),
            ):
                with self.subTest(table=table, label=label):
                    with self.assertRaises(sqlite3.IntegrityError):
                        self.insert(table, extra_columns, extra_values, label,
                                    "issued", tenant, pat, token)
            self.insert(table, extra_columns, extra_values, "live", "issued",
                        "tenant", "pat", "token")
            for column in ("tenant_id", "token_id"):
                with self.subTest(table=table, column=column):
                    with self.assertRaises(sqlite3.IntegrityError):
                        self.db.execute(
                            f"UPDATE {table} SET {column}='wrong' WHERE operation_id='live'"
                        )
        self.db.execute("UPDATE pat SET revoked_at_ms=1 WHERE pat_id='pat'")
        for table in (row[0] for row in TABLES):
            with self.subTest(table=table, phase="revoked"):
                with self.assertRaises(sqlite3.IntegrityError):
                    self.db.execute(f"UPDATE {table} SET state='adopted' WHERE operation_id='live'")
        self.db.execute("DELETE FROM pat WHERE pat_id='pat'")
        for table in (row[0] for row in TABLES):
            self.db.execute(f"UPDATE {table} SET state='revoking' WHERE operation_id='live'")
            self.assertEqual(self.db.execute(
                f"SELECT state FROM {table} WHERE operation_id='live'"
            ).fetchone()[0], "revoking")


if __name__ == "__main__":
    unittest.main()
