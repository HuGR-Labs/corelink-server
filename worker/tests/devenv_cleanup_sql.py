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
db.execute("CREATE TABLE tenant (tenant_id TEXT PRIMARY KEY)")
db.execute("INSERT INTO tenant VALUES ('tenant')")
# Build the exact merged credential schema (0127/0128/0129), with only the
# minimal predecessor tables required by those additive migrations.
signup = (ROOT / "migrations/d1/0037_signup_orchestration.sql").read_text()
db.executescript(re.search(r"CREATE TABLE IF NOT EXISTS pat \([\s\S]+?\n\);", signup).group())
for migration, column in [("0054_pat_token_id.sql", "token_id"), ("0063_pat_customer_keys.sql", "revoked_at_ms")]:
    text = (ROOT / "migrations/d1" / migration).read_text()
    db.execute(re.search(r"ALTER TABLE pat ADD COLUMN " + column + r"[^;]+;", text).group())
db.execute(re.search(r"ALTER TABLE pat ADD COLUMN runner_job_ac_key[^;]+;", (ROOT / "migrations/d1/0086_pat_runner_job_ac_key.sql").read_text()).group())
db.executescript((ROOT / "migrations/d1/0127_devenv_credential_obligation.sql").read_text())
db.executescript((ROOT / "migrations/d1/0128_runner_credential_obligation.sql").read_text())
db.executescript((ROOT / "migrations/d1/0129_credential_lifecycle_generation.sql").read_text())
op, tenant = "operation", "tenant"
now = int(time.time()*1000)
prep = queries("prepareDevenvOperation")[0]
db.execute(prep, (op, tenant, now+90000, "0")); db.commit()
assert db.execute("select state from devenv_credential_obligation").fetchone() == ("prepared",)
act = queries("activateDevenvPat")
with db:
    db.execute(act[0], ("pat", tenant, "hash", "read-write", now+100000, "tok", op, "0"))
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
        db.execute(act[0], ("pat2", tenant, "hash2", "read-write", now+100000, "tok2", op, "0"))
        raise RuntimeError("injected failure")
except RuntimeError: pass
assert db.execute("select count(*) from pat").fetchone()[0] == 0
assert db.execute("select state from devenv_credential_obligation").fetchone() == ("prepared",)
print("")
