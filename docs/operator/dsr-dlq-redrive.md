# DSR DLQ redrive authority

`POST /internal/dsr/dlq/redrive` is the only supported operator recovery path
for a terminal DSR DLQ receipt. It accepts a dedicated bearer credential,
opaque `op_…` and `apr_…` references in `x-corelink-operator-ref` and
`x-corelink-approval-ref`, and this bounded body only:

```json
{"event_id":"dsr-erasure-dlq:<64 lowercase hex characters>"}
```

The Worker reads the receipt-owned envelope, not caller-selected DSR fields.
It derives the salt in the Worker, restores `subject_id` from the stored tenant,
preserves the stored legal-hold and queue timestamp, and sends exactly one
`_dlq_requeue: 1` message. The redrive credential is distinct from queue,
shared internal, erase, and PagerDuty credentials. Never use D1 or a queue
console to reconstruct a message.

Ownership: the on-call privacy operator supplies the approved opaque references;
the Worker owns the D1 claim, queue submission, and hourly cleanup. A provider,
queue, or D1 console operator must not bypass this route.

The receipt is atomically claimed before the queue call. A successful send is
recorded as `submitted`; an exception, a stale claim lease, or a post-send
receipt failure is `ambiguous` and must not be retried automatically. The
redacted audit retains only opaque event, actor/approval references, state, and
time. It contains no raw JSON, salt, Clerk identity, provider payload, or
credential.

Envelopes expire after seven days. The hourly Worker cleanup converts stale
claims to `ambiguous`, removes expired envelopes, and removes redacted audit
metadata after 30 days. A `410 receipt_expired` response is final: open a new
supported DSR intake and retain the incident record; do not manually recreate
or replay the old queue payload.
