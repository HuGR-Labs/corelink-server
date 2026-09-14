"""Acceptance tests for the additive credential-lifecycle D1 schema."""

from pathlib import Path
import sqlite3
import unittest


ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS = ROOT / "migrations" / "d1"


class CredentialLifecycleMigrations(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(":memory:")
        self.db.execute(
            "CREATE TABLE pat (pat_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, "
            "runner_job_ac_key TEXT, token_id TEXT, revoked_at_ms INTEGER)"
        )
        self.db.execute("INSERT INTO pat VALUES ('customer', 'tenant', NULL, NULL, NULL)")
        self.db.execute("INSERT INTO pat VALUES ('runner', 'tenant', 'job-key', 'runner-token', NULL)")
        for number, name in (
            (127, "devenv_credential_obligation"),
            (128, "runner_credential_obligation"),
            (129, "credential_lifecycle_generation"),
            (130, "credential_generation_event_receipts"),
        ):
            path = MIGRATIONS / f"{number:04d}_{name}.sql"
            self.assertTrue(path.is_file(), f"missing migration: {path.name}")
            self.db.executescript(path.read_text(encoding="utf-8"))

    def tearDown(self):
        self.db.close()

    def test_backfill_classifies_runner_but_not_customer_pat(self):
        rows = dict(self.db.execute("SELECT pat_id, lifecycle_generation FROM pat"))
        self.assertEqual(rows, {"customer": None, "runner": "0"})

    def test_generation_rejects_noncanonical_and_out_of_range_values(self):
        invalid = ("", "01", "-1", "1x", "9223372036854775808")
        for generation in invalid:
            with self.subTest(generation=generation):
                with self.assertRaises(sqlite3.IntegrityError):
                    self.db.execute(
                        "INSERT INTO tenant_credential_revocation_floor VALUES (?, ?)",
                        ("tenant", generation),
                    )
        self.db.execute(
            "INSERT INTO tenant_credential_revocation_floor VALUES ('tenant', '1')"
        )
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(
                "UPDATE tenant_credential_revocation_floor SET revoked_through='00' "
                "WHERE tenant_id='tenant'"
            )

    def test_event_receipt_is_unique_and_has_closed_state_enum(self):
        self.db.execute(
            "INSERT INTO credential_generation_event_receipts VALUES "
            "('event', 'tenant', '1', 'requested')"
        )
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(
                "INSERT INTO credential_generation_event_receipts VALUES "
                "('event', 'tenant', '1', 'requested')"
            )
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(
                "INSERT INTO credential_generation_event_receipts VALUES "
                "('other', 'tenant', '1', 'unknown')"
            )

    def test_new_tables_have_only_opaque_identity_and_state_columns(self):
        expected = {
            "devenv_credential_obligation": {
                "operation_id", "tenant_id", "state", "deadline_ms", "pat_id",
                "token_id", "lifecycle_generation",
            },
            "runner_credential_obligation": {
                "operation_id", "tenant_id", "job_id", "repo", "state",
                "deadline_ms", "pat_id", "token_id", "lifecycle_generation",
            },
            "tenant_credential_revocation_floor": {"tenant_id", "revoked_through"},
            "credential_generation_revocation": {
                "pat_id", "token_id", "tenant_id", "lifecycle_generation", "state",
            },
            "credential_generation_event_receipts": {
                "event_id", "tenant_id", "lifecycle_generation", "state",
            },
        }
        for table, columns in expected.items():
            with self.subTest(table=table):
                actual = {row[1] for row in self.db.execute(f"PRAGMA table_info({table})")}
                self.assertEqual(actual, columns)

    def test_every_generation_column_rejects_noncanonical_values(self):
        valid_inserts = (
            ("devenv_credential_obligation", "INSERT INTO devenv_credential_obligation "
             "(operation_id,tenant_id,state,deadline_ms,lifecycle_generation) "
             "VALUES ('dev','tenant','prepared',1,?)"),
            ("runner_credential_obligation", "INSERT INTO runner_credential_obligation "
             "(operation_id,tenant_id,job_id,repo,state,deadline_ms,lifecycle_generation) "
             "VALUES ('run','tenant','job','repo','prepared',1,?)"),
            ("credential_generation_revocation", "INSERT INTO credential_generation_revocation "
             "(pat_id,token_id,tenant_id,lifecycle_generation) "
             "VALUES ('projection','token','tenant',?)"),
            ("credential_generation_event_receipts", "INSERT INTO credential_generation_event_receipts "
             "(event_id,tenant_id,lifecycle_generation,state) "
             "VALUES ('receipt','tenant',?,'requested')"),
        )
        for table, statement in valid_inserts:
            with self.subTest(table=table):
                for value in ("", "01", "-1", "1x", "9223372036854775808"):
                    with self.subTest(value=value):
                        self.db.execute("SAVEPOINT invalid_generation")
                        try:
                            with self.assertRaises(sqlite3.IntegrityError):
                                self.db.execute(statement, (value,))
                        finally:
                            self.db.execute("ROLLBACK TO invalid_generation")
                            self.db.execute("RELEASE invalid_generation")
                self.db.execute(statement, ("9223372036854775807",))
                with self.assertRaises(sqlite3.IntegrityError):
                    self.db.execute(
                        f"UPDATE {table} SET lifecycle_generation='00'"
                    )
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE pat SET lifecycle_generation='01' WHERE pat_id='runner'")

    def test_revocation_projection_has_closed_state_and_unique_pat(self):
        self.db.execute(
            "INSERT INTO credential_generation_revocation VALUES "
            "('pat', 'token', 'tenant', '1', 'pending')"
        )
        self.db.execute(
            "UPDATE credential_generation_revocation SET state='revoked' WHERE pat_id='pat'"
        )
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(
                "INSERT INTO credential_generation_revocation VALUES "
                "('pat', 'other-token', 'tenant', '1', 'pending')"
            )
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(
                "INSERT INTO credential_generation_revocation VALUES "
                "('other', 'token', 'tenant', '1', 'invalid')"
            )


    def test_revocation_floor_cannot_decrease(self):
        self.db.execute(
            "INSERT INTO tenant_credential_revocation_floor VALUES ('tenant', '5')"
        )
        for value in ("1", "4"):
            with self.subTest(value=value):
                with self.assertRaises(sqlite3.IntegrityError):
                    self.db.execute(
                        "UPDATE tenant_credential_revocation_floor SET revoked_through=? "
                        "WHERE tenant_id='tenant'", (value,)
                    )
        self.db.execute(
            "UPDATE tenant_credential_revocation_floor SET revoked_through='6' "
            "WHERE tenant_id='tenant'"
        )
        self.assertEqual(self.db.execute(
            "SELECT revoked_through FROM tenant_credential_revocation_floor "
            "WHERE tenant_id='tenant'"
        ).fetchone()[0], "6")

    def test_revoked_projection_cannot_reopen(self):
        self.db.execute(
            "INSERT INTO credential_generation_revocation VALUES "
            "('pat', 'token', 'tenant', '1', 'pending')"
        )
        self.db.execute(
            "UPDATE credential_generation_revocation SET state='revoked' WHERE pat_id='pat'"
        )
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(
                "UPDATE credential_generation_revocation SET state='pending' WHERE pat_id='pat'"
            )

    def test_complete_receipt_cannot_reopen(self):
        self.db.execute(
            "INSERT INTO credential_generation_event_receipts VALUES "
            "('event', 'tenant', '1', 'requested')"
        )
        self.db.execute(
            "UPDATE credential_generation_event_receipts SET state='complete' "
            "WHERE event_id='event'"
        )
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute(
                "UPDATE credential_generation_event_receipts SET state='requested' "
                "WHERE event_id='event'"
            )

if __name__ == "__main__":
    unittest.main()
