---
type: "CacheSurface"
title: "Authenticated REAPI ByteStream contract"
description: "The unmounted REAPI ByteStream service contract over the authenticated tenant-scoped ingress and the production-decorated CAS handlers."
source_files:
  - "crates/corelink-container/src/reapi_bytestream.rs"
  - "crates/corelink-container/src/reapi_bytestream/tests.rs"
  - "crates/corelink-container/src/reapi_ingress.rs"
  - "crates/corelink-container/src/reapi_ingress/validation.rs"
source_blobs:
  - "crates/corelink-container/src/reapi_bytestream.rs@d52b73e37b0b655e0a486dd18210d3ce78eb6989"
  - "crates/corelink-container/src/reapi_bytestream/tests.rs@6d0a6d546caf565f653454928985d715aeabec72"
  - "crates/corelink-container/src/reapi_ingress.rs@2fd5fdbfc51ca4ed5c759580d3a91f63e0264056"
  - "crates/corelink-container/src/reapi_ingress/validation.rs@b7f5d1045997e7b7e733bea9fcbb95caa97a6881"
checkpoint_sha: "c7e654490dcb836099118f1a41d1aed145c8fcda"
provenance: "AUTHORED"
timestamp: "2026-09-23T00:00:00Z"
---

# Authenticated REAPI ByteStream contract

`ReapiByteStreamService` remains a dependency-only service. It consumes the #2177 ingress bundle and gRPC remains unmounted until #2176 supplies the transport proof. It creates no local cache, upload-resume state, route, or storage handler.

Every read authenticates and binds the REAPI instance to the PAT tenant before one decorated CAS read. The completed handler response is then emitted in 64 KiB frames while the admission lease and ByteStream buffer permit stay live. The declared SHA-256 digest size caps each payload at the configured 5 MiB CAS ceiling; three operations may buffer a body concurrently within the declared container read and write budgets.

Every write requires a write-capable PAT, exact upload resource name, ordered offsets, an explicit `finish_write`, declared byte count, and SHA-256 match before its exactly once decorated CAS persistence call. The decorated handler retains tenant prefixes, tombstones, quota, byte accounting, audit, and BYOK controls. Quota, audit, backend, and validation failures surface as gRPC errors and never create speculative persistence.

`QueryWriteStatus` returns `UNIMPLEMENTED`: this service does not claim a durable upload state it does not have.

# Citations

1. `crates/corelink-container/src/reapi_bytestream.rs:22-48` — payload, chunk, concurrency, and container-budget bounds.
2. `crates/corelink-container/src/reapi_bytestream.rs:69-121` — authenticated read validation, one decorated read, and bounded frame emission.
3. `crates/corelink-container/src/reapi_bytestream.rs:124-192` — ordered, final, hash-checked write followed by exactly one decorated persistence call.
4. `crates/corelink-container/src/reapi_bytestream.rs:205-232` — tonic service contract and explicit unsupported resume behavior.
5. `crates/corelink-container/src/reapi_bytestream/tests.rs:154-370` — behavior and adversarial coverage for storage denial, ranges, offsets, audit/quota failure, and resume.
6. `crates/corelink-container/src/reapi_ingress.rs:160-187` — tenant-scoped SHA-256 CAS handler request with a pre-materialization byte limit.
7. `crates/corelink-container/src/reapi_ingress/validation.rs:48-118` — canonical tenant-bound read/upload resource validation.
