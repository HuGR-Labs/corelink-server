---
id: troubleshooting
title: Troubleshooting
sidebar_position: 10
description: Common error codes, what they mean, and how to fix them.
---

# Troubleshooting

## Error reference

### `401 Unauthorized`

**Meaning**: The `Authorization` header is missing, malformed, or the PAT has been revoked.

**Diagnostics**:

```bash
# Confirm the PAT is set in your shell
echo $CORELINK_PAT

# Test the PAT directly
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
```

If `/v1/users/me` returns `401`, the PAT is invalid. Possibilities:

1. The PAT was revoked from the admin dashboard.
2. The PAT was never exported (`export CORELINK_PAT=...` vs `CORELINK_PAT=...`).
3. You are using a test PAT (`clk_test_...`) against the production API.

**Fix**: Regenerate a PAT from the admin dashboard. Store it in a secret manager before closing the tab.

---

### `403 Forbidden`

**Meaning**: The PAT is valid but does not have permission to perform the requested operation.

Two sub-cases:

1. **Scope mismatch**: the PAT was created with `cas:read` only, but you are trying to write.
2. **Tenant mismatch**: the PAT belongs to `acme-prod` but the request URL contains `acme-staging`.

**Diagnostics**:

```bash
# Confirm your tenant
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# Check "tenant_id" in the response

# Confirm tenant in URL matches
echo $CORELINK_TENANT
```

**Fix**: Create a PAT with the correct scopes, or correct the `CORELINK_TENANT` environment variable.

---

### `404 Not Found`

**Meaning**: The blob with the given digest does not exist in the tenant's CAS.

**Common causes**:

- You are reading from a different tenant than the one that uploaded.
- The blob was never uploaded (common in a fresh tenant or new CI pipeline).
- The digest was computed incorrectly.

**Diagnostics**:

```bash
# Verify the digest
sha256sum ./my-file.bin
# Compare to the hash you are requesting
```

**Fix**: Upload the blob before fetching it. Confirm the tenant in the URL matches the uploading tenant.

---

### `422 Unprocessable Entity` (hash mismatch)

**Meaning**: The SHA-256 in the URL path does not match the SHA-256 of the request body.

This is a client-side error. The server computes the digest of the received bytes and compares it to the path segment. If they differ, the upload is rejected.

**Common causes**:

- Digest was computed before compression (e.g., computed on `file.tar` then uploaded `file.tar.gz`).
- Digest was computed on a partial read.
- Multipart upload tooling that adds framing bytes.

**Fix**:

```bash
# Always compute the digest from the exact bytes being sent
DIGEST=$(sha256sum ./artifact.tar.gz | awk '{print $1}')
curl -X PUT ... --data-binary @./artifact.tar.gz \
  ".../v1/cas/$CORELINK_TENANT/$DIGEST"
```

---

### `429 Too Many Requests`

**Meaning**: You have exceeded the rate limit for this operation.

The response includes a `Retry-After` header indicating how many seconds to wait.

**Fix**:

```bash
# Parse the retry delay from the response
curl -si ... | grep -i retry-after
```

For sustained high throughput, contact support to raise limits on your plan.

---

### `503 Service Unavailable` with `audit_closed`

**Meaning**: Your tenant's audit period has been closed by an admin operation. Write operations are suspended until the audit period is reopened.

This is typically triggered during a compliance audit or a billing dispute. Reads (downloads) remain available.

**Fix**: Contact CoreLink support at [support@corelink.humangr.com](mailto:support@corelink.humangr.com) with your tenant ID.

---

## Bazel-specific issues

### `remote_cache: UNAUTHENTICATED`

The `authorization` header was not forwarded. Check:

1. `CORELINK_PAT` is exported in the shell where Bazel runs.
2. Your `.bazelrc` uses `${CORELINK_PAT}` (shell expansion) not a literal placeholder.

### `remote_cache: PERMISSION_DENIED`

Tenant mismatch. Check `x-corelink-tenant` in `.bazelrc` matches `tenant_id` from `/v1/users/me`.

### All actions miss on repeated builds

`--remote_upload_local_results` is `false`. Add:

```text
build --remote_upload_local_results=true
```

---

## Turborepo-specific issues

### `Remote caching disabled`

`TURBO_TOKEN` is not set. In your shell or CI env:

```bash
export TURBO_TOKEN="$CORELINK_PAT"
```

### Cache misses on every Turborepo run

Verify `TURBO_API` contains the correct tenant suffix:

```bash
echo $TURBO_API
# should be: https://corelink-api.humangr.com/turbo/v8/acme-prod
```

---

## Self-diagnosis checklist

Run these in order:

```bash
# 1. Network reachability
curl -s https://corelink-api.humangr.com/api/health
# {"status":"ok"}

# 2. PAT validity
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"...","token_prefix":"clk_live","route_kind":"cas"}

# 3. Write a test blob
echo "healthcheck" > /tmp/cl-test.txt
DIGEST=$(sha256sum /tmp/cl-test.txt | awk '{print $1}')
curl -s -X PUT \
  -H "Authorization: Bearer $CORELINK_PAT" \
  --data-binary @/tmp/cl-test.txt \
  "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST"
# {"hash":"sha256:<digest>"}

# 4. Read it back
curl -s \
  -H "Authorization: Bearer $CORELINK_PAT" \
  "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST"
# healthcheck
```

If all four steps pass, CoreLink is working. Any failure before step 4 means the issue is in your build tool config, not CoreLink.

## Getting help

- GitHub Issues: [github.com/humangr-labs/corelink-server/issues](https://github.com/humangr-labs/corelink-server/issues)
- Email support: [support@corelink.humangr.com](mailto:support@corelink.humangr.com)

When filing a support request, include the output of steps 1–4 above and your tenant ID.
