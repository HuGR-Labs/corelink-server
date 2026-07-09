---
id: bazel
title: Bazel integration
sidebar_position: 1
description: Configure Bazel to use CoreLink as its remote cache via .bazelrc.
---

# Bazel integration

CoreLink implements the **Bazel Remote Execution API v2 (REAPI v2)** cache as a
ByteStream REST scheme:

```text
https://corelink-api.humangr.com/bazel/v2/<your-tenant-id>/blobs/<hash>/<size>
```

The `<instance>` path segment is your tenant UUID.

:::warning Native `bazel --remote_cache` support: in progress
Stock Bazel's plain-HTTP remote cache emits `/cas/<hash>` and `/ac/<hash>`
requests, which do **not** match CoreLink's ByteStream scheme and currently
return **404**. A stock-HTTP alias is being built and is not yet live. Until it
ships, use a **REAPI/ByteStream-compatible client** against the
`/bazel/v2/<tenant>` endpoint above. The REST native CAS endpoint
(`https://corelink-api.humangr.com/v1/cas/...`) is live today for direct HTTP
use — see [Raw HTTP (curl)](./raw-curl).
:::

## Prerequisites

- A REAPI/ByteStream-compatible Bazel client.
- A CoreLink PAT (`corelink_pat_...`) with cache read + write scope. See [PAT creation](../concepts/tenancy.md).

## Configure `.bazelrc`

The repo ships a committed reference config at
[`apps/examples/bazel/.bazelrc`](https://github.com/HumanGuardrail/corelink-server/blob/main/apps/examples/bazel/.bazelrc).
It points Bazel's REAPI instance at your tenant:

```ini
# Point at the CoreLink REAPI v2 endpoint (the /bazel/v2 prefix is required).
build --remote_cache=https://corelink-api.humangr.com/bazel/v2

# Your tenant UUID becomes the REAPI :instance path segment.
build --remote_instance_name=${CORELINK_TENANT}

# Authenticate with your PAT.
build --remote_header=Authorization=Bearer ${CORELINK_PAT}

build --remote_upload_local_results=true
build --remote_timeout=60
```

Export both values before building; in CI pass the PAT from a secret so it never
appears literally:

```bash
export CORELINK_PAT="corelink_pat_XXXXXXXXXXXX"   # ${{ secrets.CORELINK_PAT }} in CI
export CORELINK_TENANT="acme-prod"
```

## Verify it worked

After running a build, verify the PAT and tenant are recognized:

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

For a cache-hit check, run the same build twice. Inspect Bazel's execution log
(`--execution_log_json_file`) for `remoteCacheHit: true` entries on the second
run.

## Troubleshooting Bazel-specific issues

| Symptom | Likely cause | Fix |
|---|---|---|
| Every request 404s | Using stock plain-HTTP `--remote_cache` | Native `--remote_cache` is not yet live; use a REAPI/ByteStream client against `/bazel/v2/<tenant>` |
| `UNAUTHENTICATED` | Missing or wrong `Authorization` header | Verify `CORELINK_PAT` is exported in your shell / CI env |
| `PERMISSION_DENIED` / 403 | Instance name is not your tenant | Set `--remote_instance_name` to your tenant UUID |
| Cache miss on every build | `--remote_upload_local_results=false` | Set to `true` in at least one CI job |

Full error reference: [Troubleshooting](../troubleshooting.md).
