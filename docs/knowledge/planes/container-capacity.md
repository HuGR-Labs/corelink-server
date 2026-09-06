---
type: "CapacityContract"
title: "Deployed container capacity and cache-plane memory envelope"
description: "The basic production container is 1 GiB / 0.25 vCPU; CAS, Argon2id, Turbo, and the bounded Bloom cache derive from one checked declaration."
source_files:
  - "crates/corelink-container/src/container_capacity.rs"
checkpoint_sha: "a810ff13ddee10d4af4589a51610c1fd0422cbaf"
provenance: "AUTHORED"
tags: ["container", "capacity", "memory", "cas", "argon2id", "turbo", "bloom"]
timestamp: "2026-09-05T00:00:00Z"
---

# Deployed container capacity and cache-plane memory envelope

Production uses the Cloudflare `basic` container: 1 GiB of memory and 0.25
vCPU. `container_capacity.rs` is the single source for that measured shape and
for the process-wide slices reserved by CAS reads, CAS writes, Argon2id
verification, Turbo PUT/GET, Turbo telemetry, and the 2048-tenant Bloom cache.
The declared slices leave a runtime reserve and are checked at compile time and
again before the native server binds its listener.

The 1 GiB envelope is deliberately explicit (all values are MiB):

| slice | reservation | peak covered |
| --- | ---: | --- |
| CAS reads | 220 | one 64 MiB object × 3 live copies + one 10 MiB batch body + 4 MiB parser metadata + 8 MiB streamed response |
| CAS writes | 42 | one 20 MiB single-PUT peak + one 22 MiB batch peak (10 MiB body + 8 MiB payload + 4 MiB parser metadata) |
| Argon2id | 64 | one 64 MiB verification |
| Turbo PUT | 200 | one 100 MiB artifact plus its transient clone |
| Turbo GET | 104 | one 100 MiB artifact and response overhead (one permit; 4 MiB slice headroom) |
| Turbo events | 8 | bounded telemetry requests |
| Bloom bit arrays + map metadata | 258 | 2048 × 128 KiB plus 2 MiB metadata allowance |
| runtime reserve | 128 | allocator, runtime, framing, and non-buffering work |
| total | 1024 | equal to the deployed basic memory |

CAS read/write reservations are acquired by `FromRequestParts` extractors before
Axum buffers request or storage bytes. Batch parsers additionally cap each
NDJSON/manifest line at 1 KiB, each retained hash at 128 bytes, and the object
count before allocating the next entry. BYOK reads therefore account for SDK
bytes, the handler copy, plaintext, and bounded request metadata simultaneously.
A saturated or closed pool returns 503, while the existing per-tenant guards
preserve fairness. For single GET and streamed batch-read responses, the
per-tenant `CasReadSlot` remains owned by the response stream until consumption
or drop, not merely until the handler returns.

Per-tenant concurrency guards remain in each route. Process-wide semaphores
bound aggregate work across tenants, and rejected acquisitions return a
fail-closed 503 before request or object bytes are buffered. Separate Turbo
verb pools prevent a GET flood from starving PUTs; the Argon2id sub-cap keeps a
single tenant from consuming the verifier pool.

# Citations

1. `crates/corelink-container/src/container_capacity.rs:1-12` — checked deployed memory/vCPU envelope and bounded process-wide reservations.
