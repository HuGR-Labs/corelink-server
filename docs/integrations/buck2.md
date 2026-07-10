# Buck2 + CoreLink Integration Guide

> **Buck2 is not yet supported.** CoreLink is a **cache-only** platform served
> over **HTTP**. Buck2's remote cache / remote-execution client
> (`buck2_re_client` with `engine_address` / `cas_address` /
> `action_cache_address`) connects over **gRPC**, and CoreLink does **not**
> expose a gRPC remote-execution endpoint. There is no working `.buckconfig`
> today — a config pointed at a `grpcs://` CoreLink address would fail to
> connect. This guide will be updated if Buck2 support lands.

## What works today

The only wired build-tool cache in the Bazel family is the **Bazel REAPI v2
ByteStream** surface at `https://corelink-api.humangr.com/bazel/v2/<tenant>`.
See the [Bazel integration guide](./bazel.md).

For any tool that speaks plain HTTP, the native CAS is available directly:

```text
PUT/GET https://corelink-api.humangr.com/v1/cas/<tenant>/<blake3-hex>
```

The digest is the blob's **BLAKE3** hash (compute with `b3sum`).

## Want Buck2?

Buck2 gRPC remote execution / remote cache is not on the current surface. If you
need it, raise it with support so it can be weighed against the roadmap.
