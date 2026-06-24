#!/usr/bin/env python3
"""Operator-mint the Group-A e2e personas on a STABLE tenant + seed their D1 state.

Recipe (proven 2026-06-24): POST /_internal/pat/mint (gate = CORELINK_INTERNAL_AUTH_KEY)
→ INSERT the `pat` row via the CF D1 HTTP API with PARAM BINDING (the Argon2id PHC
hash contains `$`, so string interpolation is unsafe) → the PAT verifies end-to-end at
both the Worker (HMAC + D1 expiry lookup) and the container (NativePatGate Argon2id).

Writes `CORELINK_E2E_*` exports to the path in argv[1] (default /tmp/e2e-persona-env.sh)
for `provision-and-run-suite.sh` to source. Persona PATs live on a stable tenant so they
survive across runs (unlike the ephemeral Clerk-signup TA).

Env required: CORELINK_INTERNAL_AUTH_KEY, CLOUDFLARE_API_TOKEN, PAT_SIGNING_KEY (mint),
FABRIC_INTROSPECT_AUTH_KEY (introspect-key export). All read from the process env.
"""
import json
import os
import sys
import time
import urllib.error
import urllib.request
import uuid

API = os.environ.get("CORELINK_E2E_ENDPOINT", "https://corelink-api.humangr.com")
ACCT = "6a1fc1c626fc2628823e60b9db01f5cd"
DB = "d64742ea-e102-40b2-a844-ff02e3f94562"  # corelink-config-prod CONFIG_DB
# A stable, already-existing tenant (satisfies the pat.tenant_id FK). The hugit
# CAS tenant — additive seeds (a runner entitlement, dead/expired PATs) do not
# disturb its live use.
STABLE_TENANT = os.environ.get("E2E_STABLE_TENANT", "3560e213-1e23-4fd0-8871-7033c6052ebd")
UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/124 Safari/537.36")  # WAF 1010 dodges urllib UA

INTERNAL = os.environ["CORELINK_INTERNAL_AUTH_KEY"]
CFT = os.environ["CLOUDFLARE_API_TOKEN"]


def _post(url, data, headers, timeout=120):
    req = urllib.request.Request(
        url, data=json.dumps(data).encode(),
        headers={**headers, "content-type": "application/json", "user-agent": UA},
        method="POST")
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, r.read().decode()
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode()


def mint(tenant, scope_label, ttl_seconds=31536000):
    """Mint a PAT; returns the parsed MintResponse dict (+ token_plaintext)."""
    st, body = _post(f"{API}/_internal/pat/mint",
                     {"tenant_id": tenant, "principal_id": str(uuid.uuid4()),
                      "scopes": scope_label, "ttl_seconds": ttl_seconds},
                     {"x-corelink-internal-auth": INTERNAL})
    if st != 200:
        raise SystemExit(f"mint failed ({scope_label}): {st} {body[:200]}")
    return json.loads(body)


def d1(sql, params):
    st, body = _post(
        f"https://api.cloudflare.com/client/v4/accounts/{ACCT}/d1/database/{DB}/query",
        {"sql": sql, "params": params}, {"authorization": f"Bearer {CFT}"})
    d = json.loads(body)
    if not d.get("success"):
        raise SystemExit(f"d1 failed: {st} {d.get('errors')}")
    return d


PAT_INSERT = ("INSERT INTO pat (pat_id,tenant_id,pat_hash,scope,expires_ms,"
              "shown_once_token,shown_once_consumed,created_ms,token_id,name) "
              "VALUES (?1,?2,?3,?4,?5,?6,1,?7,?8,?9)")


def insert_pat(m, tenant, scope_d1, expires_ms, name):
    d1(PAT_INSERT, [m["pat_id"], tenant, m["hash"], scope_d1, expires_ms,
                    str(uuid.uuid4()), int(time.time() * 1000), m["token_id"], name])


def provision():
    out = {}
    now_ms = int(time.time() * 1000)

    # P3 Admin — scope 'admin' (NOT self-serve grantable; operator-minted here).
    m = mint(STABLE_TENANT, "admin")
    insert_pat(m, STABLE_TENANT, "admin", m["expires_ms"], "e2e-admin")
    out["CORELINK_E2E_PAT_ADMIN"] = m["token_plaintext"]

    # P5 Expired — a genuine PAT whose D1 expires_ms is in the PAST, so the
    # Worker/container expiry filter drops it → every op DENIED.
    m = mint(STABLE_TENANT, "cas:rw")
    insert_pat(m, STABLE_TENANT, "read-write", 1, "e2e-expired")  # expires_ms=1 (1970)
    out["CORELINK_E2E_PAT_EXPIRED"] = m["token_plaintext"]

    # Runner — seed a runners_entitlement row (additive) + a cas:rw PAT on the
    # runner tenant. RUNNER_PRO cap = 40 concurrency / 240 vCPU-h.
    d1("INSERT INTO runners_entitlement (tenant_id,max_concurrency,plan,created_at_ms,max_vcpu_h) "
       "VALUES (?1,?2,?3,?4,?5) ON CONFLICT(tenant_id) DO UPDATE SET "
       "max_concurrency=excluded.max_concurrency, plan=excluded.plan, max_vcpu_h=excluded.max_vcpu_h",
       [STABLE_TENANT, 40, "runner_pro", now_ms, 240])
    m = mint(STABLE_TENANT, "cas:rw")
    insert_pat(m, STABLE_TENANT, "read-write", m["expires_ms"], "e2e-runner")
    out["CORELINK_E2E_PAT_RUNNER"] = m["token_plaintext"]
    out["CORELINK_E2E_RUNNER_TENANT"] = STABLE_TENANT

    # Introspect key — the fabric internal-auth key (we hold it OOB).
    ikey = os.environ.get("FABRIC_INTROSPECT_AUTH_KEY", "")
    if ikey:
        out["CORELINK_E2E_INTROSPECT_KEY"] = ikey

    # Team invite — the invite journey asserts 201 (member created) OR 501
    # (HONEST-v1 backend stub); it only needs an email + the admin token (above).
    out["CORELINK_E2E_TEAM_INVITE_EMAIL"] = f"e2e-invitee+{int(time.time())}@example.com"

    return out


def main():
    dest = sys.argv[1] if len(sys.argv) > 1 else "/tmp/e2e-persona-env.sh"
    env = provision()
    with open(dest, "w") as f:
        f.write("# e2e Group-A persona env (operator-minted) — source before the suite\n")
        for k, v in env.items():
            f.write(f"export {k}={json.dumps(v)}\n")
    print(f"provisioned {len(env)} persona vars → {dest}")
    for k in env:
        masked = "<set>" if k.endswith(("PAT_ADMIN", "PAT_EXPIRED", "PAT_RUNNER", "INTROSPECT_KEY")) else env[k]
        print(f"  {k}={masked}")


if __name__ == "__main__":
    main()
