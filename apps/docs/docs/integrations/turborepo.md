---
id: turborepo
title: Turborepo integration
sidebar_position: 2
description: Configure Turborepo to use CoreLink as its remote cache via TURBO_API and TURBO_TOKEN.
---

# Turborepo integration

:::note Turborepo bridge coming soon
The Turborepo-compatible remote cache endpoint is under development in stream 1.4. The configuration shown here describes the expected
setup once it ships. The placeholder URL is `https://corelink-api.humangr.com/turbo/v8`. This page will be updated when the endpoint goes live.
:::

## How it works

Turborepo supports custom remote caches via two environment variables:

- `TURBO_API` — the base URL of the remote cache server.
- `TURBO_TOKEN` — a bearer token passed as `Authorization: Bearer`.

CoreLink exposes a Turborepo-compatible API at `/turbo/v8`. Your tenant ID is encoded in the URL so no separate header is needed.

## Configuration

### Option A: environment variables (recommended for CI)

```bash
export TURBO_API="https://corelink-api.humangr.com/turbo/v8/acme-prod"
export TURBO_TOKEN="clk_live_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
```

Then run Turborepo normally:

```bash
npx turbo run build
```

### Option B: `.turbo/config.json` (per-repo)

```json
{
  "teamId": "acme-prod",
  "apiUrl": "https://corelink-api.humangr.com/turbo/v8"
}
```

With this file in your repo root, Turborepo reads the team ID and API URL automatically. Still set `TURBO_TOKEN` as an environment variable — do not commit the token.

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
    TURBO_API: https://corelink-api.humangr.com/turbo/v8/acme-prod
    TURBO_TOKEN: ${{ secrets.CORELINK_PAT }}
  run: npx turbo run build test
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
# {"tenant_id":"acme-prod","token_prefix":"clk_live","route_kind":"cas"}
```

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| `Remote caching disabled` | `TURBO_TOKEN` not set | Export `TURBO_TOKEN` in your shell or CI env |
| `401` errors in Turborepo output | Wrong or expired PAT | Regenerate PAT from admin dashboard |
| Cache misses on every run | API URL wrong | Verify `TURBO_API` includes `/acme-prod` suffix |
| `teamId` conflict | `.turbo/config.json` teamId differs from URL segment | Keep them in sync |

Full error reference: [Troubleshooting](../troubleshooting.md).
