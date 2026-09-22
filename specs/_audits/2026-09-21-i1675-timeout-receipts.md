# i1675 timeout receipt suite

The campaign lane for issue #1675 runs exactly:

```sh
python3 scripts/test_i1675_timeout_receipts.py
```

The lane may invoke `scripts/classify_timeout_receipt.py` after each job's
instrumentation post-step. It must retain the resulting
`corelink.actions-timeout-receipt.v1` JSON artifact. The classifier uses typed
evidence only: elapsed time, `exit 124`, or an absent post-step cannot by
themselves establish a declared timeout or runner loss.

The required matrix covers a declared timeout, cancelled job, lost heartbeat,
missing post-step, SIGTERM, exit 124, ordinary command failure, success, and
conflicting runner/cancellation signals. The test's mutation cases must remain
in the campaign lane so deleting the timeout marker, changing the observation
schema, weakening positive integer validation, or accepting a non-boolean
heartbeat does not silently produce a causal receipt.
