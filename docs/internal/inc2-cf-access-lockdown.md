# Inc-2 — `/_internal/*` CF Access lockdown (as-built runbook)

**Live since:** 2026-08-19. **Design:** `docs/design/2026-08-19-internal-control-plane-lockdown.md`.
**Status:** ENFORCING in prod. Public `/_internal/*` is gated at the network layer by
Cloudflare Access; the app-layer `x-corelink-internal-auth` per-consumer gate stays underneath.

## What is deployed

- **CF Access app** (self-hosted): `id a9968a17-bc72-45e0-85b8-5f8a81a15946`, domain
  `corelink-api.humangr.com/_internal` (covers `/_internal/*`). Account `6a1fc1c626fc2628823e60b9db01f5cd`.
- **Policy:** `internal-service-tokens`, decision `non_identity` (Service Auth), includes service tokens
  `corelink-internal-operator` + `corelink-internal-githugr` (githugr parked/discontinued but left in the
  include list, harmless).
- **Service tokens** (Zero Trust → Access → Service Auth): `corelink-internal-{operator,githugr,clw,runners}`.
  clw+runners are PARKED for **Inc-3** (the no-underscore `/internal/v1/*` family), not used by this app.

## How to call `/_internal/*` now (operators / automation)

Send BOTH the CF Access service-token headers AND the app-layer key:

```
curl https://corelink-api.humangr.com/_internal/... \
  -H "CF-Access-Client-Id: <operator client id>" \
  -H "CF-Access-Client-Secret: <operator client secret>" \
  -H "x-corelink-internal-auth: <the per-consumer or shared key>"
```

Without the CF Access headers → **403 at the edge** (never reaches the Worker).
The operator token's Client-Id/Secret live in the operator's `.env.local` as
`CF_ACCESS_OPERATOR_CLIENT_ID` / `CF_ACCESS_OPERATOR_CLIENT_SECRET` (gitignored).

## Who is unaffected (and why)

Same-account **Service Binding** callers do NOT traverse the public edge, so CF Access does not apply:

- signup-worker `/_internal/pat/mint` (binding `CORELINK_API_SVC`).
- signup-worker DSR erase / verify / audit-drain crons (prefer `CORELINK_API_SVC`, `webhooks/*_cron.ts`,
  `webhooks/dsr_consumer.ts`).
- The main Worker's DSR erase **fan-out** to regional workers (`PROD_{LHR,SAM,NRT,SYD}` bindings,
  `worker/src/index.ts` ~1999).

Verified: there is NO public `https://corelink-api.humangr.com/_internal/*` fetch anywhere in `worker/src`.

## Proof (2026-08-19)

- Public `/_internal/tenant/<t>/quota` with the quota key but NO CF token → **403** (was 404).
- Same + operator `CF-Access-*` headers → **404** (reaches the Worker; legit).
- `/_internal/admin/public-mirror/promote` + operator CF token + internal key → **200 promoted** (F3.2 mirror
  intact).

## Rollback

Instant, no deploy: `DELETE /accounts/6a1fc1c6…/access/apps/a9968a17-bc72-45e0-85b8-5f8a81a15946`
(re-opens the public `/_internal/*` path). Or remove the policy to fail-open.

## Not covered (Inc-3)

The no-underscore `/internal/v1/*` family — `runner/{mint,revoke}`, `auth/{introspect,resolve-tenant,
tenant/lookup,token-exchange,rotate}`, `billing/usage` — is a SEPARATE public surface (worker-handled exact
routes, some with NO edge gate today, e.g. `fabric_introspect`). Same CF Access mechanism; tokens
`corelink-internal-{clw,runners}` are pre-minted for it. Track as Inc-3.
