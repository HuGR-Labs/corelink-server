# Bazel + CoreLink Integration Guide

CoreLink implements the
[Bazel Remote Execution API v2 (REAPI v2)](https://github.com/bazelbuild/remote-apis)
cache as a **ByteStream REST scheme**:

```text
https://corelink-api.humangr.com/bazel/v2/<tenant>/blobs/<hash>/<size>
```

The `<instance>` path segment is your tenant UUID. Blobs are backed by the same
per-tenant Cloudflare R2 store the native CAS serves, and every blob is verified
with **BLAKE3** (native keyspace) — the Bazel REAPI keyspace uses SHA-256
digests as the wire format per the REAPI spec.

> **Native `bazel --remote_cache` support: live.** Stock Bazel's plain-HTTP
> remote cache (`build --remote_cache=https://host/bazel/cache`) is served by a
> dedicated alias route at `/bazel/cache/{cas,ac}/<hash>` — no `:instance` or
> `:size` segment, matching what a vanilla Bazel client actually sends. It's
> tenant-scoped via the same `x-corelink-tenant-id` mechanism as the REAPI v2
> routes; an unauthenticated request gets **401**, not 404 (see
> `crates/corelink-container/src/routes/bazel_v2.rs`, `handle_http_cas_read` /
> `handle_http_cas_write` / their AC counterparts). You can still use a
> REAPI/ByteStream-compatible client pointed at `/bazel/v2/<tenant>` if you
> prefer the size-checked, sharded-key surface.

## Configuration

The repo ships a committed reference config at
[`apps/examples/bazel/.bazelrc`](../../apps/examples/bazel/.bazelrc). It points
Bazel's REAPI instance at your tenant:

```ini
# Point at the CoreLink REAPI v2 endpoint (the /bazel/v2 prefix is required).
build --remote_cache=https://corelink-api.humangr.com/bazel/v2

# Your tenant UUID becomes the REAPI :instance path segment.
build --remote_instance_name=${CORELINK_TENANT}

# Authenticate with your PAT. The Cloudflare Worker validates the bearer token
# and injects the x-corelink-tenant-id header before the request reaches the cache.
build --remote_header=Authorization=Bearer ${CORELINK_PAT}

build --remote_timeout=30s
build --remote_upload_local_results=true
```

Export both values before building (in CI, source the PAT from a secret):

```bash
export CORELINK_PAT="corelink_pat_XXXXXXXXXXXX"
export CORELINK_TENANT="acme-prod"
```

## Verify

Run the same build twice and inspect Bazel's execution log for cache hits on the
second run:

```bash
bazel build //... --execution_log_json_file=/tmp/exec.json
# grep for "remoteCacheHit": true entries
```

You can also confirm the PAT resolves to your tenant:

```bash
curl -s -H "Authorization: Bearer $CORELINK_PAT" \
  https://corelink-api.humangr.com/v1/users/me
# {"tenant_id":"acme-prod","token_prefix":"corelink","route_kind":"reapi_v1"}
```

## Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| 401 on every request via `/bazel/cache/...` | Missing or wrong `Authorization` header on the stock-HTTP alias | Verify `CORELINK_PAT` is exported and the header is set |
| `UNAUTHENTICATED` (REAPI v2 path) | Missing or wrong `Authorization` header | Verify `CORELINK_PAT` is exported |
| `PERMISSION_DENIED` / 403 | Instance name is not your tenant | Set `--remote_instance_name` to your tenant UUID |
| Cache miss on every build | `--remote_upload_local_results=false` | Set to `true` in at least one CI job |

## Further reading

- [`apps/examples/bazel/.bazelrc`](../../apps/examples/bazel/.bazelrc) — the committed reference config
- [REAPI v2 specification](https://github.com/bazelbuild/remote-apis)
- Native CAS over plain HTTP: `https://corelink-api.humangr.com/v1/cas/<tenant>/<blake3-hex>`
