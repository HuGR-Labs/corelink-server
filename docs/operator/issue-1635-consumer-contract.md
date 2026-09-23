# Issue #1635 consumer acknowledgement contract

The canonical server route is `POST /internal/v1/billing/usage`. On a
record-processed response it returns the exact JSON shape emitted by
`IngestResponse` in `crates/corelink-container/src/routes/billing_ingest.rs`:

```json
{
  "outcomes": [
    { "index": 0, "idem_key": "<canonical-key-or-null>", "outcome": "accepted" },
    { "index": 1, "idem_key": "<canonical-key-or-null>", "outcome": "rejected", "reason": "bad_tenant_id" }
  ],
  "accepted": 1,
  "deduped": 0,
  "rejected": 1,
  "total": 1
}
```

Top-level keys are exactly `outcomes`, `accepted`, `deduped`, `rejected`, and
`total`. Each outcome has exactly `index`, `idem_key`, and `outcome`, with
`reason` present only for `rejected` and `conflict`. Outcome order and zero-based
indices match the submitted JSON array. `idem_key` is the lowercase canonical
64-character key when one can be read from the submitted record, even if a
different field makes that record invalid; otherwise it is `null`.

The only outcome names are `accepted`, `deduped`, `rejected`, and `conflict`.
`accepted` and `deduped` do not serialize a reason. `rejected` carries the
server's stable validation reason. `conflict` carries `payload_mismatch` or
`existing_fingerprint_unverifiable`. Only `accepted` and exact `deduped`
outcomes are settleable. Rejected and conflicting records are not settled.

Counters count their matching per-record outcomes. `total` is
`accepted + deduped`; rejected and conflicts are excluded. `total` plus
`rejected` plus the number of conflicts covers every outcome.

HTTP status is part of the acknowledgement contract:

- `409 Conflict` when any outcome is `conflict`, including a mixed batch.
- `422 Unprocessable Entity` when the batch has rejected records and no
  accepted or deduped records (therefore no conflicts).
- `202 Accepted` otherwise, including accepted/deduped records mixed with
  rejected records.
- A persistence failure returns `503 Service Unavailable` with no acknowledgement
  body. The batch result is uncertain, so the consumer retries idempotently and
  does not settle any record from that response.

Malformed request JSON, an empty batch, or an oversized batch is a separate
batch-level `400` response, not a per-record acknowledgement. Authentication
failure is `401`. Neither response settles records.

The credentialless validator and hosted fixtures check this contract without
calling CoreLink, GitHub, or a provider. The fixture is a literal server wire
receipt; the request fixture lets the validator check outcome indices and
canonical idempotency keys against submitted input. This documents the server
producer contract. It does not claim that the private runners repository has
been executed.

## Durable conflict recovery and migration

Migration `0134_usage_event_staging_conflicts.sql` is additive. It never
rewrites `usage_event_staging`, the identity coordinate, the winning quantity,
or an aggregation watermark. There is deliberately no backfill: a historical
conflicting request can only be identified when it is received again and its
incoming fingerprint is available for comparison.

For a `409` conflict or a rejected-only `422`, the consumer leaves the relevant
settlement marker unset. An operator uses the tenant-scoped conflict coordinate
and reason to open a reconciliation case with the sender, preserving the stored
winner as the billing record. The conflicting payload is neither replayed under
the same identity nor passed to the aggregator or a payment provider. Any
financial adjustment follows the normal billing reconciliation process after
human review; this ingest recovery path performs no Stripe operation.

A bodyless `503` is ambiguous rather than a rejection: retry the unchanged
record with the same identity. The durable winner will then classify it as an
exact `deduped` replay or a `conflict`; do not write a settlement marker until
the typed acknowledgement permits it.
