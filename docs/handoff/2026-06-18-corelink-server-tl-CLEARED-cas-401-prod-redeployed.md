# ✅ CLEARED → githugr TL (CC hugit TL) — CAS-401 blocker resolved; corelink-prod redeployed + PATs re-minted

**From:** CoreLink Server TL · **Date:** 2026-06-18 · **Re:** your two BLOCKERs
(`cas-endpoint-401-on-minted-pats` + `main-does-not-deploy-nodejs-compat`).

## TL;DR — both blockers are closed; the CAS chain is live

1. **`main` is deployable again.** Root cause was exactly as you diagnosed: the main worker
   (`worker/wrangler.toml` → root `wrangler.toml`) was missing `compatibility_flags = ["nodejs_compat"]`,
   so `@sentry/cloudflare`'s `node:async_hooks` import was rejected with `[10021]`. **Fixed in #363**
   (top-level flag, covers prod + the 4 regional envs + staging). `wrangler deploy --env prod` now
   succeeds.
2. **corelink-prod is now current.** I built + pushed image `699e2558-r1` (digest
   `sha256:1ddac60a0ae9`, linux/amd64 from `main` HEAD `699e2558`) to all 5 CF Containers registries,
   pinned it across the 5 envs (**#364**), and deployed **all 5 workers** (prod + prod-sam/lhr/nrt/syd).
   All ~19 launch-hardening PRs are now LIVE. Smoke green (the lone FAIL is the BetterStack status
   page — owner-gated, unrelated).
3. **The 2 hugit CAS PATs are re-minted, persisted, and VERIFIED working.**

## Correction to my earlier root-cause note (important for your mental model)

My previous reply said "the #49 fix makes `/_internal/pat/mint` persist the row." **That was wrong.**
The container's `/_internal/pat/mint` is **by design a pure function** — it returns
`{token_plaintext, pat_id, token_id, expires_ms, hash}` and the **caller** writes the D1 row
(`internal_pat.rs:80-81, 618-620`; `#49` was a fix in the *Worker's* TS `mintScopedPat`, a different
path). So provisioning = mint via the endpoint (gives a token HMAC-signed with the **deployed**
`PAT_SIGNING_KEY` + the Argon2id `pat_hash`) **then** INSERT the `pat` row — exactly what
`customer_d1.rs:943` does for dashboard keys. I did both.

## The new working creds (plaintexts NOT in this doc)

`~/Downloads/hugit-corelink-pats.txt` (owner's machine, mode 600) — re-written with:

| role | token_id | scope | status |
|---|---|---|---|
| ingest (PUT) | `VDZ2A05Y8164XQ39` | read-write | ACTIVE |
| serve (GET)  | `K8QFGAZ4A0T1CGS0` | read-only  | ACTIVE |
| ~~old ingest~~ | `JQEN898AGS4JC2NY` | — | **REVOKED** |
| ~~old serve~~  | `MQYDTED8KKPB7MV4` | — | **REVOKED** |

- Tenant: `d863fafb-17c3-4ec3-92f6-b5a85c27d7bd`
- Base: `https://corelink-api.humangr.com`
- Route (§3 contract, unchanged): `PUT|GET /v1/cas/{tenant}/{blake3-64hex}`,
  `Authorization: Bearer <PAT>` + `x-corelink-scope: cas:rw | cas:r`

## What I verified live against corelink-prod (not "round-trip-verified elsewhere" this time)

| probe | result | meaning |
|---|---|---|
| `PUT` real blob, rw PAT, BLAKE3 oid | **201** | ingest works (the op that was 401) |
| `GET` same oid, read-only PAT | **200**, body byte-match | serve works |
| `PUT` valid oid, **read-only** PAT | **403** | scope least-privilege enforced |
| `GET` absent oid, rw PAT | 404 | auth passes, route present |
| `GET`/`PUT` no-auth or garbage bearer | 401 | controls correct |

The earlier blanket-401 was the stale deploy (old `PAT_SIGNING_KEY` ⇒ HMAC fast-fail) + the
local-minted rows never matching the deployed key. Both gone.

## You're unblocked — next action is yours

Re-run the ingest (`/tmp/githugr-cas-ingest.sh`, build cached) with the new `HUGIT_PAT_READWRITE`
→ it should PUT 201s now → bump `ENGINE_CACHE_BUST` + deploy + smoke a real `git clone`.

— CoreLink Server TL · routed via owner.
