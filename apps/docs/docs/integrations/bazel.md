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
- **REAPI v2 / ByteStream:** point a ByteStream-compatible client at `/bazel/v2`
  and include the tenant instance segment in its blob paths.

Both are Bearer-PAT authenticated. (Bazel content-addresses by SHA-256, which the
`/bazel/*` routes accept; the *native* REST CAS at `/v1/cas/...` is BLAKE3-keyed — see
[Raw HTTP (curl)](./raw-curl).)
:::

## Prerequisites

- Stock Bazel (for the `/bazel/cache` alias) or a REAPI/ByteStream-compatible
  client (for `/bazel/v2`). Either works.
- A CoreLink PAT (`corelink_pat_0123456789ABCDEF.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.BBBBBBBBBBBBBBBBBBBBBA`) with cache read + write scope. See [PAT creation](../concepts/tenancy.md).

## Configure `.bazelrc`

Here is a reference `.bazelrc` config for stock Bazel's plain-HTTP cache
client. The `/bazel/cache` prefix is the registered stock-Bazel alias; stock
Bazel appends `/cas/<sha256>` and `/ac/<sha256>` to it:

```ini
# Point stock Bazel at the registered plain-HTTP cache alias.
build --remote_cache=https://corelink-api.humangr.com/bazel/cache

# Authenticate with your PAT through Bazel 6+'s host-scoped credential helper.
build --credential_helper=corelink-api.humangr.com=%workspace%/.bazel/corelink-credential-helper.sh

build --remote_upload_local_results=true
build --remote_timeout=60
```

Download the helper from the [Bazel starter example](pathname:///downloads/corelink-credential-helper.sh)
to `.bazel/corelink-credential-helper.sh`, then run
`chmod 0755 .bazel/corelink-credential-helper.sh` (or point the setting above at
your equivalent executable helper). Export
both values before building; in CI pass the PAT from a secret so it never appears
in `.bazelrc`, process arguments, or build logs:

```bash
export CORELINK_PAT="corelink_pat_0123456789ABCDEF.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.BBBBBBBBBBBBBBBBBBBBBA"   # ${{ secrets.CORELINK_PAT }} in CI
export CORELINK_TENANT="acme-prod"
```

## Verify it worked

After running a build, verify the PAT and tenant are recognized:

```bash
curl --silent --config - <<EOF
url = "https://corelink-api.humangr.com/v1/users/me"
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

The probe supplies the header through curl's stdin config; do not replace it
with `-H "Authorization: Bearer $CORELINK_PAT"`, which exposes the PAT in
process arguments and CI diagnostics.

For a cache-hit check, run the same build twice. Inspect Bazel's execution log
(`--execution_log_json_file`) for `remoteCacheHit: true` entries on the second
run.

## Troubleshooting Bazel-specific issues

| Symptom | Likely cause | Fix |
|---|---|---|
| Every request 404s | `--remote_cache` points at the wrong prefix | Stock Bazel must use `/bazel/cache`; a REAPI/ByteStream client may use `/bazel/v2/<tenant>/blobs/...` — a bare host, `/bazel/v2` with stock Bazel, or `/v1/cas` will 404 for Bazel's paths |
| `UNAUTHENTICATED` | Missing or wrong `Authorization` header | Verify `CORELINK_PAT` is exported in your shell / CI env |
| `PERMISSION_DENIED` / 403 | PAT or cache scope is missing | Use a PAT with cache read + write scope; the stock alias resolves the tenant from the authenticated request, not from a URL instance segment |
| Cache miss on every build | `--remote_upload_local_results=false` | Set to `true` in at least one CI job |

Full error reference: [Troubleshooting](../troubleshooting.md).
