# Chaos campaign harness — Wave-22 audit

- **Date**: 2026-05-16
- **Wave**: R-prep wave-22
- **Branch**: `wt/r-prep-chaos-campaign`
- **Crate**: `tests/chaos` (`chaos-campaign`)
- **Companion runbook**: [`specs/_runbooks/RB-CHAOS-CAMPAIGN.md`](../_runbooks/RB-CHAOS-CAMPAIGN.md)

## Scope

A new `tests/chaos` test crate ships an **opt-in chaos engineering
campaign harness** that complements (does not replace) the existing
`tests/e2e-chaos` scheduler-level chaos drill suite and
`tests/e2e-resilience` cross-failure harness.

Where `e2e-chaos` drives the `corelink-chaos-scheduler` and asserts the
**scheduler-level** invariants (steady-state hypothesis, audit
lifecycle, rollback ≤ 5 min, ops exclusivity), the campaign harness
asserts the **product-level fail-CLOSED observable triplet** at the
operation surface that the on-call actually touches during a real
incident:

1. The operation fails CLOSED (no silent success path).
2. An audit event lands at the documented taxonomy node.
3. The documented SEV-1 / SEV-2 / INFO alert (or SLO counter) fires.

The harness is **default-off** behind the `chaos` Cargo feature flag —
`cargo test --workspace` skips it. `cargo test --workspace --features
chaos` (or `cargo test -p chaos-campaign --features chaos`) runs the
campaign.

## Scenario matrix

| # | Scenario                                  | Failure injected                              | Fail-CLOSED assertion                                                       | Audit event                                  | Alert                                | Crate cross-reference                                                                 |
| - | ----------------------------------------- | --------------------------------------------- | --------------------------------------------------------------------------- | -------------------------------------------- | ------------------------------------ | ------------------------------------------------------------------------------------- |
| 1 | Network partition between regions         | 5-minute cross-region link drop               | Read-only mode in degraded region; writes return 503                        | `corelink.failover.region.degraded`          | SEV-1 `region_isolated`              | `corelink-failover-router::InMemoryFailoverRouter` (RB-ACTIVE-FAILOVER)               |
| 2 | D1 connection pool exhausted              | All N pool slots taken; new acquire times out | Request returns 503 with `Retry-After` header; no partial transaction       | `corelink.d1.pool.exhausted`                 | SEV-2 `d1_pool_saturated`            | `apps/server` D1 binding + `corelink-rate-headers` Retry-After                        |
| 3 | Neon shadow sink silent failure           | Sink returns Ok but persists nothing          | Reconcile flags drift; R2 archive (primary) keeps accepting rows            | `corelink.audit.shadow_sink.silent_failure` | SEV-2 `audit_shadow_sink_drift`      | `corelink-audit-chain::InMemoryNeonShadowSink::inject_failure` (RB-AUDIT-EXPORT-INTEGRITY)    |
| 4 | RLS GUC dropped mid-tx                    | `app.current_tenant` unset                    | `WITH CHECK` rejects INSERT; no row visible to any tenant                   | `corelink.rls.guc.dropped`                   | SEV-1 `rls_policy_violation_attempt` | `corelink-tenant-config` + migrations RLS policies                                    |
| 5 | BYOK provider 503                         | AWS/GCP/Azure/Vault all 503                   | Route layer 503; encrypt/decrypt fail-CLOSED; no plaintext leaked           | `corelink.byok.provider.unavailable`         | SEV-1 `byok_provider_unavailable`    | `corelink-byok` provider trait, all 4 in-memory provider fakes                        |
| 6 | Stripe webhook clock skew                 | Sender clock skewed > 300 s                   | Handler rejects; signature considered invalid; no state mutation            | `corelink.billing.webhook.replay_window`     | SEV-2 `stripe_webhook_clock_skew`    | `corelink-billing-stripe` webhook verifier (RB-BILLING-WEBHOOK-DRIFT)                 |
| 7 | Clerk JWKS rotation mid-request           | JWKS endpoint serves new kid mid-flight       | Verifier re-fetches; request verified post-rotation; no cached-stale verdict | `corelink.clerk.jwks.rotated`                | INFO `clerk_jwks_rotation`           | `corelink-clerk` JWKS cache                                                           |
| 8 | CAS multipart abort mid-upload            | Client aborts multipart stream                | `complete` rejected; staged parts GC'd; manifest never seals                | `corelink.cas.multipart.aborted`             | INFO `cas_multipart_aborted`         | `corelink-r2-multipart` + `corelink-manifest`                                         |

## Test surface

`tests/chaos/tests/` ships **12 `#[test]` cases** across **8 scenario
files** (some scenarios pin both the chaos path and a control / variant
case to make the fail-CLOSED contract precise):

- `campaign_network_partition_failover.rs` — 1 test
- `campaign_d1_pool_exhaustion_degrades_gracefully.rs` — 1 test
- `campaign_neon_shadow_silent_failure_alerts.rs` — 1 test
- `campaign_rls_guc_dropout_rejects_insert.rs` — 2 tests
- `campaign_byok_provider_503_fails_closed.rs` — 2 tests
- `campaign_stripe_webhook_timestamp_drift_rejected.rs` — 2 tests
- `campaign_clerk_jwks_rotation_recovers.rs` — 2 tests
- `campaign_cas_multipart_abort_cleanup.rs` — 2 tests

All tests are gated by `#[cfg(feature = "chaos")]` so default
`cargo test --workspace` produces zero tests for the harness.

## Invariants verified

- **INV-CHAOS-CAMPAIGN-FAIL-CLOSED** — every chaos-injected operation
  fails CLOSED: no plaintext leak, no partial transaction visible, no
  cached-stale auth verdict, no silent success.
- **INV-CHAOS-CAMPAIGN-AUDIT-TAXONOMY** — every fail-CLOSED path emits
  the documented `corelink.<surface>.<verb>.<reason>` taxonomy event
  exactly once. Verified via `assert_audit_emitted_once`.
- **INV-CHAOS-CAMPAIGN-ALERT-FIRES** — every fail-CLOSED path raises the
  documented SEV-1 / SEV-2 / INFO alert. Verified via
  `assert_alert_fired`.
- **INV-CHAOS-CAMPAIGN-PRIMARY-DURABLE** — R2 archive (canonical audit
  primary) continues to accept rows even when the Neon shadow sink
  silently fails (scenario 3).
- **INV-CHAOS-CAMPAIGN-RECOVERY** — recovery paths are unit-asserted:
  partition heal restores writes (scenario 1), GUC restore re-enables
  INSERT (scenario 4), JWKS re-fetch verifies the rotated token
  (scenario 7), multipart abort is idempotent (scenario 8).

## Charter compliance

- `#![forbid(unsafe_code)]` at lib + every test file.
- No `unwrap` / `expect` / `panic` in library code; per-test files set a
  bounded `#![allow(clippy::unwrap_used, …)]` scope with `reason =
  "test code"` annotations (workspace-standard idiom).
- All scenarios are pure in-process state machines; no `tokio`
  multi-threaded runtime, no testcontainers, no network IO.
- Each scenario test runs in well under 2 minutes individually; the
  full opt-in suite finishes in seconds on a developer laptop.

## Gates run

| Gate                                                          | Status |
| ------------------------------------------------------------- | ------ |
| `cargo build -p chaos-campaign`                               | green  |
| `cargo test -p chaos-campaign --features chaos --no-fail-fast` | green  |
| `cargo test -p chaos-campaign` (default — zero tests)         | green  |
| `cargo clippy -p chaos-campaign --all-targets --features chaos -- -D warnings` | green  |
| `cargo build --workspace`                                     | green  |
| `validate_specs.py` + `validate_references.py`                | green  |

## Caveats / known follow-ups

1. **Mini-models, not crate bindings.** The harness ships
   self-contained shapes (`CampaignFailoverModel`, `CampaignD1Pool`,
   `CampaignAuditDualWrite`, `CampaignRlsTable`, `CampaignByokModel`,
   `CampaignWebhookVerifier`, `CampaignClerkJwks`, `CampaignMultipart`)
   rather than binding to the production trait surfaces. This keeps the
   campaign decoupled from refactors and asserts the contract at the
   *observable triplet* level. A later wave can replace each mini-model
   with the matching in-memory production fake (`InMemoryFailoverRouter`,
   `InMemoryNeonShadowSink`, etc.) when the integration cost is paid.
2. **Stripe scenario** asserts the verifier contract; the route-layer
   integration test (signature + idempotency cache) remains in
   `crates/corelink-billing-stripe-materializer`.
3. **Scenarios are independent** — no orchestrated sequencing across
   them. Combined-failure cases (e.g. partition + audit sink down at
   once) are deferred to wave-23 if cross-failure interactions surface
   during the dress rehearsal.

## Cross-references

- Companion runbook: [`specs/_runbooks/RB-CHAOS-CAMPAIGN.md`](../_runbooks/RB-CHAOS-CAMPAIGN.md)
- Adjacent harnesses:
  - `tests/e2e-chaos` — chaos-scheduler experiment lifecycle
  - `tests/e2e-resilience` — combined-failure resilience matrix
  - `tests/e2e-failover-router` — failover-router split-brain absence
- Adjacent runbooks: `RB-ACTIVE-FAILOVER`, `RB-AUDIT-EXPORT-INTEGRITY`,
  `RB-COLD-RESTORE-FROM-ZERO`, `RB-CHAOS-CATALOG`.
