"""Focused SQLite validation of actual production SQL and checked-in migrations."""
from pathlib import Path
import re
import sqlite3
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
SOURCE = (ROOT / "worker/src/lib/devenv_cleanup.ts").read_text()
NOW_SQL = "CAST(strftime('%s','now') AS INTEGER) * 1000"


def sqls(function):
    body = SOURCE.split("export async function " + function + "(", 1)[1]
    body = body.split("export async function ", 1)[0]
    valid = "length(lifecycle_generation) BETWEEN 1 AND 19 AND lifecycle_generation NOT GLOB '*[^0-9]*' AND (length(lifecycle_generation) = 1 OR substr(lifecycle_generation, 1, 1) <> '0') AND (length(lifecycle_generation) < 19 OR lifecycle_generation <= '9223372036854775807')"
    return [query.replace("${SQL_NOW}", NOW_SQL).replace("${SQL_VALID_GENERATION}", valid) for query in re.findall(r'db\.prepare\(\s*(?:`([^`]+)`|"([^"\n]+)")', body) for query in [query[0] or query[1]]]


class AtomicCleanup(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(":memory:")
        self.db.execute("PRAGMA foreign_keys = ON")
        self.db.execute("CREATE TABLE tenant (tenant_id TEXT PRIMARY KEY)")
        self.db.execute("INSERT INTO tenant VALUES ('tenant')")
        signup = (ROOT / "migrations/d1/0037_signup_orchestration.sql").read_text()
        table = re.search(r"CREATE TABLE IF NOT EXISTS pat \([\s\S]+?\n\);", signup).group()
        self.db.executescript(table)
        for migration, column in [("0054_pat_token_id.sql", "token_id"), ("0063_pat_customer_keys.sql", "revoked_at_ms")]:
            text = (ROOT / "migrations/d1" / migration).read_text()
            statement = re.search(r"ALTER TABLE pat ADD COLUMN " + column + r"[^;]+;", text).group()
            self.db.execute(statement)
        self.db.executescript((ROOT / "migrations/d1/0127_devenv_credential_obligation.sql").read_text())
        self.db.execute("ALTER TABLE devenv_credential_obligation ADD COLUMN lifecycle_generation TEXT NOT NULL DEFAULT '0'")
        self.db.execute("ALTER TABLE pat ADD COLUMN lifecycle_generation TEXT")
        self.db.execute("CREATE TABLE tenant_credential_revocation_floor (tenant_id TEXT PRIMARY KEY NOT NULL, revoked_through TEXT NOT NULL)")
        self.now = int(time.time() * 1000)
        self.expiry = self.now + 100000
        self.db.commit()

    def prepare(self, deadline=None, operation="operation", generation="0"):
        query = sqls("prepareDevenvOperation")[1]
        self.db.execute(query, (operation, "tenant", deadline if deadline is not None else self.now + 90000, generation))
        self.db.commit()

    def activate(self, pat="pat", fail=False, operation="operation", generation="0"):
        queries = sqls("activateDevenvPat")
        with self.db:
            self.db.execute(queries[0], (pat, "tenant", "hash-" + pat, "read-write", self.expiry, "token-" + pat, operation, generation))
            if fail:
                raise RuntimeError("injected transaction failure")
            self.db.execute(queries[1], (pat, "token-" + pat, operation, "tenant", generation))

    def revoke(self):
        queries = sqls("revokeDevenvOperation")
        with self.db:
            for query in queries[:3]:
                self.db.execute(query, ("operation", "tenant"))

    def test_activation_and_obligation_commit_together(self):
        self.prepare()
        self.activate()
        self.assertEqual(self.db.execute("SELECT state, pat_id, token_id FROM devenv_credential_obligation").fetchone(), ("issued", "pat", "token-pat"))
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM pat").fetchone()[0], 1)

    def test_failure_between_statements_rolls_back_activation(self):
        self.prepare()
        with self.assertRaises(RuntimeError):
            self.activate(fail=True)
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM pat").fetchone()[0], 0)
        self.assertEqual(self.db.execute("SELECT state FROM devenv_credential_obligation").fetchone()[0], "prepared")

    def test_late_mint_cannot_activate_after_abort(self):
        self.prepare()
        self.revoke()
        self.activate()
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM pat").fetchone()[0], 0)

    def test_late_prepare_and_expired_intent_cannot_activate(self):
        self.prepare(self.now - 2000)
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM devenv_credential_obligation").fetchone()[0], 0)
        self.prepare()
        self.db.execute("UPDATE devenv_credential_obligation SET deadline_ms = ?", (self.now - 2000,))
        self.db.commit()
        self.activate()
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM pat").fetchone()[0], 0)

    def test_retry_cannot_activate_a_second_pat(self):
        self.prepare()
        self.activate()
        self.activate("second")
        self.assertEqual(self.db.execute("SELECT pat_id FROM pat").fetchall(), [("pat",)])

    def test_revoke_is_idempotent_and_rejects_late_adoption(self):
        self.prepare()
        self.activate()
        self.revoke()
        first = self.db.execute("SELECT revoked_at_ms FROM pat").fetchone()[0]
        self.assertIsNotNone(first)
        self.revoke()
        self.assertEqual(self.db.execute("SELECT revoked_at_ms FROM pat").fetchone()[0], first)
        self.db.execute(sqls("adoptDevenvOperation")[0], ("operation", "tenant", "pat"))
        self.assertEqual(self.db.execute("SELECT state FROM devenv_credential_obligation").fetchone()[0], "revoking")

    def test_adoption_requires_exact_tenant_and_pat(self):
        self.prepare()
        self.activate()
        query = sqls("adoptDevenvOperation")[0]
        self.db.execute(query, ("operation", "other", "pat"))
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM devenv_credential_obligation").fetchone()[0], 1)
        self.db.execute(query, ("operation", "tenant", "pat"))
        self.assertEqual(self.db.execute("SELECT state FROM devenv_credential_obligation").fetchone()[0], "adopted")

    def test_absent_intent_cleanup_fences_a_late_prepare(self):
        self.revoke()
        self.prepare()
        self.activate()
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM pat").fetchone()[0], 0)
        self.assertEqual(self.db.execute("SELECT state FROM devenv_credential_obligation").fetchone()[0], "revoked")

    def test_revoke_after_confirmed_adoption_does_not_revoke_owned_pat(self):
        self.prepare()
        self.activate()
        self.db.execute(sqls("adoptDevenvOperation")[0], ("operation", "tenant", "pat"))
        self.db.commit()
        self.revoke()
        self.assertIsNone(self.db.execute("SELECT revoked_at_ms FROM pat").fetchone()[0])

    def test_generation_floor_blocks_old_activation_and_adoption(self):
        self.prepare(generation="1"); self.db.execute("INSERT INTO tenant_credential_revocation_floor VALUES ('tenant','1')"); self.db.commit()
        self.activate(); self.assertEqual(self.db.execute("SELECT COUNT(*) FROM pat").fetchone()[0], 0)
        self.prepare(operation="new-operation", generation="2"); self.activate("new-pat", operation="new-operation", generation="2")
        self.assertEqual(self.db.execute("SELECT lifecycle_generation FROM devenv_credential_obligation WHERE operation_id='new-operation'").fetchone()[0], "2")
        self.db.execute("UPDATE devenv_credential_obligation SET deadline_ms = ? WHERE operation_id='new-operation'", (self.now + 90000,)); self.db.commit()
        self.db.execute("UPDATE tenant_credential_revocation_floor SET revoked_through='2' WHERE tenant_id='tenant'"); self.db.commit()
        self.db.execute(sqls("adoptDevenvOperation")[0], ("new-operation", "tenant", "new-pat"))
        self.assertEqual(self.db.execute("SELECT state FROM devenv_credential_obligation WHERE operation_id='new-operation'").fetchone()[0], "issued")

    def test_activation_generation_must_match_prepared_obligation(self):
        self.prepare(generation="6")
        self.activate(generation="7")
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM pat").fetchone()[0], 0)
        self.assertEqual(self.db.execute("SELECT state FROM devenv_credential_obligation").fetchone()[0], "prepared")

    def test_exact_prepare_replay_does_not_rebind_marker(self):
        self.prepare(generation="3"); self.prepare(generation="3")
        self.prepare(generation="4")
        self.assertEqual(self.db.execute("SELECT lifecycle_generation FROM devenv_credential_obligation WHERE operation_id='operation'").fetchone()[0], "3")


if __name__ == "__main__":
    unittest.main()
