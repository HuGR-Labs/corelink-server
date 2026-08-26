# ADR — the F2 flip needs an audit seam the edge does not have yet

**Date:** 2026-08-26
**Status:** Accepted — blocks F2
**Supersedes nothing.** Addendum to
`docs/design/2026-08-25-adr-edge-native-find-missing.md`, which already states
the invariant this document makes implementable: *"The audit result gates the
response. No probe result reaches the caller unless its audit rows committed."*

## The finding

F1 shipped the measurement half (#1340) and proved the answer: 43/43 parity,
8.65 s → 2.10 s at n=100. Reading that as "F2 is a flag flip" is wrong, and the
shadow could not have caught it — a shadow that serves nothing has nothing to
fail open.

`probeMissingAtEdge` (`worker/src/lib/edge_find_missing.ts`) computes the same
SET as the container. It does not do the other four things the container's
`exists_batch` does (`crates/corelink-container/src/storage/r2_s3.rs:1560`):

1. **N `ReadAttempted` rows into `audit_outbox`**, one per digest, with the
   per-digest tenant/hash/principal fields — the rows the S-09 chain drains.
2. **`ReadDenied`, audited, before any dispatch** on a cross-tenant digest. The
   whole slice is scanned first, so one poisoned digest cannot let the others
   touch R2.
3. **Fail-closed coupling.** `if let Err(e) = audit_result { return
   Err(CasHandlerError::AuditFailed(e)) }` — the probe results are already in
   hand and are still thrown away. Audit down ⇒ the surface does not answer.
4. **The SLI observations** `AvailCasGet` / `LatencyCasGetP99`.

Flipping F2 against today's code would serve `findMissingBlobs` with the audit
trail silently absent, and would keep serving it while the audit sink is down.
That is not an observability gap; `findMissingBlobs` is a REAPI read surface and
those rows are the evidence.

## The seam — one container call, not a second implementation

The tempting fix is to write the `audit_outbox` rows straight from the Worker
over the D1 binding. Rejected. The row is a contract: UUIDv7 `id`, a CloudEvents
1.0 `payload_json`, `UNIQUE (request_id, event_type)` dedup, and a region column
a table trigger (`trg_audit_outbox_region_match_insert`) will `RAISE(ABORT)` on
if it disagrees with the row's tenant. Porting that to TypeScript creates a
second author of a compliance-critical row shape, and the two WILL drift — the
edge would keep passing its own tests while emitting rows the drain rejects.

Instead: **the edge probes R2 natively and awaits ONE container call that emits
the batch**, before it responds.

- New internal route `POST /_internal/audit/cas-attempted`, alongside the
  existing `/_internal/audit/{drain,archive}`, taking `{tenant, principal,
  caller_tenant, at_unix_ms, digests[]}` and emitting through the SAME
  `D1AuditOutboxSink::emit_cas_batch_async` the container already uses. One
  author of the row shape, forever.
- The edge **awaits** it. Non-2xx, timeout, or transport error ⇒ the edge
  discards its probe results and falls through to the container, which will
  attempt the audit itself and fail closed in its own taxonomy. Uncertainty
  never becomes a served answer.
- Cross-tenant is decided at the edge before any `head()` is issued, and a
  denial is reported by falling through to the container rather than by the edge
  inventing the `ReadDenied` row — same reason as above.

### Why this keeps the win

The 8.6 s was N probes to R2 over the public S3 endpoint from a 0.25-vCPU box.
The audit was never the cost: #1328 already collapsed it to ONE batched write.
The seam adds one round trip to our own origin (~tens of ms) to a path that
drops from ~8650 ms to ~2100 ms at n=100. It does not restore the per-digest
constant that made the container slow, because nothing in it is per-digest on
the wire.

### The SLIs

Emitted by the container on the audit call, tagged as an edge-served probe, so
`AvailCasGet` does not develop a hole shaped exactly like our fastest path.

## Consequences

- F2 is **not** a flag flip. It is: the internal route, the awaited call, the
  fallthrough-on-audit-failure test, and only then the flag.
- `EDGE_FIND_MISSING=shadow` stays valid and unchanged; the shadow is not on the
  serve path and needs no audit.
- F3 (red-team) runs against the serving code, after this lands — cross-tenant,
  BYOK-active, revoked PAT, spoofed instance, over-cap, and **audit-sink-down**,
  which is now a named case rather than an accident.
