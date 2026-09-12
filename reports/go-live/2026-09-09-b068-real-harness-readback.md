# B-068 production-safe real harness readback

Captured at `2026-09-09T09:11:16Z` from exact code SHA
`64e57a2ccb218f475b44a260b64af95e0bc7df2c`.

## Result

- Static owner packet and adversarial executor verifier: PASS.
- Authenticated production D1 safe subset: 4 PASS, 1 FAIL, 1 deliberately excluded.
- R2 credential-independent fail-closed network targets: 2 PASS; three real-object
  targets BLOCKED before mutation.
- Neon: BLOCKED during pure preflight; no Cargo invocation.
- Stripe: EXCLUDED by the explicit no-billing-mutation boundary.
- PAT seed: EXCLUDED; no signing seed was supplied.

## D1 observations

The following production-safe targets passed against the deployed CONFIG_DB:

- `d1_http_tenant_admin_lookup_round_trip`
- `d1_dpa_and_active_subscription_reads`
- `d1_acquire_lock_then_held_then_release`
- `d1_audit_write_blocking_records_oaudit_phase`

The post-run query found zero synthetic `tier_selection_locks` rows for correlation
`corr-d1-acquire-lock`.

`d1_http_cas_meta_round_trip` failed with Cloudflare D1 error 7500, `no such table:
cas_meta`. This is a harness defect, not a migration omission: no checked-in D1
migration owns `cas_meta`, and `cas_meta_lookup` has no caller outside this ignored
test. Creating an unused production table to satisfy the stale probe would be the
wrong repair.

`d1_persist_free_active_does_not_count_as_a_subscription` was not run. It writes a
`tier_selections` row and explicitly documents that rows remain in a throwaway test
database. Running it against production would violate the no-billing-mutation scope
and leave test state behind.

## R2 and Neon observations

`r2_cas_list_durable_audit_failure_precedes_storage` and
`r2_cas_exists_batch_fails_closed_on_bad_audit_creds` passed. They prove that a
rejected audit write fails closed before R2 dispatch; they are not substitutes for
live-object evidence.

The live R2 preflight stopped with exit 2 because no R2 S3 access key is available.
No dedicated `R2_TEST_BUCKET` is configured. Production execution is additionally
unsafe today because `storage_r2_round_trip` and
`cas_idempotent_rewrite_reports_durable_false` create unique objects without deleting
them.

The Neon preflight stopped with exit 2 because `NEON_TEST_DSN` is absent. GitHub has
only `production` and `staging` environments; the workflow-referenced
`real-integration` environment does not exist. The sole prior dispatch, run
`34265159776`, ended in `startup_failure` with zero jobs.

## Required repairs

1. Remove or replace the dead `cas_meta` target with a migration-owned D1 behavior.
2. Keep mutable D1 tests on a dedicated disposable database and assert zero residual
   rows.
3. Add exact-key cleanup to every live R2 write test, including failure paths, then
   provision a dedicated test bucket and scoped credentials.
4. Provision the protected `real-integration` environment and a disposable Neon
   branch before the next single dispatch.

No provider object, billing row, Neon row, or PAT seed was created by this run.
