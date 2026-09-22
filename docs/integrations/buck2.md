# Buck2 + CoreLink Integration Guide

CoreLink provides a cache-only REAPI gRPC ingress for Buck2. When deployed with
the container PAT verifier configured, it serves CAS, ByteStream, ActionCache,
and Capabilities on the API HTTPS origin. Capabilities advertise SHA-256 and
ActionCache updates, and do not advertise remote execution. Buck2 actions run
locally; CoreLink stores and retrieves their cache entries.

## Configure the client

Start from [`examples/buck2-starter/.buckconfig`](../../examples/buck2-starter/.buckconfig).
Its `[buck2_re_client]` section points the engine, ActionCache, and CAS clients
to the CoreLink API origin and injects the PAT through `http_headers`. Buck2
supports `$VAR` expansion in that header. Keep the PAT in the environment or a
secret manager; never put it in a committed config.

Set the instance name to the tenant ID that owns the PAT in an ignored
`.buckconfig.local` file:

```ini
[buck2_re_client]
instance_name = YOUR_PAT_TENANT_ID

[corelink]
provenance = YOUR_REPOSITORY@YOUR_COMMIT_SHA
```

The server checks that each request's instance matches the authenticated
tenant. Read operations require a valid cache-scoped PAT; CAS and ActionCache
writes also require write scope. The gRPC ingress uses SHA-256 digests,
delegates storage to the shared tenant-isolated CAS/ActionCache handlers, and
preserves their quota, audit, and byte-accounting behavior.

Use `remote_enabled = False` and `remote_cache_enabled = True` in the registered
execution platform. Do not interpret a local build as remote execution or as
proof of a CoreLink cache hit. A warm-cache claim needs a clean local build and
a report with nonzero CoreLink remote reads.

For the complete starter flow, see the [Buck2 starter README](../../examples/buck2-starter/README.md).
The separate [Bazel integration guide](./bazel.md) documents the HTTP REAPI
surface for Bazel clients.
