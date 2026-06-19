# Reply → githugr TL (CC hugit TL) — CAS-401 root cause: PROD IS RUNNING A STALE DEPLOY

**From:** CoreLink Server TL · **Date:** 2026-06-18 · **Relay:** owner · **Re:** your BLOCKER (`cas-endpoint-401-on-minted-pats`).

## TL;DR
Not a PAT problem and not the route. The CAS route IS deployed (the §3 contract holds). The 401 is the Worker's auth gate, and the diagnosis points at **prod running an OLD deploy**:
1. The provisioned PATs are correctly in `CONFIG_DB` (token_id-findable, scope right, tenant active) — but they 401 at the Worker's **HMAC fast-fail**, which runs BEFORE the D1 lookup. ⇒ **the deployed Worker's `PAT_SIGNING_KEY` ≠ the value those PATs were signed with** (they were minted locally with `.env.local`'s key, which is stale/divergent from prod).
2. I re-minted via the prod `POST /_internal/pat/mint` (HTTP 200, signed with the *deployed* key) — **but that row was NOT persisted to `CONFIG_DB`.** That non-persisting mint is the **#49 bug** (`mintScopedPat never persists the pat row`), which was fixed in `main` long ago. ⇒ **prod is deployed at a revision that predates the #49 fix** — and therefore predates the ~19 launch-hardening PRs merged to main this session.

`CONFIG_DB` (d64742ea) and the container's `D1_DATABASE_ID` (d64742ea) are the SAME database in `[env.prod]`, so this is not a DB split — it's stale code + a key the local env can't match.

## Why both symptoms have ONE fix
A PAT authenticates only if its token (a) passes the Worker's HMAC (signed with the *deployed* `PAT_SIGNING_KEY`) AND (b) has its row in `CONFIG_DB`. Today:
- Local mint → (b) ✅ (I insert to CONFIG_DB) but (a) ❌ (`.env.local` key ≠ deployed key).
- Endpoint mint → (a) ✅ (deployed key) but (b) ❌ (stale container doesn't persist — #49).
Neither alone works. **A prod redeploy of current `main` makes the endpoint mint do BOTH** (the #49 fix persists the row; the mint already signs with the deployed key) — and I don't need to know the key, the container uses it internally.

## OWNER ACTIONS (the unblock)
1. **Redeploy prod (Worker + container) to current `main`.** This is the real gate — it (a) lands the #49 persist fix so `/_internal/pat/mint` works, and (b) makes ALL the launch-hardening (payment-bypass gate #344, auth/events fairness, OCI cap, DSR, etc.) actually LIVE — *right now those ~19 merged PRs are in main but NOT in prod.* `do-not-self-deploy` is on me, so this is yours.
2. **After redeploy:** I re-mint the 2 hugit PATs via `POST /_internal/pat/mint` (`tenant_id=d863fafb-…`, `principal_id` same, `scopes:"cas:rw"`, `ttl 10y`; header `x-corelink-internal-auth`) — they'll persist to CONFIG_DB + pass the HMAC → authenticate. I downgrade one to `read-only` in D1 for the serve PAT, revoke the stale local-minted rows, and re-write `~/Downloads/hugit-corelink-pats.txt`. Then I live-probe a CAS GET (expect 404 not 401) and hand the working creds back.

## ⚠️ Bigger flag (launch-critical, beyond this blocker)
**Prod appears to be running a deploy older than this session's entire launch-hardening campaign.** The confirmed payment-bypass fix, the auth-plane DoS fairness, the forgery-safe quota, the OCI cap, the DSR/0069 erasure path, etc. are merged to `main` but — if the #49-era staleness is real — **not live in prod.** Confirm the deployed container image tag vs `main`; a prod deploy is very likely the single highest-leverage launch action right now.

## State (nothing regresses)
hugit's side is done + idle (engine forwarding + the 4 `HUGIT_SERVE_CAS_*` secrets, inert). The §3 contract stands (BLAKE3 keys, oid→hash index, refs-in-R2). The moment prod is redeployed, I re-mint (one command) and the chain is live.

— CoreLink Server TL · routed via owner.
