# Cargo census certification — 2026-09-23

## Scope and source anchors

This is a tracked-manifest census of the campaign checkout, with a read-only comparison to the fetched `origin/main`. It is not a build, runtime, deployment, or production-reachability claim.

| Ref | Commit |
|---|---|
| Campaign checkout `HEAD` | `3df52eb71acdd4e00084d42f0b64191674bf42eb` |
| Fetched `origin/main` at readback | `e0f231110524fe81ed0b8d3451903879daf9d519` |
| Historical campaign baseline | `cca798ff5bc2df660ecf2570ed243eb9775ff3d0` |
| Registry `observed_main` value | `91630baebe3ae7abe686cd4e06a5621ecdc4ab73` |

The registry's `observed_main` is stale relative to the fetched `origin/main`; refresh/reconcile it before freezing or publishing. The branch is heavily diverged (`HEAD` 629 commits ahead and 380 behind `origin/main` at readback), so campaign-checkout evidence must not be represented as a merge to current `main`.

## Census result

`git ls-tree -r` finds **107 tracked `Cargo.toml` paths** at `HEAD`, `origin/main`, and the historical baseline. The path sets are identical at all three refs. Relative to the historical baseline, ten manifest blobs changed by `origin/main`; their package names and explicit target declarations are unchanged. These changes are version/metadata/dependency/description changes, not population changes. The current campaign checkout has the corresponding historical versions of those manifests.

| Classification | Manifests | Packages | Reason / disposition |
|---|---:|---:|---|
| Workspace | 83 | 83 | Declared root workspace members outside `tests/`; eligible first-party package identities. |
| Test-only | 12 | 12 | Declared workspace members under `tests/`; these are named Cargo packages and remain eligible ownership units. |
| Independent fuzz | 10 | 10 | Tracked fuzz manifests outside the root workspace; each defines its own package/workspace and is eligible. |
| Archive | 1 | 1 | `_archive/wi-s11-002-partial/Cargo.toml`, package `corelink-erasure`; retained historical package, excluded from the eligible population by archive location. |
| Other — workspace root | 1 | 0 | Root `Cargo.toml` declares the workspace and has no `[package]`; not a package identity. |
| Fixture | 0 | 0 | No tracked fixture-only Cargo manifest found. |
| Vendor | 0 | 0 | No tracked vendored Cargo manifest found. |
| Generated | 0 | 0 | No tracked generated Cargo manifest found. |
| **Total** | **107** | **106** | **105 eligible + 1 archived package; the root manifest is not a package.** |

There is no unclassified tracked Cargo manifest in this census. The archive package was parsed directly from its tracked manifest because Cargo metadata rejects it as belonging to the root workspace without being declared/excluded there; its manifest is under `_archive` and it is not in the root member list. It declares package `corelink-erasure` and an explicit `prop_erasure` test target; the source tree supplies its default library target.

## Package identity reconciliation

Read-only `cargo metadata --no-deps --offline --format-version 1` resolved **95 root workspace packages** and their targets. Running the same metadata command separately for each of the ten independent fuzz manifests resolved **10 more packages** and their targets. The resulting 105 `(manifest path, package name)` pairs match `docs/ownership/registry.json` exactly:

- missing from registry: **0**;
- registry identities without a corresponding current manifest: **0**;
- package-name mismatches: **0**.

Package names are manifest-derived; directory names were not used as identity. Examples that demonstrate why this matters: `crates/corelink-container/Cargo.toml` is package `corelink-server`; `crates/tenant-path/Cargo.toml` is package `corelink-tenant-path`; `tools/cli/Cargo.toml` is package `corelink-cli`; and `tests/chaos/Cargo.toml` is package `chaos-campaign`.

Cargo metadata reported **631 targets** across the 105 eligible packages. Target-kind counts are: 404 tests, 85 libraries, 79 examples, 39 binaries, 15 benches, 6 `cdylib`, 3 `rlib`, 3 custom build targets, and 2 `staticlib` target kinds. A target can carry more than one kind, so kind counts sum above the number of target records. The metadata output includes exact target names and kinds; notable multi-target records include `corelink-server` (library, two binaries, custom build script and tests), `corelink-cli` (library, `corelink` binary, examples and tests), and five separate `corelink-cli-fuzz` binaries.


## Eligible package and exact target inventory

The table records every eligible manifest, Cargo package name, and exact target name/kind from the read-only metadata selection above.

| Manifest | Package | Targets (name and kind) |
|---|---|---|
| `apps/migrate-single-to-multi-region/Cargo.toml` | `migrate-single-to-multi-region` | migrate-single-to-multi-region (bin) |
| `crates/corelink-ac/Cargo.toml` | `corelink-ac` | corelink_ac (lib); ac_core_canonical_vectors (test); ac_core_canonical_vectors_sig (test); ac_core_key_rotation_sig (test); ac_core_prop_merkle (test); ac_core_prop_sig (test); ac_core_round_trip (test); ac_core_tampering (test); ac_core_timing_sig (test); schema_idempotency_canonical (test); schema_migration_canonical (test); schema_prop_ac_schema (test) |
| `crates/corelink-ac/fuzz/Cargo.toml` | `corelink-ac-fuzz` | hkdf_expand (bin) |
| `crates/corelink-adapter-host/Cargo.toml` | `corelink-adapter-host` | corelink_adapter_host (lib); brew_adversarial (test); brew_common (test); brew_prop_url_normalize (test); brew_smoke (test); cargo_adversarial (test); cargo_common (test); cargo_prop_translate (test); cargo_smoke (test); npm_adversarial (test); npm_common (test); npm_prop_metadata (test); npm_smoke (test); oci_adversarial (test); oci_blob_dos (test); oci_common (test); oci_prop_digest (test); oci_prop_manifest (test); oci_smoke_pull (test); oci_smoke_push (test); oci_token_post (test); pip_adversarial (test); pip_prop_index_parse (test); pip_smoke (test) |
| `crates/corelink-adapters-cloud/Cargo.toml` | `corelink-adapters-cloud` | corelink_adapters_cloud (lib) |
| `crates/corelink-adapters-vault/Cargo.toml` | `corelink-adapters-vault` | corelink_adapters_vault (lib) |
| `crates/corelink-analytics/Cargo.toml` | `corelink-analytics` | corelink_analytics (lib); migration_canonical_0015 (test); prop_analytics (test) |
| `crates/corelink-audit-chain/Cargo.toml` | `corelink-audit-chain` | corelink_audit_chain (lib); verifier (bin); mutation_kills (test); neon_shadow (test); neon_shadow_real (test); prop_audit_chain (test); jcs_canonicalize (bench); merkle_append (bench) |
| `crates/corelink-audit-chain/fuzz/Cargo.toml` | `corelink-audit-chain-fuzz` | jcs_canonicalize (bin); merkle_append (bin) |
| `crates/corelink-audit/Cargo.toml` | `corelink-audit` | corelink_audit (lib); anomaly_emit (example); chain_hash_compute (example); emit_token_validated (example); redaction_macros (example); canonical_vectors (test); prop_audit (test); redaction (test) |
| `crates/corelink-auth/Cargo.toml` | `corelink-auth` | corelink_auth (lib); webauthn_cross_browser_test (example); webauthn_passkey_enroll (example); webauthn_recovery_flow (example); webauthn_yubikey_admin_op (example); schema_integration_dsr_pat_export (test); schema_migration_canonical (test); schema_mutation_kills (test); schema_prop_schema (test); webauthn_adversarial (test); webauthn_canonical_vectors (test); webauthn_prop_webauthn (test) |
| `crates/corelink-bazel-bridge/Cargo.toml` | `corelink-bazel-bridge` | corelink_bazel_bridge (lib); integration (test) |
| `crates/corelink-billing-aggregator/Cargo.toml` | `corelink-billing-aggregator` | corelink_billing_aggregator (lib); prop_billing_aggregator (test) |
| `crates/corelink-billing-emit/Cargo.toml` | `corelink-billing-emit` | corelink_billing_emit (lib); prop_billing_emit (test) |
| `crates/corelink-billing-reconcile/Cargo.toml` | `corelink-billing-reconcile` | corelink_billing_reconcile (lib); billing-reconcile-run (bin); billing_reconcile_run_bin (test); prop_billing_reconcile (test) |
| `crates/corelink-billing-stripe-materializer/Cargo.toml` | `corelink-billing-stripe-materializer` | corelink_billing_stripe_materializer (lib); materializers_e2e (test); wasm32_binders (test) |
| `crates/corelink-billing-stripe-traits/Cargo.toml` | `corelink-billing-stripe-traits` | corelink_billing_stripe_traits (lib) |
| `crates/corelink-billing-stripe/Cargo.toml` | `corelink-billing-stripe` | corelink_billing_stripe (lib); prop_billing_stripe (test) |
| `crates/corelink-billing/Cargo.toml` | `corelink-billing` | corelink_billing (lib); abuse_calibration_abuse (test); abuse_migration_canonical_0013 (test); abuse_prop_abuse (test); quota_cas_migration_canonical_0012 (test); quota_cas_prop_quota_cas (test); quota_core_migration_canonical_0009 (test); quota_core_prop_quota (test); quota_fsm_prop_quota_fsm (test); replay_prop_billing_replay (test) |
| `crates/corelink-byok/Cargo.toml` | `corelink-byok` | corelink_byok (lib); read_aws (example); unwrap_dek (example); wrap_dek (example); write_aws (example); byok_aws_e2e_kms (test); byok_aws_real_unit (test); byok_aws_unit (test); byok_aws_wasm32_stub (test); byok_azure_real_unit (test); byok_azure_wasm32_stub (test); byok_core_adversarial (test); byok_core_matrix_framework (test); byok_core_mutation_kills (test); byok_core_prop_byok (test); byok_gcp_real_unit (test); byok_gcp_wasm32_stub (test); byok_revocation_adversarial (test); byok_revocation_prop_revocation (test); byok_revocation_wiring (test); byok_vault_real_unit (test); byok_vault_wasm32_stub (test); matrix (test); matrix_adversarial (test); matrix_prop (test); envelope_roundtrip (bench) |
| `crates/corelink-byok/fuzz/Cargo.toml` | `corelink-byok-fuzz` | envelope_roundtrip (bin); wrapped_dek_parse (bin) |
| `crates/corelink-cas/Cargo.toml` | `corelink-cas` | corelink_cas (lib); chunker_custom_config (example); chunker_fastcdc_optin (example); chunker_fixed_default (example); chunker_streaming (example); manifest_build_and_verify (example); manifest_sig_domain_separation (example); manifest_streaming_verify (example); manifest_tampering_detection (example); chunker_bounds_enforcement (test); chunker_canonical_vectors (test); chunker_mutation_kills (test); chunker_prop (test); dedup_prop (test); edge_migration_canonical_0011 (test); edge_prop (test); lru_tracker_prop (test); manifest_canonical_vectors (test); manifest_prop (test); manifest_streaming_memory (test); manifest_tampering (test); multipart_schema_idempotency_canonical (test); multipart_schema_migration_canonical (test); multipart_schema_mutation_kills (test); multipart_schema_prop (test) |
| `crates/corelink-cf-bindings/Cargo.toml` | `corelink-cf-bindings` | corelink_cf_bindings (cdylib, rlib); d1_real (test); do_real (test); kv_real (test); prop_cas_idempotency (test) |
| `crates/corelink-chaos-scheduler/Cargo.toml` | `corelink-chaos-scheduler` | corelink_chaos_scheduler (lib); adversarial_steady_state_breach (test); catalog_inventory (test); prop_chaos_scheduler (test) |
| `crates/corelink-clerk-cf/Cargo.toml` | `corelink-clerk-cf` | corelink_clerk_cf (cdylib, rlib); audit_sink_mutex_poison_telemetry (test); cf_worker_prefetch_wire (test); clerk_health_do (test); prod_wiring (test); prop_clerk_health_logic (test); tenant_region_wire (test) |
| `crates/corelink-clerk/Cargo.toml` | `corelink-clerk` | corelink_clerk (lib); basic (example); multi_issuer (example); rotation (example); adversarial (test); http_fetcher (test); mutation_kills (test); prop_validate (test); rotation (test); principal_id_clone (bench) |
| `crates/corelink-client-verify/Cargo.toml` | `corelink-client-verify` | corelink_client_verify (rlib, cdylib, staticlib); disable_verify (example); verify_simple (example); verify_stream (example); abi_smoke (test); prop_verify (test); verify_bench (bench) |
| `crates/corelink-client-verify/fuzz/Cargo.toml` | `corelink-client-verify-fuzz` | verify_ffi (bin); verify_sync (bin) |
| `crates/corelink-config-do/Cargo.toml` | `corelink-config-do` | corelink_config_do (lib); prop_cas (test) |
| `crates/corelink-container/Cargo.toml` | `corelink-server` | corelink_server (lib); corelink-server (bin); corelink-gc-sweep-production (bin); admin_ac_route_smoke (test); admin_pilot (test); audit_export (test); audit_outbox_residency_regression (test); byok_orchestrator (test); cargo_route_smoke (test); cas_route_smoke (test); gc_binary_wiring (test); money_path_auth_wiring (test); scope_catalog_closure (test); signup_pilot (test); signup_pilot_live_d1 (test); webhook_unified (test); build-script-build (custom-build) |
| `crates/corelink-core/Cargo.toml` | `corelink-core` | corelink_core (lib) |
| `crates/corelink-crypto/Cargo.toml` | `corelink-crypto` | corelink_crypto (lib) |
| `crates/corelink-dpa-acceptance/Cargo.toml` | `corelink-dpa-acceptance` | corelink_dpa_acceptance (lib); common (test); integration_acceptance_flow (test); prop_idempotency (test); prop_jwt_signature (test); prop_locale_mismatch (test); prop_replay_protection (test); accept_and_verify_jwt (bench) |
| `crates/corelink-dsr-statuspage-scheduler/Cargo.toml` | `corelink-dsr-statuspage-scheduler` | corelink_dsr_statuspage_scheduler (lib); dsr_statuspage_cron (test); prop_outcome_json_roundtrip (test) |
| `crates/corelink-dsr/Cargo.toml` | `corelink-dsr` | corelink_dsr (lib); prop_dsr (test) |
| `crates/corelink-dt-webhook/Cargo.toml` | `corelink-dt-webhook` | corelink_dt_webhook (lib); dlq_replay (example); mock_cve_inject (example); webhook_handle (example); adversarial_dt_webhook (test); prop_dt_webhook (test) |
| `crates/corelink-dual-approval/Cargo.toml` | `corelink-dual-approval` | corelink_dual_approval (lib); audit_inspection (example); basic_2admin (example); collusion_demo (example); adversarial (test); mutation_kills (test); mutation_kills_v2 (test); prop_dual_approval (test) |
| `crates/corelink-enterprise-inquiry/Cargo.toml` | `corelink-enterprise-inquiry` | corelink_enterprise_inquiry (lib); prop_enterprise_inquiry (test) |
| `crates/corelink-erasure-attestation/Cargo.toml` | `corelink-erasure-attestation` | corelink_erasure_attestation (lib); list_public_keys (example); lookup_attestation (example); sign_attestation (example); verify_offline (example); adversarial (test); prop_attestation (test) |
| `crates/corelink-eviction/Cargo.toml` | `corelink-eviction` | corelink_eviction (lib); migration_canonical_0008 (test); prop_eviction (test) |
| `crates/corelink-failover-router/Cargo.toml` | `corelink-failover-router` | corelink_failover_router (lib); failback_outbox_drain (test); prop_failover (test) |
| `crates/corelink-gc/Cargo.toml` | `corelink-gc` | corelink_gc (lib); gc_sweep (bin); cron_tick_simulation (example); degrade_mode_abort (example); idempotent_resume (example); manual_admin_trigger (example); mark_phase_walk (example); chaos_gc_scheduler (test); gc_sweep_bin (test); migration_canonical (test); migration_canonical_0007 (test); prop_inv_gc_004_race (test); prop_mark (test); prop_reconcile (test); prop_scheduler (test); prop_sweep (test) |
| `crates/corelink-handler-ac/Cargo.toml` | `corelink-handler-ac` | corelink_handler_ac (lib); prop_handler_ac (test) |
| `crates/corelink-handler-admin/Cargo.toml` | `corelink-handler-admin` | corelink_handler_admin (lib); prop_handler_admin (test) |
| `crates/corelink-handler-cas-erase/Cargo.toml` | `corelink-handler-cas-erase` | corelink_handler_cas_erase (lib); prop_handler_cas_erase (test) |
| `crates/corelink-handler-cas/Cargo.toml` | `corelink-handler-cas` | corelink_handler_cas (lib); mutation_kills (test); prop_handler_cas (test) |
| `crates/corelink-handler-customer/Cargo.toml` | `corelink-handler-customer` | corelink_handler_customer (lib); handler_customer (test); mutation_kills (test) |
| `crates/corelink-hash/Cargo.toml` | `corelink-hash` | corelink_hash (lib); blake3_vectors (example); blob_store_contract (test); mutation_kills (test); prop_hash (test); blake3 (bench); blake3_bench (bench) |
| `crates/corelink-hash/fuzz/Cargo.toml` | `corelink-hash-fuzz` | digest_parse (bin); verify_body (bin) |
| `crates/corelink-meta/Cargo.toml` | `corelink-meta` | corelink_meta (lib); prop_audit_outbox (test); prop_refcount (test); schema_canonical (test); tombstone_semantics (test) |
| `crates/corelink-meta/fuzz/Cargo.toml` | `corelink-meta-fuzz` | audit_idempotency (bin); commit_put_roundtrip (bin) |
| `crates/corelink-ops/Cargo.toml` | `corelink-ops` | corelink_ops (lib); corelink-supply-verify (bin); rb_fm_201_dry_run (bin); rb_fm_205_dry_run (bin); rb_fm_206_dry_run (bin); admin_api_audit_inspection (example); admin_api_basic_2admin (example); admin_api_collusion_demo (example); config_api_cas_retry (example); config_api_crud_basic (example); config_api_rollback_drill (example); deploy_audit_fail_closed (example); deploy_unsigned_deploy_blocked (example); deploy_verify_signed_deploy (example); rotation_worker_audit_chain_rotation (example); rotation_worker_byok_rotation_stub (example); rotation_worker_pat_rotation (example); rotation_worker_tdk_rotation (example); supply_chain_verify_lookup (example); supply_chain_verify_paranoid_mode (example); supply_chain_verify_verify_basic (example); survey_sign_and_record (example); admin_api_cross_wi_integration_s13 (test); admin_api_e2e_admin_dual_approval (test); admin_dry_run_prop_dual_approval_invariants (test); config_api_adversarial (test); config_api_prop_mfa_freshness (test); deploy_adversarial (test); deploy_chaos (test); deploy_prop_verify (test); dr_backup_verify_prop_backup_verify (test); dr_backup_verify_sli_binding (test); dr_drill_prop_dr_drill (test); drata_proptest_idempotency (test); drata_wiremock_drata_client (test); migrations_d1_migration_integration (test); migrations_prop_migration_additivity (test); oncall_pagerduty_events_http (test); oncall_prop_oncall (test); rotation_worker_adversarial (test); rotation_worker_prop_rotation (test); supply_chain_policy_adversarial_dep_policy (test); supply_chain_policy_e2e_dependabot (test); supply_chain_policy_prop_cargo_deny (test); supply_chain_verify_adversarial (test); supply_chain_verify_prop_verify (test); survey_prop_survey (test); tenant_offboarding_prop_tenant_offboarding (test) |
| `crates/corelink-pat/Cargo.toml` | `corelink-pat` | corelink_pat (lib); mint_and_verify (example); scope_check (example); adversarial (test); canonical_vectors (test); constant_time (test); emit_e2e_seed (test); mutation_kills (test); prop_pat (test) |
| `crates/corelink-privacy-erasure-worker/Cargo.toml` | `corelink-privacy-erasure-worker` | corelink_privacy_erasure_worker (lib); chaos_per_backend_failure (test); integration_erasure_lifecycle (test); prop_idempotency_replay_100x (test); prop_pseudonymization_correctness (test); regression_refcount_aware_scrub (test); regression_stripe_invoice_preserved (test); sli_binding (test); verification_job_24h (test) |
| `crates/corelink-privacy-pseudonymize/Cargo.toml` | `corelink-privacy-pseudonymize` | corelink_privacy_pseudonymize (lib); pseudonymization_invariants (test) |
| `crates/corelink-privacy/Cargo.toml` | `corelink-privacy` | corelink_privacy (lib); breach_prop_breach_emit_escalation_matrix (test); breach_regression_audit_fail_closed_behavior (test); consent_chaos_audit_emit_failure (test); consent_integration_consent_lifecycle (test); consent_prop_hmac_roundtrip (test); consent_prop_idempotency_replay (test); consent_regression_locale_enforce (test); consent_regression_notice_version (test); consent_regression_symmetric_schema (test); dpa_versioning_integration_dpa_lifecycle (test); dpa_versioning_prop_grace_boundary (test); dpa_versioning_prop_re_accept_idempotency (test); dpa_versioning_prop_read_only_enforcement (test); dpa_versioning_regression_bump_kind (test); notice_prop_audit_fail_closed (test); notice_prop_notice_hash_determinism (test); notice_regression_3locale_sync (test); residency_integration_residency_routing (test); residency_property_region_pinning_30k (test); residency_property_residency_20k (test); residency_region_adversarial (test); residency_regression_d1_check_constraints (test); sub_processor_prop_sub_processor_emit (test) |
| `crates/corelink-r2-multipart/Cargo.toml` | `corelink-r2-multipart` | corelink_r2_multipart (lib); concurrency_limit (example); cross_tenant_replay (example); happy_path (example); orphan_sweeper (example); chaos_r2_multipart (test); failover_inventory (test); prop_r2_multipart (test) |
| `crates/corelink-rate-headers/Cargo.toml` | `corelink-rate-headers` | corelink_rate_headers (lib); migration_canonical_0014 (test); prop_rate_headers (test) |
| `crates/corelink-ratelimit/Cargo.toml` | `corelink-ratelimit` | corelink_ratelimit (lib); eviction_reset_bypass (test); migration_canonical_0010 (test); mutation_kills (test); prop_ratelimit (test) |
| `crates/corelink-reapi/Cargo.toml` | `corelink-reapi` | corelink_reapi (lib); batch_read_blobs_e2e (test); canonical_vectors (test); capabilities (test); find_missing_handler_e2e (test); handler_e2e (test); integration_bit_rot (test); prop_cas (test); prop_cas_read (test); prop_cross_tenant_read (test); prop_find_missing_batch (test); prop_idempotency (test); read_handler_e2e (test); timing_padding_grpc_e2e (test); build-script-build (custom-build) |
| `crates/corelink-reapi/fuzz/Cargo.toml` | `corelink-reapi-fuzz` | audit_request_id_total (bin); parse_read_resource_name (bin); proto_decode_batch_update (bin) |
| `crates/corelink-region/Cargo.toml` | `corelink-region` | corelink_region (lib); chaos_region_outage (test); d1_replica_lag_sli (test); do_sync_age_sli (test); kv_propagation_sli (test); neon_replica_lag_sli (test); prop_region_invariants (test); r2_crr_sli (test); sli_binding (test) |
| `crates/corelink-replica-worker/Cargo.toml` | `corelink-replica-worker` | corelink_replica_worker (lib); adversarial (test); coverage_sli (test); prop_replica (test); sli_binding (test); sli_emit (test) |
| `crates/corelink-replication-coordinator/Cargo.toml` | `corelink-replication-coordinator` | corelink_replication_coordinator (lib); prop_coordinator (test); split_brain_reject (test) |
| `crates/corelink-replication/Cargo.toml` | `corelink-replication` | corelink_replication (lib); rollout_controller_budget_exceeded (example); rollout_controller_manual_abort (example); rollout_controller_start_rollout (example); rollout_controller_adversarial (test); rollout_controller_prop_rollout (test) |
| `crates/corelink-rotation-adapters/Cargo.toml` | `corelink-rotation-adapters` | corelink_rotation_adapters (lib); prop_rotation_invariants (test) |
| `crates/corelink-runbook-tracker/Cargo.toml` | `corelink-runbook-tracker` | corelink_runbook_tracker (lib) |
| `crates/corelink-runner-aggregate/Cargo.toml` | `corelink-runner-aggregate` | corelink_runner_aggregate (lib); runner-aggregate-run (bin); runner_aggregate_run_bin (test) |
| `crates/corelink-runner-overage/Cargo.toml` | `corelink-runner-overage` | corelink_runner_overage (lib) |
| `crates/corelink-signup/Cargo.toml` | `corelink-signup` | corelink_signup (lib); chaos_stripe_outage (test); mutation_kills (test); prop_signup_orchestration (test); orchestrator (bench) |
| `crates/corelink-slack-real/Cargo.toml` | `corelink-slack-real` | corelink_slack_real (lib); prop_slack_emit_atomic (test); wiremock_real_client (test) |
| `crates/corelink-slo/Cargo.toml` | `corelink-slo` | corelink_slo (lib); prop_slo (test) |
| `crates/corelink-statuspage-real/Cargo.toml` | `corelink-statuspage-real` | corelink_statuspage_real (lib); dsr_publish (test); prop_statuspage_invariants (test); wasm32_backend (test) |
| `crates/corelink-stripe-real/Cargo.toml` | `corelink-stripe-real` | corelink_stripe_real (lib); checkout_promo (test); direct_proxy (test); live_integration (test); prop_dlq (test); prop_portal (test); prop_webhook (test); wallet_broker_proxy (test); webhook_e2e (test); webhook_verify (bench) |
| `crates/corelink-telemetry/Cargo.toml` | `corelink-telemetry` | corelink_telemetry (lib); migration_canonical_0016 (test); pii_redaction_100k_synthetic (test); prop_canary (test); prop_logpush (test); prop_otel_export (test); prop_synthetic_pager (test) |
| `crates/corelink-terraform-drift-consumer/Cargo.toml` | `corelink-terraform-drift-consumer` | corelink_terraform_drift_consumer (lib); adversarial (test) |
| `crates/corelink-tier-selection/Cargo.toml` | `corelink-tier-selection` | corelink_tier_selection (lib); mutation_kills (test); prop_tier_selection (test); select (bench) |
| `crates/corelink-tracing/Cargo.toml` | `corelink-tracing` | corelink_tracing (lib); prop_tracing (test) |
| `crates/corelink-transparency-log/Cargo.toml` | `corelink-transparency-log` | corelink_transparency_log (lib); witness_attestation (example); adversarial (test); prop_witness (test) |
| `crates/corelink-turbo-bridge/Cargo.toml` | `corelink-turbo-bridge` | corelink_turbo_bridge (lib); integration (test) |
| `crates/corelink-wasm/Cargo.toml` | `corelink-wasm` | corelink_wasm (cdylib) |
| `crates/corelink-worker/Cargo.toml` | `corelink-worker` | corelink_worker (lib); ac_handler_e2e (test); adversarial_reproducible (test); auth_middleware_smoke (test); canonical_keys (test); integration_r2 (test); prop_ac_full (test); prop_ac_handlers (test); prop_ac_ttl (test); prop_auth_full (test); prop_auth_middleware (test); prop_multipart_full (test); prop_neg_cache (test); prop_r2_path (test); prop_reproducible (test); prop_revocation (test); prop_split_splice (test); reapi_v2_ac_conformance (test); reapi_v2_split_splice_conformance (test); timing_indistinguishability (test); side_channel (bench) |
| `crates/corelink-worker/fuzz/Cargo.toml` | `corelink-worker-fuzz` | r2_path (bin); r2_put_get_roundtrip (bin) |
| `crates/tenant-path/Cargo.toml` | `corelink-tenant-path` | corelink_tenant_path (lib); edge_parity_vectors (test); prop_tenant_path (test); derive (bench); derive_prefix_cached (bench); derive_prefix_v2 (bench) |
| `crates/tenant-path/fuzz/Cargo.toml` | `corelink-tenant-path-fuzz` | derive_prefix (bin); derive_prefix_extended (bin) |
| `tests/chaos/Cargo.toml` | `chaos-campaign` | chaos_campaign (lib); campaign_byok_provider_503_fails_closed (test); campaign_cas_multipart_abort_cleanup (test); campaign_clerk_jwks_rotation_recovers (test); campaign_combined_d1_exhaustion_plus_stripe_drift (test); campaign_combined_neon_shadow_failure_plus_audit_export (test); campaign_combined_partition_plus_byok_503 (test); campaign_d1_pool_exhaustion_degrades_gracefully (test); campaign_neon_shadow_silent_failure_alerts (test); campaign_network_partition_failover (test); campaign_rls_guc_dropout_rejects_insert (test); campaign_stripe_webhook_timestamp_drift_rejected (test) |
| `tests/e2e-billing-flow/Cargo.toml` | `e2e-billing-flow` | e2e_billing_flow (lib); end_to_end (test) |
| `tests/e2e-byok-revoke/Cargo.toml` | `e2e-byok-revoke` | e2e_byok_revoke (lib); adversarial_race_condition (test); adversarial_stampede (test); adversarial_transient_api_error (test); happy_revoke_flow (test); live_provider_gated (test); multi_provider_matrix (test); prop_fail_closed (test); recovery_flow (test) |
| `tests/e2e-chaos/Cargo.toml` | `e2e-chaos` | e2e_chaos (lib); adversarial_chaos_staging_only (test); adversarial_ops_exclusivity (test); adversarial_sev1_drill_pause (test); chaos_clerk_outage_grace_period (test); chaos_cpu_pressure_sustained (test); chaos_dns_failure_dual_resolver_failover (test); chaos_kv_d1_cold_start_below_sla (test); chaos_latency_injection_p99_bounded (test); chaos_memory_pressure_oomk_avoided (test); chaos_network_partition_recovers (test); chaos_r2_disk_fill_quarantine (test); prop_seed_replay_identical (test) |
| `tests/e2e-dsr/Cargo.toml` | `e2e-dsr` | e2e_dsr (lib); adversarial_erasure_partial_failure (test); adversarial_mfa_failed (test); adversarial_receipt_replay_after_expiry (test); adversarial_receipt_signature_invalid (test); adversarial_sla_breach (test); happy_access (test); happy_erasure (test); happy_objection (test); happy_portability (test); happy_rectification (test); happy_restriction (test); prop_dsr_invariants (test) |
| `tests/e2e-failover-router/Cargo.toml` | `e2e-failover-router` | e2e_failover_router (lib); scenarios (test) |
| `tests/e2e-pilot-onboarding/Cargo.toml` | `e2e-pilot-onboarding` | e2e_pilot_onboarding (lib); test_01_tenant_signup (test); test_02_first_cas_upload (test); test_03_audit_export_roundtrip (test); test_04_dsr_erasure (test); test_05_tenant_offboarding (test) |
| `tests/e2e-replication-failover/Cargo.toml` | `e2e-replication-failover` | e2e_replication_failover (lib); scenarios (test) |
| `tests/e2e-resilience/Cargo.toml` | `e2e-resilience` | e2e_resilience (lib); scenarios (test) |
| `tests/e2e-signup-flow/Cargo.toml` | `e2e-signup-flow` | e2e_signup_flow (lib); adversarial_dpa_not_accepted (test); adversarial_webhook_signature_invalid (test); happy_path_free (test); happy_path_starter_stripe_test_mode (test); idempotency_replay (test); prop_atomic_invariants (test) |
| `tests/e2e-tenant-isolation/Cargo.toml` | `e2e-tenant-isolation` | e2e_tenant_isolation (lib); adversarial (test) |
| `tests/e2e-user-journeys/Cargo.toml` | `e2e-user-journeys` | e2e-user-journeys (bin) |
| `tools/cli/Cargo.toml` | `corelink-cli` | corelink_cli (lib); corelink (bin); quickstart_audit (example); quickstart_dsr_submit (example); quickstart_get (example); quickstart_list (example); quickstart_put (example); quickstart_signup (example); quickstart_stats (example); audit_export (test); cli_telemetry_optin (test); integration (test); release_workflow_contract (test); verify_ndjson_http (test) |
| `tools/cli/fuzz/Cargo.toml` | `corelink-cli-fuzz` | auth_resolution (bin); cli_input (bin); config_toml (bin); json_deserialize (bin); secret_redaction_check (bin) |
| `tools/dt-cli/Cargo.toml` | `corelink-dt-cli` | corelink-dt-cli (bin) |
| `tools/dt-reconcile/Cargo.toml` | `corelink-dt-reconcile` | corelink-dt-reconcile (bin) |
| `tools/openapi/Cargo.toml` | `corelink-openapi` | corelink_openapi (lib) |
| `tools/sbom-publish/Cargo.toml` | `sbom-publish` | sbom_publish (lib); sbom-publish (bin); generate (example); ingest_dt_retry (example); validate_ntia (example); adversarial (test); prop_sbom (test) |
| `tools/sdks/go/Cargo.toml` | `corelink-go` | corelink_go (cdylib, staticlib) |
| `tools/sdks/python/Cargo.toml` | `corelink-py` | corelink_py (cdylib); build-script-build (custom-build) |

## Reproduction and limits

Commands run from `/private/tmp/corelink-ownership-campaign`:

```text
git ls-tree -r --name-only HEAD
git ls-tree -r --name-only origin/main
git ls-tree -r --name-only cca798ff5bc2df660ecf2570ed243eb9775ff3d0
cargo metadata --no-deps --offline --format-version 1 --manifest-path Cargo.toml
cargo metadata --no-deps --offline --format-version 1 --manifest-path <each independent fuzz Cargo.toml>
```

No dependency graph was resolved (`--no-deps`), no compilation/test/fuzz command ran, and no runtime or deployment reachability was inferred. Target declarations and metadata are census facts only. Since current `origin/main` differs from the campaign checkout on ten manifest contents and the registry anchor is stale, refresh the census against an explicitly selected frozen commit before treating it as final publication evidence.
