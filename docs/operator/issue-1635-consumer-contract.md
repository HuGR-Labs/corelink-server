# Issue #1635 consumer acknowledgement contract

The server acknowledgement is per record. A runner consumer may write a
settlement marker only for `accepted` or `deduped` records. A `conflicting`,
`rejected`, or `ambiguous` record must remain retryable or quarantined and must
not be counted as settled.

The credentialless validator in
`scripts/verify_i1635_consumer_contract.py` checks the contract without calling
CoreLink, GitHub, or any provider. It requires a versioned acknowledgement with
unique event IDs, typed status reasons, and an ordered `settlement_event_ids`
list that exactly matches the settleable outcomes. This is a producer/consumer
boundary check; it does not claim that the private runners repository has been
executed.

The contract is intentionally suitable for mixed batches. For example, a batch
containing `accepted`, `deduped`, `conflicting`, `rejected`, and `ambiguous`
records may settle only the first two IDs, in their original order. A transport
failure with an unknown server result is `ambiguous` and must not create a
settlement marker.
