---
id: turborepo
title: Turborepo integration
sidebar_position: 2
description: Configure Turborepo to use CoreLink as its remote cache via TURBO_API and TURBO_TOKEN.
---

# Turborepo integration

CoreLink implements the Vercel Remote Cache `/v8/artifacts` protocol, so
Turborepo can use CoreLink as a drop-in replacement for Vercel's remote cache.

## How it works

Turborepo supports custom remote caches via two environment variables:

- `TURBO_API` — the **bare origin** of the remote cache server. Turborepo
  appends its own `/v8/artifacts/...` path — do **not** add any path or tenant
  segment yourself.
- `TURBO_TOKEN` — your CoreLink PAT, passed as `Authorization: Bearer`.

Your tenant is resolved from the PAT, **not** from the URL. The `teamId`
Turborepo sends is treated as a logical sub-namespace *within* your
authenticated tenant (teams under one tenant stay partitioned) — it is not a
security boundary and does not appear in the base URL.

## Configuration

### Option A: Environment variables (recommended for CI)

```bash
export TURBO_API="https://corelink-api.humangr.com"
export TURBO_TOKEN="corelink_pat_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
```

Then run Turborepo normally (pass a `--team` label so Turborepo enables remote
caching):

```bash
npx turbo run build --team=acme --token="$TURBO_TOKEN"
```

### Option B: `.turbo/config.json` (per-repo)

```json
{
  "teamId": "acme",
  "apiUrl": "https://corelink-api.humangr.com"
}
```

With this file in your repo root, Turborepo reads the team label and API URL automatically. Still set `TURBO_TOKEN` as an environment variable — do not commit the token.

### Option C: `turbo.json` remote config

```json
{
  "$schema": "https://turbo.build/schema.json",
  "remoteCache": {
    "enabled": true
  }
}
```

This enables remote caching; the URL and token come from environment variables.

## GitHub Actions example

```yaml
- name: Build with Turborepo + CoreLink cache
  env:
    TURBO_API: https://corelink-api.humangr.com
    TURBO_TOKEN: ${{ secrets.CORELINK_PAT }}
  run: npx turbo run build test --team=acme --token="$TURBO_TOKEN"
```

Store the PAT in `Settings → Secrets and variables → Actions` as `CORELINK_PAT`.

## Verify it worked

After configuring, run your pipeline twice. On the second run, Turborepo should report remote cache hits:

```text
• Packages in scope: web, api, shared
• Running build in 3 packages
• Remote caching enabled

web:build  cache hit, replaying output...  0.8s
api:build  cache hit, replaying output...  0.6s
shared:build  cache hit, replaying output...  0.3s
```

You can also confirm the token is valid:

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `Remote caching disabled` | `TURBO_TOKEN` not set | Export `TURBO_TOKEN` in your shell or CI env |
| `401` errors in Turborepo output | Wrong or expired PAT | Regenerate PAT from admin dashboard |
| Cache misses on every run | `TURBO_API` has an extra path segment | `TURBO_API` must be the **bare origin** `https://corelink-api.humangr.com` — no `/turbo`, `/v8`, or tenant suffix |
| `400 Bad Request` on artifact PUT/GET | Missing team label | Pass `--team=<label>` (or set `teamId` in `.turbo/config.json`) |

Full error reference: [Troubleshooting](../troubleshooting.md).
