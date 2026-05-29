---
id: intro
title: What is CoreLink?
sidebar_position: 1
description: CoreLink is a multi-tenant content-addressable cache for build artifacts, packages, container layers, and ML model weights — hosted on Cloudflare.
---

# What is CoreLink?

CoreLink is a hosted, multi-tenant **content-addressable cache** for build artifacts. It stores any blob exactly once by its SHA-256 digest and serves it from the Cloudflare edge nearest to each client.

Build tools that support the [Remote Execution API (REAPI)](https://github.com/bazelbuild/remote-apis) — Bazel, Buck2, NativeLink, and others — can point directly at CoreLink with zero code changes. Turborepo connects via a single environment variable. Raw HTTP clients use the REST endpoints.

## Who it is for

- **Teams running Bazel or Buck2** that want a managed remote cache without operating S3 buckets, Redis, or `bazel-remote` themselves.
- **Turborepo monorepos** that want a custom remote cache outside Vercel's hosted offering.
- **Platform engineering teams** that want tenant isolation, audit logs, and BYOK encryption in one service.

## What CoreLink is not

CoreLink is not a remote execution engine. It stores and retrieves content by hash; it does not schedule or run build actions. Use it alongside [BuildBarn](https://github.com/buildbarn/bb-storage) or [EngFlow](https://www.engflow.com) if you need remote execution.

## How it works

```
build tool                CoreLink API (Cloudflare Worker)       R2 / KV
─────────────────────     ─────────────────────────────────     ─────────
PUT /v1/cas/<t>/<hash> ─► auth (PAT) → tenant isolation        → stored once
GET /v1/cas/<t>/<hash> ◄─ cache-hit lookup                     ← returned
```

Every blob is addressed by its SHA-256 digest. If two tenants upload the same bytes, each tenant pays for one copy and has independent access control — content is shared at the storage layer, access is not.

## Key capabilities

| Capability | Details |
|---|---|
| Content-addressable storage (CAS) | SHA-256–keyed blob store. Deduplicates automatically. |
| Action cache (AC) | Maps `(action_digest) → (output_digest)` so Bazel skips identical actions. |
| Multi-tenancy | Each tenant is isolated at the PAT level. Cross-tenant reads are never possible. |
| BYOK encryption | Tenants on the Enterprise plan can supply their own AES-256 key. |
| Audit log | Every read and write is appended to an immutable, tenant-scoped log. |
| REAPI v2 | Full `ContentAddressableStorage` + `ActionCache` + `ByteStream` gRPC services. |

## Next step

The fastest path to your first cache hit is the [5-minute quickstart](./quickstart.md).
