# B-063 partition replay evidence

This repository check is the offline evidence boundary for issue #1648. It
does not contact D1, R2, the container endpoint, GitHub, or PagerDuty. An
authorized operator may save one redacted read-only D1 population as:

```json
{
  "partition": {"tenant_id": "<stable-redaction>", "region": "enam"},
  "rows": []
}
```

Each row must retain `id`, `tenant_id`, `region`, `sequence_number`,
`prev_hash`, `chain_hash`, `enqueued_at`, `canonical_jcs`, `algorithm_id`,
`epoch_id`, and `link_key_id`. Payload bytes stay in the operator's ephemeral
input and are not committed as evidence.

Run:

```sh
python3 scripts/verify_b063_archive_replay.py --input /path/to/redacted.json
```

The verifier applies the production reader's deterministic
`ORDER BY sequence_number, id`, recomputes the BLAKE3 link from
`prev_hash || canonical_jcs`, checks contiguous sequence and tenant/region
isolation, and reports zero D1/R2 writes. `drainable` is exit 0;
`quarantine_required` is exit 1 and identifies the first break; malformed,
mixed-tenant, or hash-invalid input is `fail_closed` with exit 2.

This artifact cannot close B-063/#1648. Closure still requires the issue's
three distinct zero-failure production population reads and three complete
green `audit-archive-lag` runs after an authorized repair.
