# Inc-3 — `/internal/v1/*` CF Access lockdown (as-built runbook)

**Live since:** 2026-08-19. **Sibling:** Inc-2 (`docs/internal/inc2-cf-access-lockdown.md`) locks the
underscore `/_internal/*` family; this increment locks the no-underscore `/internal/v1/*` control-plane
family (runner mint/revoke, auth introspect/rotate, billing usage). **Status:** ENFORCING in prod. The
app-layer `x-corelink-internal-auth` per-consumer gate stays underneath the network gate.

## What is deployed

- **CF Access app** (self-hosted): `id 83664833-1538-45f2-9aae-bdc1bd5b3fda`, domain
  `corelink-api.humangr.com/internal/v1` (covers `/internal/v1/*`). Account `6a1fc1c626fc2628823e60b9db01f5cd`.
- **Policy:** `internal-service-tokens` (`id 3800cb5c-4204-426a-9634-6acdd203617a`), decision
  `non_identity` (Service Auth), includes service tokens `corelink-internal-runners` (the fabricd caller)
  + `corelink-internal-operator` (manual operator calls).
- **Caller wiring (corelink-runners):** the fabric server (`corelink-fabric-server`) crate `cf_access`
  attaches `CF-Access-Client-Id`/`CF-Access-Client-Secret` from env `CORELINK_CF_ACCESS_CLIENT_ID`/`_SECRET`
  on every `/internal/v1/*` request (introspect, mint, revoke, billing). Those secrets are set on the
  `corelink-fabricd` Worker (`wrangler secret put`) and forwarded into the FabricdContainer DO via
  `deploy/cloudflare-fabricd/src/index.ts` `envVars`. The container image was repinned to
  `@sha256:db3b03ef…` (a new image digest is what rolls the container so it picks up the freshly-set
  secrets + env-forward + the `cf_access` code).

## How to call `/internal/v1/*` now (operators / automation)

Send BOTH the CF Access service-token headers AND the app-layer key:

```
curl https://corelink-api.humangr.com/internal/v1/... \
  -H "CF-Access-Client-Id: <operator or runners client id>" \
  -H "CF-Access-Client-Secret: <operator or runners client secret>" \
  -H "x-corelink-internal-auth: <the per-consumer key>"
```

Without the CF Access headers → **403 at the edge** (never reaches the Worker). The operator token's
Client-Id/Secret live in the operator's `.env.local` as `CF_ACCESS_OPERATOR_CLIENT_ID` /
`CF_ACCESS_OPERATOR_CLIENT_SECRET` (gitignored).

## Who is unaffected

Only the fabricd (`corelink-fabric-server`) is a live external caller of `corelink-api.humangr.com/internal/v1/*`.
The clw CLI does not call it at runtime (mint/rotate are consumed dispatcher-side = fabricd). githugr is
discontinued. The fabricd's OWN observability endpoints (`/internal/v1/status`, `/internal/v1/occupancy`)
are served on the fabricd's own domain, not `corelink-api`, so they are not gated by this app.

## Verification (prove-by-use, 2026-08-19)

All green against prod:

1. fabricd path, NO CF token → **403** (Cloudflare Access edge block).
2. Replicated fabricd's exact request (internal key + runners CF-Access headers) → **200 `{"valid":false}`**.
3. Operator token + internal key → **200**.
4. `/internal/v1/billing/usage` no CF token → **403**.
5. `/internal/v1/runner/mint` no CF token → **403**.
6. fabricd `/v1/health` = **200** post-cutover (operational, not stranded).

Any future token-path regression fails LOUD at the fabricd boot introspect self-check (FATAL-on-rejected),
never a silent strand.

## Resolving the fabricd image `@sha256` digest (out-of-band)

`build-fabricd-image.yml` sometimes cannot resolve the pushed `@sha256` at push time (imagetools empty).
Resolve it from the CF managed registry:

```
# 1. mint a pull credential (CLOUDFLARE_API_TOKEN needs Containers read)
curl -X POST \
  https://api.cloudflare.com/client/v4/accounts/<acct>/containers/registries/registry.cloudflare.com/credentials \
  -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" -H 'Content-Type: application/json' \
  --data '{"permissions":["pull"],"expiration_minutes":15}'
# → { username: "v1", password: "<jwt>" }

# 2. read the tag's manifest digest (Docker-Content-Digest header)
curl -sSI -u "v1:<jwt>" \
  -H 'Accept: application/vnd.oci.image.manifest.v1+json' \
  https://registry.cloudflare.com/v2/<acct>/<name>/manifests/<tag>
```

## Rollback

Instant, reversible: delete the CF Access app (re-opens `/internal/v1/*` to the app-layer key alone).

```
curl -X DELETE \
  https://api.cloudflare.com/client/v4/accounts/6a1fc1c626fc2628823e60b9db01f5cd/access/apps/83664833-1538-45f2-9aae-bdc1bd5b3fda \
  -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN"
```
