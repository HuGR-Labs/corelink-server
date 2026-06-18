---
id: quickstart
title: 5-minute quickstart
sidebar_position: 2
description: Sign up, install the CLI, push your first blob, pull it back. Under 5 minutes from a fresh shell.
---

# 5-minute quickstart

Goal: authenticated, first push and pull, verified in under 5 minutes.

:::note CLI coming soon
`corelink-cli` is under active development (stream 1.1). All examples below work **today** with `curl`. Once the CLI ships, the equivalent `corelink` commands are shown alongside.
:::

## Step 1 — Get a PAT

1. Sign up at [corelink-app.humangr.com/sign-up](https://corelink-app.humangr.com/sign-up).
2. After the 2-step onboarding wizard finishes, your tenant is provisioned and a starter PAT is shown **exactly once** on the welcome screen.
3. Copy the PAT and store it in a secret manager (1Password, AWS Secrets Manager, GitHub Actions secret — anything but plaintext). It is never shown again.

Your PAT looks like:

```text
clk_live_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
```

Export it for the examples below:

```bash
export CORELINK_PAT="clk_live_XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"
export CORELINK_TENANT="your-tenant-id"   # shown on the welcome screen
```

## Step 2 — Verify your credentials

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

Expected response:

```json
{
  "tenant_id": "your-tenant-id",
  "token_prefix": "clk_live",
  "route_kind": "cas"
}
```

If you get `401 Unauthorized`, the PAT is wrong or expired — generate a new one from the admin dashboard.

## Step 3 — Push a blob

Compute the SHA-256 of a local file and upload it:

```bash
# Compute digest
DIGEST=$(sha256sum ./my-artifact.bin | awk '{print $1}')

# Upload
curl -s -X PUT \
  -H "Authorization: Bearer $CORELINK_PAT" \
  -H "Content-Type: application/octet-stream" \
  --data-binary @./my-artifact.bin \
  "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST"
```

Expected response (HTTP 201):

```json
{"hash": "sha256:<digest>"}
```

## Step 4 — Pull the blob back

```bash
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST" \
  -o ./my-artifact-downloaded.bin
```

Verify the bytes are identical:

```bash
diff my-artifact.bin my-artifact-downloaded.bin && echo "match"
```

## Step 5 — Connect your build tool

Once you have a working PAT, connect your build tool:

- **Bazel** → [Bazel integration guide](./integrations/bazel.md)
- **Turborepo** → [Turborepo integration guide](./integrations/turborepo.md)
- **Raw HTTP / scripting** → [raw curl examples](./integrations/raw-curl.md)

## Troubleshooting

| Error | Cause | Fix |
|---|---|---|
| `401 Unauthorized` | Bad or expired PAT | Re-generate from admin dashboard |
| `403 Forbidden` | PAT scoped to a different tenant | Check `CORELINK_TENANT` matches your PAT's tenant |
| `404 Not Found` on GET | Blob not uploaded yet | Push first, then pull |
| `422 Unprocessable Entity` | SHA-256 in URL does not match body | Recompute digest from the actual file bytes |

Full error reference: [Troubleshooting](./troubleshooting.md).
