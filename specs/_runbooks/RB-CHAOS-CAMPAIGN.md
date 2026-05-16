---
id: "RB-CHAOS-CAMPAIGN"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "chaos", "wave-22", "fail-closed", "r-prep"]
---

# RB-CHAOS-CAMPAIGN — Wave-22/23 chaos campaign harness runbook

> Companion audits:
> [`specs/_audits/2026-05-16-chaos-campaign-harness.md`](../_audits/2026-05-16-chaos-campaign-harness.md)
> (wave-22 baseline — 8 isolated scenarios) and
> [`specs/_audits/2026-05-16-chaos-combined-failures.md`](../_audits/2026-05-16-chaos-combined-failures.md)
> (wave-23 — 3 combined-failure orchestrated scenarios).
> Adjacent: `RB-ACTIVE-FAILOVER`, `RB-AUDIT-EXPORT-INTEGRITY`,
> `RB-COLD-RESTORE-FROM-ZERO`, `RB-CHAOS-CATALOG`.

## What this runbook is

`tests/chaos` ships **eight fail-CLOSED chaos scenarios** (wave-22) plus
**three combined-failure orchestrated scenarios** (wave-23), each
verifying the observable triplet (fail-CLOSED, audit event, alert) for
one canonical incident class — or, for the combined scenarios, for a
pair of incidents that interact in production. This runbook tells you:

- how to run the suite
- which scenarios fire which alerts in production
- expected recovery time per scenario
- how to extend the campaign

## How to run

The campaign is **opt-in** behind a Cargo feature flag — default
`cargo test --workspace` does not run it.

```bash
# Run the full campaign (≈ seconds on a laptop):
cargo test -p chaos-campaign --features chaos --no-fail-fast

# Run a single scenario:
cargo test -p chaos-campaign --features chaos \
  --test campaign_rls_guc_dropout_rejects_insert

# Verify default-off behaviour (zero tests, exit 0):
cargo test -p chaos-campaign
```

The harness is pure in-process — no testcontainers, no network IO, no
external infra. CI executes the opt-in suite on demand via the
`workflow_dispatch` matrix; it is not on the default PR gate so a
regression in the campaign cannot block routine merges, but a release
candidate **must** pass it before tagging.

## Scenario → alert → recovery matrix

| # | Scenario                          | Audit event                                  | Alert (severity)                       | Expected detection time | Expected recovery time | First responder                  |
| - | --------------------------------- | -------------------------------------------- | -------------------------------------- | ----------------------- | ---------------------- | -------------------------------- |
| 1 | Network partition between regions | `corelink.failover.region.degraded`          | `region_isolated` (SEV-1)              | ≤ 30 s probe interval   | ≤ 5 min auto-failover  | on-call SRE → RB-ACTIVE-FAILOVER |
| 2 | D1 pool exhausted                 | `corelink.d1.pool.exhausted`                 | `d1_pool_saturated` (SEV-2)            | ≤ 1 min Retry-After     | ≤ 10 min (autoscale)   | on-call SRE                      |
| 3 | Neon shadow sink silent failure   | `corelink.audit.shadow_sink.silent_failure` | `audit_shadow_sink_drift` (SEV-2)      | ≤ 5 min reconcile pass  | ≤ 30 min (replay)      | on-call SRE → RB-AUDIT-EXPORT-INTEGRITY  |
| 4 | RLS GUC dropped mid-tx            | `corelink.rls.guc.dropped`                   | `rls_policy_violation_attempt` (SEV-1) | ≤ 30 s WAF/alert        | ≤ 5 min (rollback)     | on-call security                 |
| 5 | BYOK provider 503                 | `corelink.byok.provider.unavailable`         | `byok_provider_unavailable` (SEV-1)    | ≤ 30 s circuit-breaker  | ≤ 15 min (provider)    | on-call security + provider esc. |
| 6 | Stripe webhook clock skew         | `corelink.billing.webhook.replay_window`     | `stripe_webhook_clock_skew` (SEV-2)    | ≤ 1 min                 | ≤ 10 min (NTP)         | on-call billing                  |
| 7 | Clerk JWKS rotation               | `corelink.clerk.jwks.rotated`                | `clerk_jwks_rotation` (INFO)           | immediate (per-request) | n/a — self-heal        | none — informational             |
| 8 | CAS multipart abort               | `corelink.cas.multipart.aborted`             | `cas_multipart_aborted` (INFO)         | immediate (per-request) | n/a — GC pass          | none — expected on client cancel |

## Combined-failure recovery (wave-23)

The wave-22 dress rehearsal flagged that several pairs of single-failure
scenarios *interact* — one failure can either mask or amplify the other
unless the fail-CLOSED contract is independently asserted on each
surface. Wave-23 pins three orchestrated combinations as `#[test]`
cases. Each combination has a documented expected behaviour and a
documented recovery sequence:

| Pair | Combined chaos                                                  | Expected combined behaviour                                                                                                                                | Combined recovery time                                                                                  | First responder                              |
| ---- | --------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- | -------------------------------------------- |
| A    | Network partition (Scenario 1) + BYOK provider 503 (Scenario 5) | Writes against the partitioned region fail CLOSED **and** every BYOK acquire fails CLOSED. Both SEV-1 audit anchors fire and both SEV-1 alert names emit. | ≤ 15 min (max of region auto-failover ≤ 5 min and provider escalation ≤ 15 min — recover region first). | on-call SRE + on-call security (joint)       |
| B    | D1 pool exhausted (Scenario 2) + Stripe webhook drift (Scenario 6) | D1 surface returns 503 with `Retry-After`; the webhook verifier rejects on signature/timestamp policy independently of D1 state. Both SEV-2 audit anchors and SEV-2 alerts emit per-surface. | ≤ 10 min (D1 autoscale ≤ 10 min; webhook re-delivery handled by Stripe after NTP fix ≤ 10 min — surfaces recover in parallel). | on-call SRE + on-call billing (parallel)      |
| C    | Neon shadow silent failure (Scenario 3) + concurrent audit export | R2 archive (primary) accepts every audit row. The export trailer seals on R2 count alone — shadow-sink health does not gate the export. Reconcile pass detects drift and fires SEV-2 once. | ≤ 30 min (shadow-replay catch-up; export trailer completes on its own schedule). | on-call SRE → RB-AUDIT-EXPORT-INTEGRITY      |

Combined-recovery rules of thumb:

1. **Recover the higher-severity incident first** — Pair A drains the
   region before the BYOK provider escalation completes.
2. **Treat surfaces as independent unless audit/alert evidence ties
   them** — Pair B's webhook verifier and D1 pool MUST surface their own
   SEV-2 anchors; if only one fires, the other surface has silently
   masked the failure (page on this).
3. **R2 is the canonical durability surface** — Pair C never blocks an
   export on Neon shadow health; trailer emission is gated on R2 only.

Each combined scenario is encoded as a single `#[test]` in
`tests/chaos/tests/campaign_combined_*.rs` and is gated by the same
`chaos` feature flag as the wave-22 scenarios. The wave-23 audit
(`specs/_audits/2026-05-16-chaos-combined-failures.md`) documents each
pair's chaos injection sequence, fail-CLOSED contract, audit anchors,
and recovery sequence in detail.

## When an alert fires in production

1. **SEV-1** — page primary on-call. The matching scenario test
   documents the expected fail-CLOSED behaviour: confirm the operation
   is failing CLOSED (not silently succeeding) before any mitigation.
2. **SEV-2** — slack on-call channel. Inspect the audit-event count
   over the last 5 min; if it exceeds the documented baseline the
   incident is escalating.
3. **INFO** — no human action; the system has handled the chaos as
   designed (JWKS rotation, multipart abort GC). These are surfaced for
   audit-trail completeness.

## Extending the campaign

Add a new isolated scenario:

1. Add a mini-model in `tests/chaos/src/lib.rs` exposing
   `inject_<failure>()` plus a `<surface>_outcome()` accessor and audit
   / alert vectors.
2. Add `tests/chaos/tests/campaign_<scenario>.rs` gated by
   `#[cfg(feature = "chaos")]`, asserting the observable triplet.
3. Register the test binary in `tests/chaos/Cargo.toml` `[[test]]`.
4. Append a row to the matrix in
   `specs/_audits/2026-05-16-chaos-campaign-harness.md` and to the
   matrix above.
5. Run `cargo test -p chaos-campaign --features chaos` and
   `cargo clippy -p chaos-campaign --all-targets --features chaos -- -D warnings`.

Add a new combined-failure scenario (wave-23 pattern):

1. Compose two (or more) existing wave-22 mini-models inside the test
   file directly — do **not** add a new model unless one is missing.
2. Save as `tests/chaos/tests/campaign_combined_<pair>.rs`,
   `#[cfg(feature = "chaos")]` gated.
3. Register the test binary in `tests/chaos/Cargo.toml` `[[test]]`.
4. Append a row to §combined-failure-recovery above and to the matrix
   in `specs/_audits/2026-05-16-chaos-combined-failures.md`.
5. Same gates: `cargo test -p chaos-campaign --features chaos` and
   `cargo clippy -p chaos-campaign --all-targets --features chaos -- -D warnings`.

## Charter constraints honoured

- `#![forbid(unsafe_code)]`
- no `unwrap` / `expect` / `panic` in library code
- default-off feature flag — workspace-default test surface unchanged
- pure in-process synchronous state machines; no `tokio`, no
  testcontainers, no network IO
- DCO sign-off + Co-Authored-By on every commit
