---
type: "CacheSurface"
title: "Native CAS surface"
description: "CoreLink's first-party content-addressable storage surface: digest-keyed reads, writes, deletes, lists, and bulk routes over the shared R2-backed cache."
source_files:
  - "crates/corelink-container/src/routes/cas.rs"
  - "crates/corelink-container/src/routes/cas/foundation_core.rs"
  - "crates/corelink-container/src/routes/cas/foundation_state.rs"
  - "crates/corelink-container/src/routes/cas/single_setup.rs"
  - "crates/corelink-container/src/routes/cas/single_handlers.rs"
  - "crates/corelink-container/src/routes/cas/batch_write.rs"
  - "crates/corelink-container/src/routes/cas/batch_read.rs"
  - "crates/corelink-container/src/routes/cas/list_delete.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs"
source_blobs:
  - "crates/corelink-container/src/routes/cas.rs@41c65ce80c66084ef424ac4189b644bbe8461f95"
  - "crates/corelink-container/src/routes/cas/foundation_core.rs@63671990ccb9f0e9e6cba8804457737b42c18120"
  - "crates/corelink-container/src/routes/cas/foundation_state.rs@8dcd20ecd23850e331763d5ccfcf4d0260489f0e"
  - "crates/corelink-container/src/routes/cas/single_setup.rs@27e4090d053420fa0bfe07ee655d9c6ce6573439"
  - "crates/corelink-container/src/routes/cas/single_handlers.rs@f4259f925d39cff5d70ae6fb8fc5996f2109dba2"
  - "crates/corelink-container/src/routes/cas/batch_write.rs@3561b0e3188b5ff05ff5998fdad3f33ee5f6ad78"
  - "crates/corelink-container/src/routes/cas/batch_read.rs@762ed7b8f4726a6b034e5b2afd1980f3f66c20bf"
  - "crates/corelink-container/src/routes/cas/list_delete.rs@0a5e23ba9d812af9ffe5bd76e1f9a88d91dc12eb"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs@613fdde5ed6e70e5ba2079903c013e3dc33f27fb"
checkpoint_sha: "648ecdccd229bdb5154b86843053c28b9cce9d36"
provenance: "AUTHORED"
tags: ["surfaces", "cas", "cache", "hot-path"]
timestamp: "2026-06-26T00:00:00Z"

---
# Native CAS surface

The native CAS surface is the first-party digest-keyed blob API. The router serves single-object
read, write, delete, and list routes alongside three static bulk endpoints; other cache adapters
share the underlying CAS storage and handler seams (`crates/corelink-container/src/routes/cas/single_setup.rs:187-222`, `crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs:383-405`).

# Role

A request reaches these handlers after the Worker and Durable Object resolve the authenticated
tenant. The route checks that the client-supplied tenant segment matches that identity, validates
the digest and operation scope, and then delegates to the injected CAS handler (`crates/corelink-container/src/routes/cas/single_handlers.rs:23-55,145-174`).

# How It Works

1. The router mounts GET, PUT, DELETE, list, batch-write, batch-read, and batch-exists routes. Static bulk paths are siblings of the digest wildcard (`crates/corelink-container/src/routes/cas/single_setup.rs:187-222`).
2. Single-object reads reject a tenant mismatch, malformed digest, insufficient read scope, a rejected PAT, or a tombstone before calling the read handler (`crates/corelink-container/src/routes/cas/single_handlers.rs:23-92`).
3. Single-object writes reserve per-tenant and process-wide capacity before body buffering. The handler then checks tenant identity, digest shape, write scope, the container's PAT write capability, and the monthly quota before delegating the write (`crates/corelink-container/src/routes/cas/foundation_state.rs:144-204`; `crates/corelink-container/src/routes/cas/single_handlers.rs:133-198`).
4. Bulk writes apply the same tenant, digest, scope, PAT, and quota gates; they parse a newline-delimited manifest followed by concatenated bytes and pass each verified object through the write handler (`crates/corelink-container/src/routes/cas/batch_write.rs:45-145,162-175`; framing helper: `crates/corelink-container/src/routes/cas/single_setup.rs:270-287`).
5. Bulk reads and existence checks use a separate per-tenant read-concurrency pool. The batch reader processes a bounded window, caps individual objects and aggregate response bytes, and drains pending reads on terminal errors (`crates/corelink-container/src/routes/cas/foundation_state.rs:240-303`; `crates/corelink-container/src/routes/cas/batch_read.rs:181-195`, `crates/corelink-container/src/routes/cas/batch_read.rs:217-225`, `crates/corelink-container/src/routes/cas/batch_read.rs:365-385`).
6. The `R2CasHandler` stores and retrieves objects through the S3-compatible R2 client. Its CAS core composes tenant- and region-scoped object keys and implements the underlying CAS operations (`crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs:383-405`).
7. The test module is assembled from the split core, batch, cancellation, write-cap, read-ceiling, and edge units in `cas.rs` (`crates/corelink-container/src/routes/cas.rs:53-86`).

# Invariants

- The authenticated tenant is the isolation key. A different path tenant returns 403 before storage (`crates/corelink-container/src/routes/cas/single_handlers.rs:23-29`).
- A non-canonical digest returns 400 before an R2 key is used (`crates/corelink-container/src/routes/cas/single_handlers.rs:30-34`).
- Writes require write scope and the container re-derives the PAT's stored `can_write` capability before storage (`crates/corelink-container/src/routes/cas/single_handlers.rs:155-167`; helper: `crates/corelink-container/src/routes/cas/single_setup.rs:318-328`).
- A tombstoned digest is returned as HTTP 410 before the native read reaches R2. A tombstone lookup fault returns 503 (`crates/corelink-container/src/routes/cas/single_handlers.rs:56-77`).
- The per-tenant upload guard runs before the body extractor, so an over-cap request returns 429 without buffering the upload (`crates/corelink-container/src/routes/cas/foundation_state.rs:144-204`).
- Batch request bodies and object counts have explicit limits; over-cap write requests return 413 (`crates/corelink-container/src/routes/cas/foundation_core.rs:110-160`; `crates/corelink-container/src/routes/cas/batch_write.rs:45-145`).
- Batch reads hold their tenant and process-wide reservations through response consumption; object and aggregate overflows fail closed (`crates/corelink-container/src/routes/cas/foundation_state.rs:240-350`; `crates/corelink-container/src/routes/cas/batch_read.rs:181-195`, `crates/corelink-container/src/routes/cas/batch_read.rs:217-225`, `crates/corelink-container/src/routes/cas/batch_read.rs:365-385`).
- Usage metering is recorded after successful handler outcomes and does not gate the storage decision (`crates/corelink-container/src/routes/cas/single_handlers.rs:92-123`, `crates/corelink-container/src/routes/cas/single_handlers.rs:198-212`).

# Citations

- `crates/corelink-container/src/routes/cas.rs:45-51` — the executable route module includes.
- `crates/corelink-container/src/routes/cas/foundation_core.rs:110-160` — batch size and request-body limits.
- `crates/corelink-container/src/routes/cas/foundation_state.rs:144-204` — pre-body upload reservation; `:240-303` — separate read reservation.
- `crates/corelink-container/src/routes/cas/single_setup.rs:187-222` — route table; `:318-328` — native write-capability gate helper.
- `crates/corelink-container/src/routes/cas/single_handlers.rs:23-92` — single-read tenant, digest, scope, PAT, tombstone, and quota gates; `:133-212` — single-write gate, commit, response, and metering.
- `crates/corelink-container/src/routes/cas/batch_write.rs:45-145` — bulk-write gates; `:162-175` — per-object `state.write.write` delegation.
- `crates/corelink-container/src/routes/cas/batch_read.rs:181-195` — request lease and bounded fanout; `:217-225,302` — byte limits; `:365-385` — response stream retains reservations.
- `crates/corelink-container/src/routes/cas/list_delete.rs:34-103` — delete and list handlers.
- `crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs:383-405` — R2 CAS object operations.
