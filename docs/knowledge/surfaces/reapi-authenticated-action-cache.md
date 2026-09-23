---
type: "CacheSurface"
title: "Authenticated REAPI ActionCache contract"
description: "The unmounted REAPI ActionCache and cache-only Capabilities services over the authenticated tenant-scoped ingress and production-decorated ActionCache handlers."
source_files:
  - "crates/corelink-container/src/reapi_action_cache.rs"
  - "crates/corelink-container/src/reapi_action_cache/tests.rs"
  - "crates/corelink-container/src/reapi_ingress.rs"
  - "crates/corelink-reapi/proto/build/bazel/remote/execution/v2/remote_execution.proto"
source_blobs:
  - "crates/corelink-container/src/reapi_action_cache.rs@679919da0b00f09088321e65934967bd804210f8"
  - "crates/corelink-container/src/reapi_action_cache/tests.rs@9fc3883b9a529d36892724db4c13522f7f9655e1"
  - "crates/corelink-container/src/reapi_ingress.rs@fb5b4587efddf66562d31044110016fe2b26eabe"
  - "crates/corelink-reapi/proto/build/bazel/remote/execution/v2/remote_execution.proto@4571a37249a961fb4dca033b3b3c1344bb6020c0"
checkpoint_sha: "acbf17bac05bae34eb0d4b809f0b529a39722897"
provenance: "AUTHORED"
tags: ["surfaces", "reapi", "grpc", "auth", "tenancy", "action-cache"]
timestamp: "2026-09-23T00:00:00Z"
---

# Authenticated REAPI ActionCache contract

`ReapiActionCacheService` authenticates every lookup and update through the #2177 ingress bundle, binds the REAPI instance to the D1-verified tenant, and holds the shared quota/concurrency lease before invoking a handler. gRPC remains unmounted until #2176 proves transport transparency and #2183 composes the accepted sibling contracts.

The service validates the action digest, the serialized result ceiling, every output-file and output-directory digest, and inline output bytes before its one decorated update call. It serializes the complete vendored `ActionResult` wire message as one payload, preserving output metadata and all symlink collections. The decorated ActionCache path retains immutable update behavior, tenant prefixes, audit, BYOK, tombstones, byte accounting, and quota controls. A miss is `NOT_FOUND`; an immutable divergent update is `ALREADY_EXISTS`; storage, audit, and quota ambiguity fails closed.

`ReapiCacheCapabilitiesService` requires the same authenticated tenant binding and advertises SHA-256 plus ActionCache updates. Its execution capabilities field is unset: there is no execution service or execution claim.

# Citations

1. `crates/corelink-container/src/reapi_action_cache.rs:31-89` — authenticated ActionCache lookup/update and the sole decorated handler calls.
2. `crates/corelink-container/src/reapi_action_cache.rs:153-211` — action/result digest, inline-byte, and serialized-size validation.
3. `crates/corelink-container/src/reapi_action_cache.rs:218-249` — cache-only authenticated capability response with no execution.
4. `crates/corelink-container/src/reapi_action_cache/tests.rs:176-374` — round-trip, denial, malformed data, immutable conflict, quota/audit, and capability behavior.
5. `crates/corelink-reapi/proto/build/bazel/remote/execution/v2/remote_execution.proto:103-210` — ActionCache RPCs and complete vendored ActionResult wire shape.
