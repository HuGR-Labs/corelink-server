---
id: bazel
title: Bazel integration
sidebar_position: 1
description: Configure Bazel to use CoreLink as its remote cache via .bazelrc.
---

# Bazel integration

:::note Bazel bridge coming soon
The Bazel bridge (REAPI v2 gRPC endpoint) is under development in stream 1.3. The `.bazelrc` lines shown below use the placeholder URL
`https://corelink-api.humangr.com/bazel/v2`. This page will be updated with the final URL once the bridge ships. The REST CAS endpoint
(`https://corelink-api.humangr.com/v1/cas/...`) is live today.
:::

## Prerequisites

- Bazel 6.0 or later (supports `--remote_header` natively).
- A CoreLink PAT with `cas:read cas:write ac:read ac:write` scopes. See [PAT creation](../concepts/tenancy.md).

## Configure `.bazelrc`

Add these lines to your project's `.bazelrc`:

```text
# CoreLink remote cache
build --remote_cache=https://corelink-api.humangr.com/bazel/v2
build --remote_header=x-corelink-tenant=<YOUR_TENANT_ID>
build --remote_header=authorization=Bearer <YOUR_PAT>
build --remote_upload_local_results=true
build --remote_timeout=60
```

Replace `<YOUR_TENANT_ID>` with your tenant ID (e.g. `acme-prod`) and `<YOUR_PAT>` with a PAT. In CI, pass the PAT via an environment variable:

```text
# .bazelrc — CI-safe variant (no literal secrets)
build --remote_cache=https://corelink-api.humangr.com/bazel/v2
build --remote_header=x-corelink-tenant=acme-prod
build --remote_header=authorization=Bearer ${CORELINK_PAT}
build --remote_upload_local_results=true
build --remote_timeout=60
```

## Optional: separate upload vs. download PATs

If your security model requires separate credentials for read-only (dev machines) and read-write (CI), create two PATs:

```text
# Developer machines — read-only
build:dev --remote_cache=https://corelink-api.humangr.com/bazel/v2
build:dev --remote_header=x-corelink-tenant=acme-prod
build:dev --remote_header=authorization=Bearer ${CORELINK_PAT_DEV}
build:dev --remote_upload_local_results=false

# CI — read + write
build:ci --remote_cache=https://corelink-api.humangr.com/bazel/v2
build:ci --remote_header=x-corelink-tenant=acme-prod
build:ci --remote_header=authorization=Bearer ${CORELINK_PAT_CI}
build:ci --remote_upload_local_results=true
```

Invoke with `bazel build --config=ci //...` in CI and `--config=dev` on developer machines.

## Verify it worked

After running a build, verify the PAT and tenant are recognized:

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"acme-prod","token_prefix":"clk_live","route_kind":"cas"}
```

For a cache-hit check, run the same build twice. The second run should report cache hits in Bazel's output:

```text
INFO: Build completed successfully, 42 total actions, 42 remote-cache-hit actions.
```

Once the Bazel bridge ships, you will also see CoreLink-side hit logs in the admin dashboard under **Audit** → **Cache events**.

## Troubleshooting Bazel-specific issues

| Symptom | Likely cause | Fix |
|---|---|---|
| `Error: remote_cache: UNAUTHENTICATED` | Missing or wrong `authorization` header | Verify `CORELINK_PAT` is exported in your shell / CI env |
| `Error: remote_cache: PERMISSION_DENIED` | Tenant mismatch | Check `x-corelink-tenant` matches your PAT's tenant |
| Cache miss on every build | `--remote_upload_local_results=false` | Set to `true` in at least one CI job |
| TLS handshake failure | Bazel version < 6 | Upgrade to Bazel 6+ for `--remote_header` support |

Full error reference: [Troubleshooting](../troubleshooting.md).
