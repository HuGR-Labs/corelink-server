# RB-AUDIT-RESIDENCY-THIRD-STATE — full-population audit residency check

Run the read-only control before a residency attestation or after an audit/DSR
incident. It does not alter D1, audit evidence, tenant rows, or erasure state.

```bash
python3 scripts/verify_audit_residency.py \
  --environment production \
  --database-id "$CORELINK_PROD_D1_DATABASE_ID"
```

`CLOUDFLARE_ACCOUNT_ID` and `CLOUDFLARE_API_TOKEN` must already be in the
environment. The command neither reads `.env.local` nor writes a secret. For a
reviewable rerun, save the raw API response in an approved restricted evidence
store and use `--input /restricted/path/d1-residency.json`; do not commit it.

## State and exit contract

| State | Exit | Meaning | Operator action |
| --- | ---: | --- | --- |
| `COMPLIANT` | 0 | Every non-empty audit row is `satisfied`. | Archive the JSON output with the change/attestation record. |
| `FAILED` | 1 | At least one known mismatch (`violated`) or unprovable row (`unevaluable`) exists. | Do not attest compliance. Preserve evidence and investigate the named buckets. |
| `INDETERMINATE` | 2 | Credentials, timeout, response shape, control, or full-population partition cannot be trusted. | Restore read access/query health and rerun; never interpret this as zero violations. |

The three buckets are disjoint and exhaustive over the `audit_outbox` left
population. A row is `satisfied` only when a joined tenant and both canonical
regions exist and are equal. A known unequal pair is `violated`. A missing
tenant, missing region, or malformed/unknown region is `unevaluable`.

## Mandatory controls

Inspect the JSON `counts` object. It includes the full denominator and each
partition, DSR-erasure evidence, unexplained orphan denominator, and `weur`
evidence. In production `erasure_log_rows=0` is indeterminate: without the
control, the tool cannot distinguish retained Art. 5(2) DSR evidence from an
unexplained orphan. `weur_audit_rows > 0` with `weur_tenants=0` must remain
visible through `weur_orphan_rows`; it cannot become a green result.

Erased tenants remain legitimate retained audit evidence. They still fail this
*residency proof* because their `primary_region` no longer exists, while their
retention/erasure semantics remain unchanged. The unexplained-orphan counts are
separate specifically so they cannot disappear from the denominator.
