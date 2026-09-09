# B-127 production preflight — migration held

Date: 2026-09-08

This note records redacted, read-only preflight evidence. No historical
`audit_outbox` row was read, updated, deleted, or rewritten.

## D1 migration state

The read-only `d1 migrations list CONFIG_DB --env prod --remote` snapshot
explicitly reported `0107_audit_outbox_tenant_residency_guard.sql` as pending.
That snapshot is not a complete receipt for the current migration population:
the target tree now contains the contiguous follow-on files `0108` through
`0116`, and this repository has no committed production-ledger evidence that
those files were applied in order. Until a fresh read-only ledger listing proves
otherwise, the deployment gate must treat the whole range `0107–0116` as
pending (not only `0107`):

```text
0107_audit_outbox_tenant_residency_guard.sql
0108_team_member_invitation_security.sql
0109_audit_chain_epoch_contract.sql
0110_audit_chain_epoch_row_metadata.sql
0111_b076_payable_subscription_ownership.sql
0112_githugr_tenant_org_map.sql
0113_clerk_provisioning_lock.sql
0114_byok_revocation_customer_audit_atomic.sql
0115_gc_purge_fence.sql
0116_synthetic_page_delivery_lifecycle.sql
```

The migration range was not applied because the deployed writer artifact does
not contain the tenantless-writer repair. The list above is the conservative
repository-side pending set; it must not be mistaken for a provider query.

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
3. Re-run the read-only migration-list preflight; only then apply the pending
   `0107–0116` range through the migration ledger, in order.
4. Re-run the aggregate B-127 verifier. Historical orphan rows remain
   unchanged and may continue to keep the verifier in `FAILED` until the
   unexplained residual is separately accounted for.
