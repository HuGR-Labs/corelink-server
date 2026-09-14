"""Exercise the cleanup SQL in SQLite, including transaction rollback and fences."""
from pathlib import Path
import re, sqlite3, time

ROOT = Path(__file__).resolve().parents[2]
SRC = (ROOT / "worker/src/lib/devenv_cleanup.ts").read_text()
NOW = "CAST(strftime('%s','now') AS INTEGER) * 1000"
GEN = "length(lifecycle_generation) BETWEEN 1 AND 19 AND lifecycle_generation NOT GLOB '*[^0-9]*' AND (length(lifecycle_generation) = 1 OR substr(lifecycle_generation, 1, 1) <> '0') AND (length(lifecycle_generation) < 19 OR lifecycle_generation <= '9223372036854775807')"
def queries(fn):
    body = SRC.split("export async function " + fn + "(", 1)[1].split("export async function ", 1)[0]
    out=[]
    for a,b in re.findall(r'db\.prepare\(\s*(?:`([^`]+)`|"([^"\n]+)")', body): out.append((a or b).replace("${NOW}", NOW).replace("${GENERATION}", GEN))
    return out

db = sqlite3.connect(":memory:")
db.executescript("""
CREATE TABLE pat (pat_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, pat_hash TEXT NOT NULL, scope TEXT, expires_ms INTEGER, token_id TEXT, shown_once_token TEXT, shown_once_consumed INTEGER, created_ms INTEGER, revoked_at_ms INTEGER, lifecycle_generation TEXT);
CREATE TABLE devenv_credential_obligation (operation_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL, state TEXT NOT NULL, deadline_ms INTEGER NOT NULL, pat_id TEXT, token_id TEXT, lifecycle_generation TEXT NOT NULL DEFAULT '0');
CREATE TABLE tenant_credential_revocation_floor (tenant_id TEXT PRIMARY KEY, revoked_through TEXT NOT NULL);
""")
op, tenant = "operation", "tenant"
now = int(time.time()*1000)
prep = queries("prepareDevenvOperation")[1]
db.execute(prep, (op, tenant, now+90000, "0")); db.commit()
assert db.execute("select state from devenv_credential_obligation").fetchone() == ("prepared",)
act = queries("activateDevenvPat")
with db:
    db.execute(act[0], ("pat", tenant, "hash", "read", now+100000, "tok", op, "0"))
    db.execute(act[1], ("pat", "tok", op, tenant, "0"))
assert db.execute("select state from devenv_credential_obligation").fetchone() == ("issued",)
with db:
    for q in queries("revokeDevenvOperation")[:3]: db.execute(q, (op, tenant))
assert db.execute("select revoked_at_ms from pat").fetchone()[0] is not None
assert db.execute("select state from devenv_credential_obligation").fetchone() == ("revoking",)
# A failed activation batch must leave both sides unchanged (SQLite transaction analogue).
db.execute("delete from pat"); db.execute("delete from devenv_credential_obligation"); db.commit()
db.execute(prep, (op, tenant, now+90000, "0")); db.commit()
try:
    with db:
        db.execute(act[0], ("pat2", tenant, "hash2", "read", now+100000, "tok2", op, "0"))
        raise RuntimeError("injected failure")
except RuntimeError: pass
assert db.execute("select count(*) from pat").fetchone()[0] == 0
assert db.execute("select state from devenv_credential_obligation").fetchone() == ("prepared",)
print("")
