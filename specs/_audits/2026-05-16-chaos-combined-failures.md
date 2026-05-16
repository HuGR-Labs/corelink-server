# Chaos combined-failure scenarios — Wave-23 audit

- **Date**: 2026-05-16
- **Wave**: R-prep wave-23
- **Branch**: `wt/r-prep-chaos-combined-failures`
- **Crate**: `tests/chaos` (`chaos-campaign`)
- **Companion runbook**: [`specs/_runbooks/RB-CHAOS-CAMPAIGN.md`](../_runbooks/RB-CHAOS-CAMPAIGN.md)
- **Predecessor audit**: [`specs/_audits/2026-05-16-chaos-campaign-harness.md`](2026-05-16-chaos-campaign-harness.md) (wave-22 — 8 isolated scenarios baseline)

## Scope

Wave-22 shipped the `tests/chaos` campaign harness with eight isolated
fail-CLOSED scenarios. Each scenario injects a single failure mode and
asserts the canonical observable triplet (fail-CLOSED, audit event,
alert). The wave-22 dress rehearsal flagged that several pairs of those
single-failure scenarios *interact* under realistic load — a recovery
window for one incident can overlap with the onset of another, and a
naive implementation could mask one of the two fail-CLOSED contracts
under the cover of the other's failure path.

The wave-22 closure deferred those orchestrated combined-failure tests
to wave-23 with explicit caveat. This audit documents the wave-23
delivery: **three combined-failure orchestrated scenarios** added to
the same `chaos-campaign` crate, gated by the same `--features chaos`
opt-in flag, asserting per-pair expected behaviour and recovery
sequence.

## Combined scenario matrix

| Pair | New test file                                                   | Composed from                                  | Failure injection sequence                                                                                                                                | Fail-CLOSED contract                                                                                                                                  | Audit anchors (both must emit)                                                              | Alerts (both must fire)                                                |
| ---- | --------------------------------------------------------------- | ---------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| A    | `campaign_combined_partition_plus_byok_503.rs`                  | Scenario 1 + Scenario 5                        | (1) Partition UsEast; (2) within the failover window, mark all 4 BYOK providers (AWS / GCP / Azure / Vault) 503.                                          | Writes against the partitioned region return 503 **and** every BYOK acquire returns `ProviderUnavailable503` — no plaintext fall-back, no fail-OPEN. | `corelink.failover.region.degraded` (once) + `corelink.byok.provider.unavailable` (≥ once)  | SEV-1 `region_isolated` + SEV-1 `byok_provider_unavailable`            |
| B    | `campaign_combined_d1_exhaustion_plus_stripe_drift.rs`          | Scenario 2 + Scenario 6                        | (1) Saturate the D1 pool; (2) deliver a Stripe webhook with sender timestamp drifted +400 s outside the 300 s replay window.                              | D1 acquire returns 503 with `Retry-After` **and** webhook verifier returns `OutsideReplayWindow` — the verifier rejects on its own merits before any DB acquire. | `corelink.d1.pool.exhausted` (once) + `corelink.billing.webhook.replay_window` (once)        | SEV-2 `d1_pool_saturated` + SEV-2 `stripe_webhook_clock_skew`          |
| C    | `campaign_combined_neon_shadow_failure_plus_audit_export.rs`    | Scenario 3 + in-file `ExportTrailerModel`      | (1) Emit two audit rows under healthy state; (2) inject shadow-sink silent failure; (3) emit three more rows while the export pass is running; (4) seal trailer; (5) reconcile. | R2 archive holds all 5 rows (primary durability intact); the export trailer seals on R2 count alone; shadow sink reflects only the pre-injection 2 rows; reconcile reports drift. | `corelink.audit.shadow_sink.silent_failure` (once)                                          | SEV-2 `audit_shadow_sink_drift`                                        |

Each combined scenario is encoded as a **single** `#[test]` so the
chaos-suite count moves from 13 (wave-22) to 16 (wave-22 + wave-23).

## Per-pair expected behaviour and recovery

### Pair A — Partition + BYOK 503

**Why this pair matters.** Both scenarios independently emit SEV-1
audit anchors and SEV-1 alerts. Production risk is that a single
"degraded mode" code path could short-circuit one of the two checks
(for example, returning a partitioned-region 503 *before* the BYOK
acquire is even attempted, hiding a latent BYOK fail-OPEN). The test
asserts that both fail-CLOSED contracts hold under combined chaos —
the BYOK quorum is queried, returns 503, and the audit anchor lands
regardless of the failover state.

**Recovery sequence.**
1. Heal the partitioned region (auto-failover, ≤ 5 min). Writes resume
   on the recovered region.
2. While BYOK providers remain 503, BYOK acquires continue to fail
   CLOSED — there is no shortcut path that the region recovery
   "rescues". Provider escalation (≤ 15 min) is the canonical
   recovery vector.
3. Once any one provider returns to availability, the preferred-then-
   fallback acquire path succeeds — verified by the test's recovery
   stanza.

**Combined recovery time**: ≤ 15 min (max of region recovery ≤ 5 min
and provider escalation ≤ 15 min; surfaces recover independently).

### Pair B — D1 exhaustion + Stripe webhook drift

**Why this pair matters.** The billing webhook handler and the tenant
request path share the D1 pool. A saturated pool could mask a
replay-window rejection by 503'ing the webhook before its
signature/timestamp check runs. The test pins the contract that the
verifier evaluates signature and timestamp **before** any D1 acquire
happens — the saturated-pool state must not leak into the verifier's
decision.

**Recovery sequence.**
1. D1 surface: auto-scale (≤ 10 min) bumps pool capacity. The post-
   scale fresh pool is verified to acquire without emitting the
   saturation audit event or alert.
2. Stripe surface: NTP fix on the sender drains the drift; Stripe's
   re-delivery handles the rejected webhooks once the clock is in
   bounds (≤ 10 min). Verifier behaviour does not change — replay-
   window enforcement is constant.

**Combined recovery time**: ≤ 10 min (surfaces recover in parallel; no
cross-dependency).

### Pair C — Neon shadow silent failure + audit export

**Why this pair matters.** A silent shadow-sink failure during a
long-running export window risks the export trailer waiting on a sink
that will never catch up. The contract is that the export trailer is
gated on **R2 archive health only** — the shadow sink is best-effort
and its drift is a SEV-2 reconcile-loop signal, not an export blocker.

**Recovery sequence.**
1. R2 archive is the canonical primary; trailer emission proceeds and
   seals immediately on the R2 row count. The test's `seal_trailer()`
   call returns the full 5-row count even mid-chaos.
2. Reconcile pass detects drift between R2 and shadow; SEV-2
   `audit_shadow_sink_drift` fires.
3. Shadow-sink replay job (operator-driven, ≤ 30 min) catches up.
   Post-replay reconcile passes — verified by the test's recovery
   stanza using a fresh `CampaignAuditDualWrite` with both surfaces
   aligned.

**Combined recovery time**: ≤ 30 min (shadow replay catch-up; export
trailer completes on its own schedule, never blocked).

## Test surface

`tests/chaos/tests/` now ships **16 `#[test]` cases**:

- Wave-22 isolated (13 tests across 8 files — unchanged):
  `campaign_network_partition_failover.rs` (1) +
  `campaign_d1_pool_exhaustion_degrades_gracefully.rs` (1) +
  `campaign_neon_shadow_silent_failure_alerts.rs` (1) +
  `campaign_rls_guc_dropout_rejects_insert.rs` (2) +
  `campaign_byok_provider_503_fails_closed.rs` (2) +
  `campaign_stripe_webhook_timestamp_drift_rejected.rs` (2) +
  `campaign_clerk_jwks_rotation_recovers.rs` (2) +
  `campaign_cas_multipart_abort_cleanup.rs` (2)
- Wave-23 combined (3 tests across 3 new files — this audit):
  `campaign_combined_partition_plus_byok_503.rs` (1) +
  `campaign_combined_d1_exhaustion_plus_stripe_drift.rs` (1) +
  `campaign_combined_neon_shadow_failure_plus_audit_export.rs` (1)

All 16 tests are gated by `#[cfg(feature = "chaos")]`. Default
`cargo test -p chaos-campaign` runs zero tests across all 11 test
binaries (each binary's `test result` line reports `0 passed`).

## How to run

```bash
# Full campaign (≈ seconds on a laptop):
cargo test -p chaos-campaign --features chaos --no-fail-fast

# Just the wave-23 combined scenarios:
cargo test -p chaos-campaign --features chaos \
  --test campaign_combined_partition_plus_byok_503 \
  --test campaign_combined_d1_exhaustion_plus_stripe_drift \
  --test campaign_combined_neon_shadow_failure_plus_audit_export

# Default-off behaviour:
cargo test -p chaos-campaign
```

## Charter conformance

- `#![forbid(unsafe_code)]` enforced (crate-level on `tests/chaos/src/lib.rs`)
- no `unwrap` / `expect` / `panic` in library code; test files override
  via `#![allow(clippy::unwrap_used, …, reason = "test code")]`
- default-off feature flag — workspace-default test surface unchanged
- pure in-process synchronous state machines; no `tokio`, no
  testcontainers, no network IO
- DCO sign-off + Co-Authored-By on the SEAL commit

## Why this audit lives in `_audits/`

`_audits/` is excluded from `validate_specs.py::SKIP_ALL` (see
`scripts/validate_specs.py` §`SKIP_ALL = {"_audits", "_archive",
"_schemas", "_compliance"}`); no canonical front matter is required.
The companion runbook `RB-CHAOS-CAMPAIGN.md` carries the front matter
that the spec validator gates on.

## Gates (this commit)

- `cargo build --workspace` — green
- `cargo test -p chaos-campaign --features chaos --no-fail-fast` — 16 tests passing
- `cargo test -p chaos-campaign --no-fail-fast` — 0 tests (default-off)
- `cargo clippy --workspace --all-targets --features chaos -- -D warnings` — clean
- `scripts/validate_specs.py` — green
- `scripts/validate_references.py` — green
