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

:::tip Two ways to point Bazel at CoreLink — both live
- **Stock plain-HTTP remote cache (simplest):** point `--remote_cache` at the
  **stock-HTTP alias** `https://corelink-api.humangr.com/bazel/cache` — it serves the
  `/cas/<sha256>` and `/ac/<sha256>` paths stock Bazel emits (`PUT`→`204`, `GET`→`200`).
  No REAPI client needed.
- **REAPI v2 / ByteStream:** point `--remote_cache` at `/bazel/v2` (this doc's config)
  for a ByteStream-compatible client.

Both are Bearer-PAT authenticated. (Bazel content-addresses by SHA-256, which the
`/bazel/*` routes accept; the *native* REST CAS at `/v1/cas/...` is BLAKE3-keyed — see
[Raw HTTP (curl)](./raw-curl).)
:::

## Prerequisites

- Stock Bazel (for the `/bazel/cache` alias) or a REAPI/ByteStream-compatible
  client (for `/bazel/v2`). Either works.
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
| Every request 404s | `--remote_cache` points at the wrong prefix | Use `/bazel/cache` (stock plain-HTTP) or `/bazel/v2` (REAPI/ByteStream) — both live; a bare host or `/v1/cas` will 404 for Bazel's paths |
| `UNAUTHENTICATED` | Missing or wrong `Authorization` header | Verify `CORELINK_PAT` is exported in your shell / CI env |
| `PERMISSION_DENIED` / 403 | Instance name is not your tenant | Set `--remote_instance_name` to your tenant UUID |
| Cache miss on every build | `--remote_upload_local_results=false` | Set to `true` in at least one CI job |

Full error reference: [Troubleshooting](../troubleshooting.md).
