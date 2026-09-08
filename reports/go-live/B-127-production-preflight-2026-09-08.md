# B-127 production preflight — migration held

Date: 2026-09-08

This note records redacted, read-only preflight evidence. No historical
`audit_outbox` row was read, updated, deleted, or rewritten.

## D1 migration state

`d1 migrations list CONFIG_DB --env prod --remote` reported exactly one
pending migration:

```text
0107_audit_outbox_tenant_residency_guard.sql
```

The migration was not applied because the deployed writer artifact does not
contain the tenantless-writer repair.

## Deployed writer/container state

The Cloudflare Containers API reported all five production writer services on
the same image tag `ddd95560-r1`:

```text
corelink-prod-corelinkserver-prod
corelink-prod-sam-corelinkserver-prod-sam
corelink-prod-lhr-corelinkserver-prod-lhr
corelink-prod-nrt-corelinkserver-prod-nrt
corelink-prod-syd-corelinkserver-prod-syd
```

The image was repinned by commit `0d6e97c80` on 2026-08-30. The deployed
artifact's source still contains:

```text
COALESCE((SELECT primary_region FROM tenant WHERE tenant_id = ?2), 'wnam')
```

The required repair landed later in `45ed60def` (2026-09-06): a missing
tenant remains `NULL` so migration 0107 rejects the write. `origin/main`
contains the equivalent source repair in its D03 bundle (`a65c7d7ca`), but the
currently deployed image predates both repairs.

## Required order

1. Build and deploy a new common writer image from the current `origin/main`
   source, with a new tag, to all five services above.
2. Prove the running artifact contains the bare tenant-region subquery and no
   tenantless `COALESCE` fallback in the production writer path.
3. Re-run the read-only migration-list preflight; only then apply 0107 through
   the migration ledger.
4. Re-run the aggregate B-127 verifier. Historical orphan rows remain
   unchanged and may continue to keep the verifier in `FAILED` until the
   unexplained residual is separately accounted for.
