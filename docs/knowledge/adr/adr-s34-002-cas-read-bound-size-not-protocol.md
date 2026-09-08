---
type: "ADR"
title: "ADR-S34-002 — the CAS read path is bounded by object SIZE, not by streaming"
description: "Measuring the read path to plan streaming showed the concurrency permit bounds the count but not the size, that a CAS object may legitimately reach the 1 GiB mirror cap, and that 95% of stored objects are under 1 MiB — so the fix is a size ceiling, not a protocol change."
source_files:
  - "specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md"
source_blobs:
  - "specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md@0d81f2e49bad9c34dd907420dfb1986a85ef87af"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["adr", "cas", "read-path", "memory", "streaming", "availability", "dos"]
timestamp: "2026-08-26T00:00:00Z"
---
# ADR-S34-002 — the CAS read path is bounded by object SIZE, not by streaming

ADR-S34-001 parked the read-path memory argument on the assumption that the per-tenant concurrency permit added by B-052 answered it, leaving streaming to justify itself on time-to-first-byte alone. Measurement showed both halves of that assumption were wrong, in opposite directions: the memory concern is NOT closed, and streaming is still not the answer.

# Context

`CasReadHandler::read` is a synchronous trait method returning an owned `Vec<u8>`, served by blocking a worker thread; the S3 client collects the whole body and then copies it, so two N-byte allocations are live at once and three on the BYOK path, where the plaintext is produced while the ciphertext is still held (`specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md:36-56`).

`CAS_READ_CONCURRENCY_LIMIT` is 8 per tenant, and B-052 correctly extended it to the single GET — but peak heap is `N x object_size` and B-052 bounds only `N`. The 10 MiB `DefaultBodyLimit` does not cap the other factor: it bounds client-supplied request bodies, while the server-side ingest paths present no body at all and cap a mirrored blob at **1 GiB**. A CAS object may therefore be three orders of magnitude larger than the request-body limit implies, and the read path buffers it whole (`specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md:58-75`).

What is actually stored was enumerated rather than assumed — the full `corelink-cas-prod` bucket on 2026-08-26, 22,597 objects and 3.57 GB: median 593 bytes, 87.9% under 64 KiB, 95.1% under 1 MiB, maximum 52.3 MB, with the 4.86% above 1 MiB holding 78.3% of stored bytes (`specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md:77-97`).

# Decision

Four decisions. **The memory concern is not closed** and B-051 must stop claiming it is: the worst case after B-052 is eight concurrent reads of an object whose only ceiling is the mirror's 1 GiB cap. **Bound the size, not the protocol** — a read-side ceiling is a constant and a check, the same shape as the permit B-052 added, and it composes with it so that `N x max_size` becomes a number the system chose rather than one it inherited. **Streaming is not justified by this data**: it would buy nothing for the 95% of objects under 1 MiB while costing an async-trait migration of `CasReadHandler` across 11 production implementors and 12 call sites. **If streaming is ever revisited it is size-triggered**, serving small objects exactly as today and streaming only above a threshold, which keeps the digest re-verify intact for the 95% and confines the integrity question to the tail (`specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md:99-127`).

# Consequences

B-051 is re-scoped from streaming to the size bound; the integrity risk it originally guarded is unchanged because the scrubber (B-050) now provides at-rest coverage regardless. A read-side ceiling is a behaviour change on the serving path — an object above the threshold that is served today would stop being served — so the threshold must sit above the observed maximum or the change needs its own migration story, which is deliberately left to the implementing item rather than guessed (`specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md:129-137`).

Three alternatives were rejected: shipping streaming as originally scoped (a large migration whose benefit lands on 4.86% of objects, and which removes the read-path digest re-verify for all of them); doing nothing on the grounds that 52.3 MB x 8 is survivable (the number that makes it survivable is an accident — nothing prevents a 1 GiB mirror blob from being read eight times concurrently); and lowering the concurrency limit instead (it degrades the 95% of reads that are tiny in order to bound the 4.86% that are not — the factor to bound is the one that varies by three orders of magnitude) (`specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md:139-155`).

# Citations

1. `specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md:36-56` — the read path is a sync trait method over a blocking call, with two live N-byte allocations and three on the BYOK path.
2. `specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md:58-75` — the permit bounds the count; the 10 MiB body limit does not bound object size, and the mirror cap is 1 GiB.
3. `specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md:77-97` — the measured size distribution of the whole prod CAS bucket.
4. `specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md:99-127` — the four decisions, including the async-trait migration cost that makes streaming expensive.
5. `specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md:129-137` — consequences: B-051 re-scoped, and the threshold question left open on purpose.
6. `specs/03_architecture/adrs/ADR-S34-002-cas-read-bound-size-not-protocol.md:139-155` — the three rejected alternatives.


# Revalidation

This concept was revalidated against the cumulative implementation tree; its existing source citations remain the controlling evidence for the behavior described above.
