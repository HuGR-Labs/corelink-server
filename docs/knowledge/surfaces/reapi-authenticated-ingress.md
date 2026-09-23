---
type: "CacheSurface"
title: "Authenticated REAPI ingress kernel"
description: "The unmounted, tenant-scoped REAPI gRPC ingress boundary that authenticates through the D1-backed PAT verifier, admits requests once, and forwards only to production-decorated CAS and ActionCache handlers."
source_files:
  - "crates/corelink-container/src/lib.rs"
  - "crates/corelink-container/src/reapi_ingress.rs"
  - "crates/corelink-container/src/reapi_ingress/admission.rs"
  - "crates/corelink-container/src/reapi_ingress/validation.rs"
  - "crates/corelink-container/src/reapi_ingress/tests.rs"
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/routes/build.rs"
source_blobs:
  - "crates/corelink-container/src/lib.rs@7163afcaa8410571779d8a53aaeea8177a5cf3c2"
  - "crates/corelink-container/src/reapi_ingress.rs@e8ed42e40339f55a63cddf2eb95efbe7444e0053"
  - "crates/corelink-container/src/reapi_ingress/admission.rs@fc6bd2e49b672c806b9eaa72f329df7af7a6624d"
  - "crates/corelink-container/src/reapi_ingress/validation.rs@b7f5d1045997e7b7e733bea9fcbb95caa97a6881"
  - "crates/corelink-container/src/reapi_ingress/tests.rs@40d59193ef67f2e168e5801d86a7c70d148fd3fe"
  - "crates/corelink-container/src/routes.rs@da4d710d15537491da17f84c90f133d06124711e"
  - "crates/corelink-container/src/routes/build.rs@65aaaf4eba3bdfafadb5d7d7760a4895bba026ad"
checkpoint_sha: "9f73cedfa760f53da02a33dc7af27ce43b75536a"
provenance: "AUTHORED"
tags: ["surfaces", "reapi", "grpc", "auth", "tenancy", "cache"]
timestamp: "2026-09-23T00:00:00Z"
---

# Authenticated REAPI ingress kernel

The REAPI ingress kernel is a typed dependency bundle for future cache RPCs. It receives the same
production-decorated CAS and ActionCache handlers that the native router uses, plus the D1-backed PAT
verifier, tenant-cap resolver, and quota authority. It is deliberately an unmounted dependency: public
gRPC composition waits for the Worker → Durable Object → container transport proof in #2176 and the
subsequent REAPI service contracts (`crates/corelink-container/src/reapi_ingress.rs:1-7`; `crates/corelink-container/src/routes/build.rs:57-87`).

# Role

Every future RPC calls `authorize` before it can access a handler. The kernel borrows exactly one bearer
value only while calling the authoritative verifier, derives both the tenant and write capability from
that result, rejects reserved or mismatched tenant instances, checks write scope, then holds one
quota/concurrency admission lease for the resulting context (`crates/corelink-container/src/reapi_ingress.rs:388-433`).

# Invariants

- Bearer material is parsed as a borrowed `&str`, never copied into `AuthorizedTenant` or
  `AdmittedIngress`; the admitted context contains only the D1-proven tenant, its write capability,
  handler references, resolver, and lease (`crates/corelink-container/src/reapi_ingress.rs:50-70`; `:388-432`; `:436-458`).
- Missing, malformed, invalid, revoked, or scope-less credentials return `UNAUTHENTICATED`; verifier or
  admission uncertainty returns `UNAVAILABLE`; a cross-tenant instance or read-only write returns
  `PERMISSION_DENIED`; exhausted quota/concurrency returns `RESOURCE_EXHAUSTED`
  (`crates/corelink-container/src/reapi_ingress.rs:140-147`; `:404-419`; `:461-479`; `crates/corelink-container/src/reapi_ingress/admission.rs:29-45`).
- The admission seam uses the existing `QuotaGate` and a bounded semaphore. It drops the permit on a
  quota denial and maps every non-capacity quota response to unavailable, so ambiguity cannot admit a
  request (`crates/corelink-container/src/reapi_ingress/admission.rs:22-45`).
- CAS and ActionCache calls only use the handler trait objects supplied by the router factory after its
  normal accounting, BYOK, tombstone, quota, and tenant-prefix decorators; the ingress creates no
  storage or handler implementation (`crates/corelink-container/src/reapi_ingress.rs:333-385`; `crates/corelink-container/src/routes/build.rs:513-530`).
- Digest and ByteStream resource validation accepts only lowercase SHA-256, non-negative lengths, and
  exact tenant-matched REAPI resource forms (`crates/corelink-container/src/reapi_ingress/validation.rs:7-118`).
- Hosted tests use four storage spies to prove auth, instance, scope, and admission denials never invoke
  CAS or ActionCache, and also assert the admitted context cannot render a bearer placeholder
  (`crates/corelink-container/src/reapi_ingress.rs:397-419`; `crates/corelink-container/src/reapi_ingress/tests.rs:45-168`; `:219-316`).

# Citations

1. `crates/corelink-container/src/lib.rs:185-190` — exports the ingress kernel from the container crate.
2. `crates/corelink-container/src/reapi_ingress.rs:140-147` — D1-backed PAT verification and failure classification.
3. `crates/corelink-container/src/reapi_ingress/admission.rs:22-45` — shared quota and concurrency admission.
4. `crates/corelink-container/src/reapi_ingress/validation.rs:7-118` — canonical digest and ByteStream resource-name validation.
5. `crates/corelink-container/src/reapi_ingress/tests.rs:45-168` — storage-spy denial coverage.
6. `crates/corelink-container/src/routes.rs:521-525` — router factory exports the unmounted ingress bundle.
7. `crates/corelink-container/src/routes/build.rs:63-90` — the optional bundle contract and no-mount boundary.

# Revalidation

This concept is grounded in the ingress implementation and router wiring added for issue #2177. Public
gRPC remains unmounted until #2176 and the CAS, ByteStream, and ActionCache REAPI service contracts are green.
