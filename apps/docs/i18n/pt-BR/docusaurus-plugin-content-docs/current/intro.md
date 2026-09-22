---
id: intro
title: What is CoreLink?
sidebar_position: 1
description: CoreLink is a multi-tenant content-addressable cache for build artifacts, packages, container layers, and ML model weights — hosted on Cloudflare.
---
<!-- i18n:MT (pt-BR) — bootstrap MT-stub from EN source; replace with native-speaker translation before GA -->

> MT: Esta página está em tradução. A versão canônica em inglês é a fonte de verdade até a revisão por falante nativo (D+10).
>
> Canonical EN source: `docs/intro.md`


# What is CoreLink?

CoreLink is a hosted, multi-tenant **content-addressable cache** for build artifacts. It stores any blob exactly once by its BLAKE3 digest and serves it from the Cloudflare edge nearest to each client.

Build tools that speak the [Remote Execution API (REAPI)](https://github.com/bazelbuild/remote-apis) **over HTTP/REST** — Bazel, and other REST-capable REAPI clients — can point directly at CoreLink with zero code changes. CoreLink serves no gRPC ingress, so gRPC-only REAPI clients (Buck2, NativeLink) cannot connect today. Turborepo connects via a single environment variable. Raw HTTP clients use the REST endpoints.

## Who it is for

- **Teams running Bazel** that want a managed remote cache without operating S3 buckets, Redis, or `bazel-remote` themselves.
- **Turborepo monorepos** that want a custom remote cache outside Vercel's hosted offering.
- **Platform engineering teams** that want tenant isolation, audit logs, and BYOK encryption in one service.

## What CoreLink is not

CoreLink is not a remote execution engine. It stores and retrieves content by hash; it does not schedule or run build actions. Use it alongside [BuildBarn](https://github.com/buildbarn/bb-storage) or [EngFlow](https://www.engflow.com) if you need remote execution.

## How it works

```
build tool                CoreLink API (Cloudflare Worker)       R2 / KV
─────────────────────     ─────────────────────────────────     ─────────
PUT /v1/cas/<t>/<b3>   ─► auth (PAT) → tenant isolation        → stored once
GET /v1/cas/<t>/<b3>   ◄─ cache-hit lookup                     ← returned
```

Every blob is addressed by its BLAKE3 digest — compute it with `b3sum`, **not** `sha256sum`; a digest of the wrong algorithm is rejected with `422 content hash mismatch`. If two tenants upload the same bytes, each tenant pays for one copy and has independent access control — content is shared at the storage layer, access is not.

## Key capabilities

| Capability | Details |
|---|---|
| Content-addressable storage (CAS) | BLAKE3-keyed blob store (`b3sum`). Deduplicates automatically. |
| Action cache (AC) | Maps `(action_digest) → (output_digest)` so Bazel skips identical actions. |
| Multi-tenancy | Each tenant is isolated at the PAT level. Cross-tenant reads are never possible. |
| BYOK encryption | Tenants on the Enterprise plan can supply their own AES-256 key. |
| Audit log | Every read and write is appended to an immutable, tenant-scoped log. |
| REAPI v2 | Bazel REAPI v2 (`ContentAddressableStorage` + `ActionCache` + `ByteStream` semantics) served over plain HTTP/REST — no gRPC ingress. |

## Next step

The fastest path to your first cache hit is the [5-minute quickstart](./quickstart.md).
