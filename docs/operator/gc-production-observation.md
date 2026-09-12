# B-071 production GC observation

This procedure measures reclaimable storage in production without granting any
delete authority. It must run inside the deployed container (or against the
exact extracted image) so `/usr/local/bin/corelink-gc-sweep-production` and the
recorded immutable image digest refer to the same artifact.

Prepare a regular JSON file with explicit, owner-approved scopes. Discovery is
deliberately not supported:

```json
{
  "schema_version": 1,
  "scopes": [
    {
      "tenant_id": "ee30f7ba-fc25-4d71-939e-ebe130b4c6a3",
      "region": "sam",
      "run_id": "11111111-2222-4333-8444-555555555555",
      "bucket": "corelink-cas-prod-sam"
    }
  ]
}
```

Each run ID must already identify a `running` production `gc_run` in
`physical_delete` for exactly that tenant and region. The observation does not
create or advance a run. `syd` is rejected until its macro-residency mapping is
provisioned. Duplicate tenant/region scopes and mutable image tags are rejected.

Supply a D1 read-only Cloudflare token through `CF_API_TOKEN`, plus
`CLOUDFLARE_ACCOUNT_ID`, `D1_DATABASE_ID`, and `R2_TDK_HEX`. The collector
passes only those required values and certificate/PATH settings to the child.
It strips any inherited live-delete confirmation and all R2 credentials, then
forces `GC_OBSERVATION_ONLY=true` and `GC_LIVE_DELETE=false` for every scope.

```sh
python3 scripts/collect_b071_gc_observation.py collect \
  --scopes /secure/path/b071-scopes.json \
  --image-digest 'registry.cloudflare.com/account/image@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef' \
  --operator 'operator@example.com' \
  --output evidence/owner-actions/B-071/gc-production-dry-run.json
```

The output is written atomically with mode `0600`. Collection fails without an
artifact if any child exits non-zero, emits anything other than one structured
report, crosses the 250-row bound or phase budget, mismatches its explicit
scope, reports unbalanced classifications, or reports any deletion.

The generated package has `approval.status=PENDING_OWNER_REVIEW` and
`live_delete_authorized=false`. The reviewer may record `APPROVED` or
`REJECTED`, their identity, and the decision time, but this B-071 package can
never authorize live deletion. Verify the result with:

```sh
python3 scripts/collect_b071_gc_observation.py verify \
  --evidence evidence/owner-actions/B-071/gc-production-dry-run.json \
  --expect-approval pending
```

Use `--expect-approval approved` only after the human review fields are present.
Approval means the measurement was reviewed; a first destructive execution
still requires a separate implementation and authorization.
