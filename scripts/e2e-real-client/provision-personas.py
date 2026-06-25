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


def _get(url, headers, timeout=30):
    req = urllib.request.Request(url, headers={**headers, "user-agent": UA})
    try:
        with urllib.request.urlopen(req, timeout=timeout) as r:
            return r.status, r.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()


def discover_brew_path(formula="jq"):
    """Discover a LIVE, digest-addressed public Homebrew bottle path from ghcr.io.

    ghcr digests rot as Homebrew bumps versions, so we resolve one at provision
    time instead of hardcoding: anon pull-token → highest version tag → first
    per-platform manifest → its single bottle layer digest. The brew adapter
    serves `blobs/sha256:<digest>` by fetching that exact blob from ghcr, so
    sha256(served-bytes) == the URL digest by construction (the journey's HARD #1).
    Returns the `v2/homebrew/core/<formula>/blobs/sha256:<64hex>` path, or None.
    """
    try:
        _, b = _get(f"https://ghcr.io/token?scope=repository:homebrew/core/{formula}:pull", {})
        tok = json.loads(b)["token"]
        auth = {"authorization": f"Bearer {tok}"}
        _, b = _get(f"https://ghcr.io/v2/homebrew/core/{formula}/tags/list", auth)
        tags = [t for t in json.loads(b).get("tags", []) if t != "latest"]
        if not tags:
            return None
        tag = tags[-1]  # tags/list is lexically sorted; the last is the newest version
        _, b = _get(f"https://ghcr.io/v2/homebrew/core/{formula}/manifests/{tag}",
                    {**auth, "accept": "application/vnd.oci.image.index.v1+json"})
        mans = json.loads(b).get("manifests", [])
        if not mans:
            return None
        _, b = _get(f"https://ghcr.io/v2/homebrew/core/{formula}/manifests/{mans[0]['digest']}",
                    {**auth, "accept": "application/vnd.oci.image.manifest.v1+json"})
        layers = json.loads(b).get("layers", [])
        if not layers:
            return None
        digest = layers[0]["digest"].split(":", 1)[1]
        return f"v2/homebrew/core/{formula}/blobs/sha256:{digest}"
    except Exception:
        return None


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


def create_tenant(region="enam"):
    """Create a brand-new throwaway tenant row (satisfies the pat/quota FKs).

    region is constrained to the residency set; ENAM is the US R2 region (all
    CoreLink CAS buckets are ENAM). Returns the new tenant_id (a fresh UUID).
    """
    tid = str(uuid.uuid4())
    now = int(time.time() * 1000)
    d1("INSERT OR IGNORE INTO tenant (tenant_id,primary_region,created_at_ms,updated_at_ms) "
       "VALUES (?1,?2,?3,?3)", [tid, region, now])
    return tid


def mint_pat_on(tenant, scope_label, scope_d1, name, expires_ms=None):
    """Mint a PAT on `tenant` + write its D1 row; return the plaintext token."""
    m = mint(tenant, scope_label)
    insert_pat(m, tenant, scope_d1, m["expires_ms"] if expires_ms is None else expires_ms, name)
    return m["token_plaintext"]


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

    # DSR tombstone — a STABLE, already-erased content address proving the GDPR
    # 410-Gone tombstone gate. Provisioned once (2026-06-25) end-to-end: PUT a
    # throwaway blob → seed a `dsr_requested` row → POST /_internal/cas/:t/:h/erase
    # → the `cas_tombstone` D1 row (migration 0067) is durable, so a GET returns
    # 410 Gone forever. Re-using the known hash avoids per-run mutation (a fresh
    # erase each run would accumulate tombstone rows). The erase legitimacy gate
    # (rt-nuclear #18/#19) binds the erase to that live `dsr_requested` row, so a
    # leaked internal key alone cannot erase arbitrary blobs.
    out["CORELINK_E2E_TOMBSTONED_HASH"] = (
        "8ef09de3e5b0d59b90942f7b3e77653e6f6ac6e7630611eeebabc04bab299917")

    # Shared-cache `_public` dedup HIT (M2 moat) — a live, digest-addressed public
    # Homebrew bottle path (resolved fresh from ghcr so it never rots). The brew
    # adapter warms it from ghcr into the cross-tenant `_public` namespace, so
    # tenant B can serve byte-identical bytes (the COGS/margin proof).
    brew_path = discover_brew_path()
    if brew_path:
        out["CORELINK_E2E_PUBLIC_BREW_PATH"] = brew_path

    # ── Dedicated throwaway tenants (Group C / dedicated-tenant wave) ─────────
    # These are SEPARATE from the stable hugit tenant so the destructive drives
    # (cap-drive, $0-ceiling deny) never touch a live tenant.

    # Quota tenant — a low $-ceiling (10000 micros = $0.01 ≈ 10 ops at the
    # default 1000-micros/op flat cost) with accrued=0, RE-SEEDED each run so the
    # headroom is deterministic: the two under-cap probes serve, then the bounded
    # cap-drive trips a clean 402. (The journey reorders so the under-cap-needing
    # cells run before quota_hard_cap, which shares the monotonic accrual.)
    qt = create_tenant()
    d1("INSERT INTO tenant_quota (tenant_id,monthly_budget_usd_micros,accrued_usd_micros,"
       "cycle_anchor_ms,updated_at_ms) VALUES (?1,?2,0,?3,?3) ON CONFLICT(tenant_id) DO UPDATE SET "
       "monthly_budget_usd_micros=excluded.monthly_budget_usd_micros, accrued_usd_micros=0, "
       "cycle_anchor_ms=excluded.cycle_anchor_ms, updated_at_ms=excluded.updated_at_ms",
       [qt, 10000, now_ms])
    out["CORELINK_E2E_QUOTA_TENANT"] = qt
    out["CORELINK_E2E_PAT_QUOTA"] = mint_pat_on(qt, "cas:rw", "read-write", "e2e-quota")

    # Past-due tenant — billing status='past_due' + a $0 ceiling, so a P11
    # data-plane write is DENIED (402) by the billing/$-ceiling gate ON THAT
    # tenant (a genuine billing-state-integrity proof, not a cross-tenant 403).
    pt = create_tenant()
    d1("INSERT INTO tenant_quota (tenant_id,monthly_budget_usd_micros,accrued_usd_micros,"
       "cycle_anchor_ms,updated_at_ms) VALUES (?1,0,0,?2,?2) ON CONFLICT(tenant_id) DO UPDATE SET "
       "monthly_budget_usd_micros=0, accrued_usd_micros=0, updated_at_ms=excluded.updated_at_ms",
       [pt, now_ms])
    d1("INSERT INTO tenant_billing (tenant_id,status,schema_version,created_at_ms,updated_at_ms) "
       "VALUES (?1,'past_due',1,?2,?2) ON CONFLICT(tenant_id) DO UPDATE SET "
       "status='past_due', updated_at_ms=excluded.updated_at_ms", [pt, now_ms])
    out["CORELINK_E2E_PASTDUE_TENANT"] = pt
    out["CORELINK_E2E_PAT_PASTDUE"] = mint_pat_on(pt, "cas:rw", "read-write", "e2e-pastdue")

    # Fresh tenant — a brand-new tenant with NO tenant_storage_state / tenant_quota
    # row, so its FIRST cargo PUT exercises the cap-seed auto-seed path (must serve
    # 200/201, never the old fail-closed 502). A new UUID each run keeps it fresh.
    ft = create_tenant()
    out["CORELINK_E2E_FRESH_TENANT"] = ft
    out["CORELINK_E2E_PAT_FRESH"] = mint_pat_on(ft, "cas:rw", "read-write", "e2e-fresh")

    # Signup-worker endpoint — the LIVE Stripe webhook receiver. The unsigned-
    # webhook DENY journey (#9) POSTs an UNSIGNED forged event here and asserts a
    # 400 (the signature gate rejects BEFORE any side effect) — safe: no secret,
    # no mutation. The mutating SIGNED-webhook journeys stay gated (they need the
    # live whsec + would move real billing state — driven only via a local
    # signup-worker harness, never injected into prod).
    out["CORELINK_E2E_SIGNUP_WORKER_ENDPOINT"] = os.environ.get(
        "E2E_SIGNUP_WORKER_ENDPOINT", "https://corelink-signup.humangr.com")

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
        masked = "<set>" if "PAT_" in k or k.endswith("INTROSPECT_KEY") else env[k]
        print(f"  {k}={masked}")


if __name__ == "__main__":
    main()
