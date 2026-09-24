# corelink-server current-main source readback — 2026-09-22

## Source and method

Source pin: `origin/main@140e16eab6315bfdec1a0e4a9892d8557a071781`
(`fix(b035): complete TLS surface audit (#2056)`). The object was available locally and
the files below were read directly with `git show <pin>:<path>`; no branch movement or
checkout was used for this readback.

Inspected: `crates/corelink-container/Cargo.toml`, `src/main.rs`,
`src/routes/build.rs`, `src/routes/dsr/adapter_d1.rs`,
`src/routes/dsr/adapter_d1_registry.rs`, `src/routes/dsr/adapter_d1_tests.rs`,
and migrations `migrations/d1/0133` through `0138`.

This is source inspection only. No Rust build or test, Cargo graph resolution, server
boot, HTTP request, mount, D1/R2/Stripe/KMS call, sweep, or deployment was executed.
The campaign checkout is not assumed to contain this source tree; all material claims
below come from direct object reads at the pinned commit. A later readback of
`origin/main@f5c963f22d2c8f5a332f6f0e970b397da09adebd` found material changes in the
server pilot source tree, so this record is historical SOURCE evidence for the pinned
object and is not a current-main equivalence claim.

## Manifest inventory

The package is `corelink-server` in `crates/corelink-container/Cargo.toml`. The manifest
declares the `corelink-server` and `corelink-gc-sweep-production` binaries; Cargo's
automatic integration-test discovery has 13 top-level test targets (14 Rust files when
the shared `tests/harness` helper is counted). The manifest has seven feature choices
including `default`: `byok-aws-real`, `byok-gcp-real`, `byok-azure-real`,
`byok-vault-real`, `cf-r2-real`, and `cf-billing-real`. Every feature is disabled by
default. This manifest inventory does not establish which artifact was built or shipped.

## Composition root and adapters

`src/main.rs` validates the container capacity, selects storage from environment,
applies production gates, constructs the audit and BYOK collaborators, then serves the
composed Axum router on `PORT` (default 50051). It adds the failover heartbeat, `/_health`
and a 10 MiB global body limit; Turbo's router supplies its own larger route limit.
The source also conditionally adds internal PAT mint, auth introspection, usage billing
ingest, DSR, audit drain, CAS-attempt audit, audit archive, CAS scrub, tenant-quota read,
public attestation, CAS erase, public revocation, public mirror, tier selection, DPA
acceptance, Stripe webhook, and DSR anchor routers. These builders gate their routes on
their respective keys and storage/adapters. A source call site does not prove that a
gate succeeded or a route mounted at runtime.

`src/routes/build.rs` composes 15 routers for CAS, AC, admin, tenant detail, pilot admin,
BYOK admin, audit export/analytics, users, customer, DSR portal, customer runners,
workspaces, Bazel v2, and Turbo v8. Signup is environment gated. Cargo, Brew, npm, OCI,
and pip adapters are conditionally wired from PAT and storage configuration; npm also
requires its KV, and OCI requires its manifest KV and token key. Residency, failover,
rate limit, optional OTel, and origin timing wrap the data-plane router. R2/D1 native
clients, in-memory test/development paths, Stripe materializer, and BYOK providers are
selected in source; their live use was not observed.

## DSR migration reconciliation

| Migration | Table(s) | Tenant scope in SQL | Classification in source |
|---|---|---|---|
| 0133 `stripe_webhook_effect_ledger` | `stripe_webhook_event_effects` | no `tenant_id` | outside `ALL_TENANT_KEYED_TABLES` |
| 0134 `usage_event_staging_conflicts` | `usage_event_staging_conflicts` | `tenant_id` | `TENANT_ID_TABLES` erase-by-tenant |
| 0135 `runner_aggregate_durable_state` | `runner_period_terms_snapshot`, `runner_aggregate_event_claim` | `tenant_id` | `RETAIN_SET` billing evidence |
| 0136 `storage_mutation_liability` | `storage_mutation_liability` | `tenant_id` | `TENANT_ID_TABLES` erase-by-tenant |
| 0137 `runner_checkout_attempts` | `runner_checkout_attempts` | `tenant_id` | `TENANT_ID_TABLES` erase-by-tenant |
| 0138 `dsr_dlq_delivery_receipts` | `dsr_dlq_delivery_receipts` | no `tenant_id` | outside `ALL_TENANT_KEYED_TABLES` |

The DSR registry's five buckets are tenant-id erase, namespace erase, retain,
CAS-plane-owned, and special erase. The completeness check enforces exactly one bucket
for each member of `ALL_TENANT_KEYED_TABLES` before DSR operations. The named regression
`runner_billing_tables_have_exact_dsr_dispositions` checks the 0134/0135/0137 tables;
`runner_billing_migration_tables_remain_classified` pins those migration/table pairs.
The broad migration-drift tests are present in source. None of these tests was executed.

## Status

Static source relationships and manifest declarations are read back at the pin above.
Resolved Cargo consumers, Rust validation, actual target/feature selection, environment
gates, mounted routes, adapter credentials, remote behavior, and production runtime remain
unknown or unobserved.
