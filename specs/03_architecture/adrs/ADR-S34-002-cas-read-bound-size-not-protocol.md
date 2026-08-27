---
id: "ADR-S34-002"
type: "adr"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-08-26"
updated: "2026-08-26"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "cas", "streaming", "read-path", "memory", "availability", "dos"]
references:
  - "crates/corelink-container/src/storage/r2_s3.rs"
  - "crates/corelink-container/src/routes/cas.rs"
  - "crates/corelink-handler-cas/src/handler.rs"
  - "crates/corelink-container/src/routes/public_mirror.rs"
  - "crates/corelink-container/src/routes/public_pullthrough.rs"
---

# ADR-S34-002 — the CAS read path is bounded by object SIZE, not by streaming

- **Status:** Proposed (2026-08-26)
- **Deciders:** CoreLink tech lead (architecture delegated by the owner/stakeholder)
- **Context tags:** cas, read-path, memory, availability

## Context

ADR-S34-001 ordered the scrubber (B-050) ahead of streaming CAS reads (B-051)
and parked the memory argument with a note: the per-tenant concurrency permit
added by B-052 is the cheap answer to read-path heap, so streaming would have
to justify itself on time-to-first-byte alone.

Both halves of that note turn out to be wrong, and they are wrong in opposite
directions. This ADR records what measurement showed and what follows.

### The read path buffers the whole object, three times in the worst case

`CasReadHandler::read` is a **synchronous** trait method returning an owned
`Vec<u8>` (`crates/corelink-handler-cas/src/handler.rs:41`). The R2 adapter
satisfies it by blocking a worker thread —
`tokio::task::block_in_place(|| handle.block_on(self.client.get(&key)))`
(`crates/corelink-container/src/storage/r2_s3.rs:1282`) — and the client
collects the entire body and then copies it:

```rust
let bytes = output.body.collect().await? .into_bytes().to_vec();
```

(`crates/corelink-container/src/storage/r2_s3.rs:236-242`.) That is two N-byte
allocations alive at once: the SDK's `Bytes` and the `Vec<u8>` copy made to fit
the trait's return type. On the BYOK-active path `decrypt_body`
(`crates/corelink-container/src/storage/r2_s3.rs:821`) produces a third — the
plaintext — while the ciphertext is still held. The route then moves the final
`Vec` straight into the response body
(`crates/corelink-container/src/routes/cas.rs:866`).

### The concurrency permit bounds the COUNT, not the SIZE

`CAS_READ_CONCURRENCY_LIMIT` is 8 per tenant
(`crates/corelink-container/src/routes/cas.rs:343`). B-052 correctly extended
it to the single GET. But peak heap is `N x object_size`, and B-052 bounds only
`N`. Nothing in the read path bounds the other factor.

It is tempting to assume the 10 MiB `DefaultBodyLimit`
(`crates/corelink-container/src/main.rs:504`) caps object size transitively. It
does not: it bounds **client-supplied request bodies**. The server-side ingest
paths never present a request body at all. `MIRROR_MAX_BLOB_BYTES` is **1 GiB**
(`crates/corelink-container/src/routes/public_mirror.rs:128`) and the
pull-through resolver re-checks against the same 1 GiB ceiling
(`crates/corelink-container/src/routes/public_pullthrough.rs:103`) before
writing the blob into the `_public` moat. So a CAS object may legitimately be
three orders of magnitude larger than the request-body limit suggests, and the
read path will buffer it whole.

### What is actually stored (measured, not assumed)

Full enumeration of `corelink-cas-prod` on 2026-08-26 — 22,597 objects,
3.57 GB:

| statistic | value |
|---|---|
| p50 | 593 B |
| p75 | 9.7 KiB |
| p90 | 148 KiB |
| p95 | 1.0 MiB |
| p99 | 4.0 MiB |
| max | 52.3 MB |

| bucket | share of objects |
|---|---|
| ≤ 4 KiB | 69.9% |
| ≤ 64 KiB | 87.9% |
| ≤ 1 MiB | 95.1% |
| ≤ 4 MiB | 99.8% |

The 4.86% of objects above 1 MiB hold **78.3% of stored bytes**.

## Decision

**Decision 1: the memory concern is NOT closed, and B-051 must stop claiming it
is.** The worst case after B-052 is 8 concurrent reads of an object whose only
ceiling is the 1 GiB mirror cap. The measured max is 52.3 MB today, which is
survivable; the ceiling that permits it is not a bound anyone chose for the read
path.

**Decision 2: bound the SIZE, not the protocol.** The targeted fix is a
read-side size ceiling — refuse, or handle differently, an object above a
threshold — not a rewrite of how bytes travel. This is a constant and a check,
in the same shape as the permit B-052 added, and it composes with it: `N x
max_size` becomes a number the system chose rather than one it inherited.

**Decision 3: streaming is not justified by this data.** 95.1% of objects are
≤ 1 MiB and the median is 593 bytes. For those, buffering is not the cost —
the hot path's own floor is several serial D1 reads, which streaming does not
touch. Streaming would pay for the 4.86% tail with an **async-trait migration
of `CasReadHandler`**: the method is sync today, 11 production implementors and
12 production call sites depend on that
(`corelink-adapter-host/src/{brew,cargo,npm,oci,pip}/bridge.rs`,
`corelink-bazel-bridge/src/adapter.rs`, `routes/{admin,cas,cas_erase,
customer_export}.rs`, `adapter_cache.rs`, `storage/r2_s3.rs`), and every one of
them would have to change before a single byte streamed. That is a large,
high-blast-radius refactor bought for the tail of the distribution.

**Decision 4: if streaming is ever revisited, it is size-triggered.** The only
version that pays for itself serves small objects exactly as today and streams
above a threshold. That keeps the digest re-verify intact for the 95% and
confines the integrity question — which ADR-S34-001 answers with the scrubber —
to the tail.

## Consequences

B-051 does not proceed as written; it is re-scoped to the size bound. The
`skipped`-shaped risk it was guarding against is unchanged, because the scrubber
(B-050) now provides at-rest coverage regardless.

A read-side size ceiling is a behaviour change on the serving path: an object
above the threshold that is served today would stop being served, so the
threshold must sit above the observed max (52.3 MB) or the change needs its own
migration story. That is the substantive open question and it is deliberately
left to the implementing item rather than guessed here.

## Alternatives rejected

**Ship streaming as originally scoped.** Rejected on the measured distribution:
it is a large async-trait migration whose benefit lands on 4.86% of objects, and
it removes the read-path digest re-verify for all of them.

**Do nothing, on the grounds that 52.3 MB x 8 is survivable.** Rejected because
the number that makes it survivable is an accident. Nothing prevents a 1 GiB
`_public` mirror blob from being read eight times concurrently; the limit that
allows it was chosen for the mirror's fetch path, not the container's heap.

**Lower `CAS_READ_CONCURRENCY_LIMIT` instead.** Rejected: it degrades the 95% of
reads that are tiny in order to bound the 4.86% that are not. The factor to
bound is the one that varies by three orders of magnitude.
