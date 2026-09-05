---
id: raw-curl
title: Raw HTTP (curl) usage
sidebar_position: 3
description: Using CoreLink directly with curl — for scripting, debugging, and CI pipelines that don't use a build tool.
---

# Raw HTTP (curl) usage

This page covers using the CoreLink API directly with `curl`. It is useful for:

- Scripting artifact uploads in release pipelines.
- Debugging auth or network issues before wiring a build tool.
- Any toolchain that speaks HTTP but does not use REAPI.

## Authentication setup

```bash
export CORELINK_PAT="corelink_pat_0123456789ABCDEF.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA.BBBBBBBBBBBBBBBBBBBBBA"
export CORELINK_TENANT="acme-prod"
export CORELINK_BASE="https://corelink-api.humangr.com"
```

## Upload a file

The native CAS content-addresses every blob by its **BLAKE3** digest (lowercase
hex), so compute the digest with `b3sum` — not `sha256sum`. Install it with
`brew install b3sum` (macOS) or `cargo install b3sum` / your distribution's package
(Linux).

```bash
# 1. Compute the BLAKE3 digest
DIGEST=$(b3sum ./artifact.tar.gz | awk '{print $1}')
echo "Digest: $DIGEST"

# 2. Upload
curl -s -X PUT \
  -H "Content-Type: application/octet-stream" \
  --data-binary @./artifact.tar.gz \
  "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$DIGEST" --config - <<EOF
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
```

On success the server returns **201 Created** (or **200 OK** if the blob already
existed) and the response body is the stored BLAKE3 hex — the same value you
sent in the URL:

```text
af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262
```

## Download a file

```bash
curl -s \
  "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$DIGEST" \
  -o ./artifact-downloaded.tar.gz --config - <<EOF
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF

# Verify integrity
b3sum ./artifact-downloaded.tar.gz
# should match $DIGEST
```

## Check if a blob exists (HEAD request)

```bash
curl -s -I \
  "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$DIGEST" --config - <<EOF
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
```

- HTTP 200: blob exists.
- HTTP 404: blob not found.

## Upload a directory as an archive

Useful for caching build output directories:

```bash
# Archive, compute digest, upload in one pipeline
tar -czf - ./dist/ \
  | tee >(b3sum | awk '{print $1}' > /tmp/digest.txt) \
  | curl -s -X PUT \
      -H "Content-Type: application/octet-stream" \
      --data-binary @- \
      "$CORELINK_BASE/v1/cas/$CORELINK_TENANT/$(cat /tmp/digest.txt)" --config /dev/fd/3 3<<EOF
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF

echo "Uploaded as $(cat /tmp/digest.txt)"
```

## Scripted push + pull in GitHub Actions

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    env:
      CORELINK_PAT: ${{ secrets.CORELINK_PAT }}
      CORELINK_TENANT: acme-prod
    steps:
      - uses: actions/checkout@v4

      - name: Build
        run: make build

      - name: Push artifact to CoreLink
        run: |
          DIGEST=$(b3sum ./dist/app.bin | awk '{print $1}')
          curl -fsSL -X PUT \
            -H "Content-Type: application/octet-stream" \
            --data-binary @./dist/app.bin \
            "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/$DIGEST" --config - <<EOF
          header = "Authorization: Bearer ${CORELINK_PAT}"
          EOF
          echo "ARTIFACT_DIGEST=$DIGEST" >> $GITHUB_OUTPUT
        id: push

  deploy:
    needs: build
    runs-on: ubuntu-latest
    env:
      CORELINK_PAT: ${{ secrets.CORELINK_PAT }}
      CORELINK_TENANT: acme-prod
    steps:
      - name: Pull artifact from CoreLink
        run: |
          curl -fsSL \
            "https://corelink-api.humangr.com/v1/cas/$CORELINK_TENANT/${{ needs.build.outputs.ARTIFACT_DIGEST }}" \
            -o ./app.bin --config - <<EOF
          header = "Authorization: Bearer ${CORELINK_PAT}"
          EOF
          chmod +x ./app.bin
```

## Verify it worked

```bash
curl --silent --config - <<EOF
url = "https://corelink-api.humangr.com/v1/users/me"
header = "Authorization: Bearer ${CORELINK_PAT}"
EOF
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

If `tenant_id` matches your tenant and there are no errors, you are fully authenticated.

## Common pitfalls

| Issue | Cause | Fix |
|---|---|---|
| `422 Unprocessable Entity` | BLAKE3 digest in URL does not match the uploaded bytes (for example, computed before gzip) | Compute the digest with `b3sum` from the exact bytes being uploaded |
| `curl: (22) The requested URL returned error: 401` | PAT not exported or wrong | `echo $CORELINK_PAT` to verify |
| Corrupt downloaded file | Used `--output -` (stdout) piped to a file while curl also wrote progress to stdout | Always use `-o <filename>` or `-s` flag |
| Large file times out | Default curl timeout hit | Add `--max-time 300` for large artifacts |

Full error reference: [Troubleshooting](../troubleshooting.md).
